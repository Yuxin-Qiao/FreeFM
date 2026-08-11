use crate::domain::{
    Availability, AvailabilityEvidence, PlaylistSummary, Song, availability_evidence,
    availability_from_fields, merge_song_metadata, same_recording_score, sorted_version_markers,
    value_id_ref,
};
use crate::error::{AppError, AppResult};
use crate::protocol::{PLAYLIST_NAME, RemoteApi};
use crate::storage::{StateFile, TrustedStore};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    #[allow(dead_code)]
    AddOriginal,
    TrustedMapping,
    CandidateOnly,
    Skip,
    AlreadyPresent,
    DeferredByBudget,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Candidate {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) duration_delta_ms: Option<i64>,
    pub(crate) album: Option<String>,
    pub(crate) version_markers: Vec<String>,
    pub(crate) score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Decision {
    pub(crate) original_id: String,
    pub(crate) original_title: String,
    pub(crate) original_artist: String,
    pub(crate) original_duration_ms: Option<i64>,
    pub(crate) action: Action,
    pub(crate) availability: Availability,
    pub(crate) availability_evidence: AvailabilityEvidence,
    pub(crate) selected_id: Option<String>,
    pub(crate) selected_title: Option<String>,
    pub(crate) selected_artist: Option<String>,
    pub(crate) selected_duration_ms: Option<i64>,
    pub(crate) selected_album: Option<String>,
    pub(crate) selected_version_markers: Vec<String>,
    pub(crate) trusted_mapping: bool,
    pub(crate) trusted_invalid: bool,
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PlanReport {
    pub(crate) ok: bool,
    pub(crate) command: String,
    pub(crate) private_fm_count: usize,
    pub(crate) playlist_name: String,
    pub(crate) playlist_id: Option<String>,
    pub(crate) playlist_exists: bool,
    pub(crate) playlist_lookup: String,
    pub(crate) existing_track_count: usize,
    pub(crate) would_create_playlist: bool,
    pub(crate) would_add_ids: Vec<String>,
    pub(crate) max_additions: Option<usize>,
    pub(crate) eligible_add_count: usize,
    pub(crate) deferred_count: usize,
    pub(crate) added_ids: Vec<String>,
    pub(crate) added_count: usize,
    pub(crate) decisions: Vec<Decision>,
    pub(crate) client_calls: u64,
    pub(crate) http_requests: u64,
    pub(crate) state_corrupt_recovered: bool,
    pub(crate) trusted_corrupt_recovered: bool,
}

pub(crate) fn account_uid(body: &Value) -> AppResult<String> {
    let top_code = body.get("code").and_then(Value::as_i64);
    let inner_code = body.pointer("/data/code").and_then(Value::as_i64);
    if matches!(top_code, Some(301) | Some(401) | Some(403))
        || matches!(inner_code, Some(301) | Some(401) | Some(403))
    {
        return Err(AppError::LoginRequired);
    }
    if (body.get("account").is_some_and(Value::is_null)
        && body.get("profile").is_some_and(Value::is_null))
        || (body.pointer("/data/account").is_some_and(Value::is_null)
            && body.pointer("/data/profile").is_some_and(Value::is_null))
    {
        return Err(AppError::LoginRequired);
    }
    body.pointer("/account/id")
        .and_then(value_id_ref)
        .or_else(|| body.pointer("/profile/userId").and_then(value_id_ref))
        .or_else(|| body.pointer("/data/account/id").and_then(value_id_ref))
        .ok_or_else(|| AppError::ApiIncompatible("登录状态响应缺少用户 id".to_string()))
}

pub(crate) fn account_vip_type(body: &Value) -> Option<i64> {
    body.pointer("/profile/vipType")
        .and_then(Value::as_i64)
        .or_else(|| body.pointer("/account/vipType").and_then(Value::as_i64))
        .or_else(|| {
            body.pointer("/data/profile/vipType")
                .and_then(Value::as_i64)
        })
}

pub(crate) fn select_playlist(
    playlists: &[PlaylistSummary],
    uid: &str,
    _preferred_id: Option<&str>,
) -> AppResult<Option<PlaylistSummary>> {
    let owned = playlists
        .iter()
        .filter(|playlist| {
            playlist.name == PLAYLIST_NAME && playlist.owner_id.as_deref() == Some(uid)
        })
        .cloned()
        .collect::<Vec<_>>();
    if owned.len() > 1 {
        return Err(AppError::AmbiguousPlaylist);
    }
    let only = owned.into_iter().next();
    Ok(only)
}

fn candidate_record(original: &Song, candidate: &Song, score: f32) -> Candidate {
    Candidate {
        id: candidate.id.clone(),
        title: candidate.name.clone(),
        artist: candidate.artists.join("、"),
        duration_ms: candidate.duration_ms,
        duration_delta_ms: match (original.duration_ms, candidate.duration_ms) {
            (Some(left), Some(right)) => Some(right - left),
            _ => None,
        },
        album: candidate.album.clone(),
        version_markers: sorted_version_markers(candidate),
        score,
    }
}

pub(crate) fn build_plan<R: RemoteApi>(
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    trusted: &TrustedStore,
    command: &str,
    state_corrupt_recovered: bool,
) -> AppResult<PlanReport> {
    build_plan_with_limit(
        remote,
        uid,
        state,
        trusted,
        command,
        state_corrupt_recovered,
        None,
    )
}

pub(crate) fn build_plan_with_limit<R: RemoteApi>(
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    trusted: &TrustedStore,
    command: &str,
    state_corrupt_recovered: bool,
    max_additions: Option<usize>,
) -> AppResult<PlanReport> {
    let fm_songs = remote.private_fm()?;
    let fm_ids = fm_songs
        .iter()
        .map(|song| song.id.clone())
        .collect::<Vec<_>>();
    let detail_map = remote.details(&fm_ids)?;
    let cached_playlist = if let Some(state_playlist_id) = state.playlist_id.as_deref() {
        remote.playlist_summary_by_id(state_playlist_id, uid)?
    } else {
        None
    };
    let (selected_playlist, playlist_lookup) = match cached_playlist {
        Some(playlist) => (Some(playlist), "cached_detail".to_string()),
        None => {
            let playlists = remote.user_playlists(uid)?;
            (
                select_playlist(&playlists, uid, state.playlist_id.as_deref())?,
                "user_playlist_pages".to_string(),
            )
        }
    };
    let playlist_id = selected_playlist
        .as_ref()
        .map(|playlist| playlist.id.clone());
    let existing = if let Some(id) = playlist_id.as_deref() {
        remote.playlist_tracks(id)?
    } else {
        HashSet::new()
    };
    let mut decisions = Vec::new();
    let mut planned_ids = HashSet::new();
    for original in fm_songs {
        let original = merge_song_metadata(&original, detail_map.get(&original.id));
        // The privilege fields are the primary entitlement signal. The official
        // URL endpoint is queried only as a capability probe; its URL is never
        // returned, stored, downloaded, or used for playlist replacement.
        let original_probe = remote.playback_probe(&original.id)?;
        let original_availability = availability_from_fields(&original, Some(original_probe));
        let original_evidence = availability_evidence(&original, original_probe);
        let mut candidates = Vec::new();
        let (action, selected, reason, trusted_mapping, trusted_invalid) = if original_availability
            == Availability::Free
        {
            (
                Action::AddOriginal,
                Some(original.clone()),
                "原歌曲 privilege 表明普通账号可完整播放".to_string(),
                false,
                false,
            )
        } else {
            // A user-approved trusted mapping wins deterministically, but only
            // while the approved target still passes the strict free check.
            let mut trusted_target = None;
            let mut trusted_stale = false;
            if let Some(mapping) = trusted.mappings.get(&original.id) {
                let target_details = remote.details(std::slice::from_ref(&mapping.target_id))?;
                if let Some(target) = target_details.get(&mapping.target_id).cloned() {
                    let target = merge_song_metadata(&target, None);
                    let target_probe = remote.playback_probe(&target.id)?;
                    if availability_from_fields(&target, Some(target_probe)) == Availability::Free {
                        trusted_target = Some(target);
                    }
                }
                if trusted_target.is_none() {
                    trusted_stale = true;
                }
            }
            if let Some(target) = trusted_target {
                (
                    Action::TrustedMapping,
                    Some(target),
                    "用户确认过的 trusted mapping；已验证该免费版本当前仍可完整播放".to_string(),
                    true,
                    false,
                )
            } else {
                let search_results = remote.search(&original.name)?;
                let search_ids = search_results
                    .iter()
                    .map(|song| song.id.clone())
                    .collect::<Vec<_>>();
                let searched_details = remote.details(&search_ids)?;
                let mut valid_candidates = Vec::new();
                for result in search_results {
                    let candidate = merge_song_metadata(&result, searched_details.get(&result.id));
                    let Some(score) = same_recording_score(&original, &candidate) else {
                        continue;
                    };
                    let preliminary = availability_from_fields(&candidate, None);
                    if matches!(
                        preliminary,
                        Availability::Restricted | Availability::Unavailable
                    ) {
                        continue;
                    }
                    let candidate_probe = remote.playback_probe(&candidate.id)?;
                    if availability_from_fields(&candidate, Some(candidate_probe))
                        != Availability::Free
                    {
                        continue;
                    }
                    valid_candidates.push((candidate, score));
                }
                valid_candidates.sort_by(|(left, left_score), (right, right_score)| {
                    right_score
                        .total_cmp(left_score)
                        .then_with(|| {
                            let left_delta = left
                                .duration_ms
                                .zip(original.duration_ms)
                                .map(|(candidate, source)| (candidate - source).abs())
                                .unwrap_or(i64::MAX);
                            let right_delta = right
                                .duration_ms
                                .zip(original.duration_ms)
                                .map(|(candidate, source)| (candidate - source).abs())
                                .unwrap_or(i64::MAX);
                            left_delta.cmp(&right_delta)
                        })
                        .then_with(|| left.id.cmp(&right.id))
                });
                candidates = valid_candidates
                    .iter()
                    .take(3)
                    .map(|(candidate, score)| candidate_record(&original, candidate, *score))
                    .collect();
                if let Some((candidate, _score)) = valid_candidates.into_iter().next() {
                    (
                        Action::CandidateOnly,
                        Some(candidate),
                        "原歌曲受限；免费同曲候选仅供人工 review，v0.1 不自动替换".to_string(),
                        false,
                        trusted_stale,
                    )
                } else {
                    (
                        Action::Skip,
                        None,
                        match original_availability {
                            Availability::Unavailable => {
                                "原歌曲不可用且没有高置信度免费同曲".to_string()
                            }
                            Availability::Restricted => {
                                "原歌曲需要 VIP/购买且没有高置信度免费同曲".to_string()
                            }
                            _ => "播放权限未知，按安全原则跳过".to_string(),
                        },
                        false,
                        trusted_stale,
                    )
                }
            }
        };
        let selected_id = selected.as_ref().map(|song| song.id.clone());
        let duplicate = matches!(action, Action::AddOriginal | Action::TrustedMapping)
            && selected_id
                .as_ref()
                .is_some_and(|id| existing.contains(id) || !planned_ids.insert(id.clone()));
        let final_action = if duplicate && !matches!(action, Action::Skip) {
            Action::AlreadyPresent
        } else {
            action
        };
        decisions.push(Decision {
            original_id: original.id.clone(),
            original_title: original.name.clone(),
            original_artist: original.artists.join("、"),
            original_duration_ms: original.duration_ms,
            action: final_action,
            availability: original_availability,
            availability_evidence: original_evidence,
            selected_id,
            selected_title: selected.as_ref().map(|song| song.name.clone()),
            selected_artist: selected.as_ref().map(|song| song.artists.join("、")),
            selected_duration_ms: selected.as_ref().and_then(|song| song.duration_ms),
            selected_album: selected.as_ref().and_then(|song| song.album.clone()),
            selected_version_markers: selected
                .as_ref()
                .map(sorted_version_markers)
                .unwrap_or_default(),
            trusted_mapping,
            trusted_invalid,
            candidates,
            reason: if duplicate {
                "已在 FreeFM · Auto 或本次计划中，保持幂等".to_string()
            } else {
                reason
            },
        });
    }
    let eligible_add_count = decisions
        .iter()
        .filter(|decision| {
            matches!(
                decision.action,
                Action::AddOriginal | Action::TrustedMapping
            )
        })
        .count();
    let mut candidates_to_add = Vec::new();
    let mut deferred_count = 0;
    for decision in &mut decisions {
        if !matches!(
            decision.action,
            Action::AddOriginal | Action::TrustedMapping
        ) {
            continue;
        }
        let within_budget = max_additions.is_none_or(|limit| candidates_to_add.len() < limit);
        if within_budget {
            if let Some(id) = &decision.selected_id {
                candidates_to_add.push(id.clone());
            }
        } else {
            decision.action = Action::DeferredByBudget;
            decision.reason = "本次 --max-additions 预算已用完；歌曲仍符合加入条件".to_string();
            deferred_count += 1;
        }
    }
    Ok(PlanReport {
        ok: true,
        command: command.to_string(),
        private_fm_count: decisions.len(),
        playlist_name: PLAYLIST_NAME.to_string(),
        playlist_id,
        playlist_exists: selected_playlist.is_some(),
        playlist_lookup,
        existing_track_count: existing.len(),
        would_create_playlist: selected_playlist.is_none(),
        would_add_ids: candidates_to_add,
        max_additions,
        eligible_add_count,
        deferred_count,
        added_ids: Vec::new(),
        added_count: 0,
        decisions,
        client_calls: remote.client_calls(),
        http_requests: remote.http_requests(),
        state_corrupt_recovered,
        trusted_corrupt_recovered: false,
    })
}
