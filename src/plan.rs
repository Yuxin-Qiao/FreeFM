use crate::domain::{
    Availability, AvailabilityEvidence, PlaylistSummary, Song, availability_evidence,
    availability_from_fields, merge_song_metadata, same_recording_score, sorted_version_markers,
    value_id_ref,
};
use crate::error::{AppError, AppResult};
use crate::protocol::{PLAYLIST_NAME, RemoteApi};
use crate::storage::StateFile;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    #[allow(dead_code)]
    AddOriginal,
    CandidateOnly,
    Skip,
    AlreadyPresent,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Decision {
    pub(crate) original_id: String,
    pub(crate) original_title: String,
    pub(crate) original_artist: String,
    pub(crate) action: Action,
    pub(crate) availability: Availability,
    pub(crate) availability_evidence: AvailabilityEvidence,
    pub(crate) selected_id: Option<String>,
    pub(crate) selected_title: Option<String>,
    pub(crate) selected_artist: Option<String>,
    pub(crate) selected_duration_ms: Option<i64>,
    pub(crate) selected_version_markers: Vec<String>,
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
    pub(crate) decisions: Vec<Decision>,
    pub(crate) client_calls: u64,
    pub(crate) http_requests: u64,
    pub(crate) state_corrupt_recovered: bool,
}

pub(crate) fn account_uid(body: &Value) -> AppResult<String> {
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

pub(crate) fn build_plan<R: RemoteApi>(
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    command: &str,
    state_corrupt_recovered: bool,
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
    let mut candidates_to_add = Vec::new();
    let mut planned_ids = HashSet::new();
    for original in fm_songs {
        let original = merge_song_metadata(&original, detail_map.get(&original.id));
        // The privilege fields are the primary entitlement signal. The official
        // URL endpoint is queried only as a capability probe; its URL is never
        // returned, stored, downloaded, or used for playlist replacement.
        let original_probe = remote.playback_probe(&original.id)?;
        let original_availability = availability_from_fields(&original, Some(original_probe));
        let original_evidence = availability_evidence(&original, original_probe);
        let (action, selected, reason) = if original_availability == Availability::Free {
            (
                Action::AddOriginal,
                Some(original.clone()),
                "原歌曲 privilege 表明普通账号可完整播放".to_string(),
            )
        } else {
            let search_results = remote.search(&original.name)?;
            let search_ids = search_results
                .iter()
                .map(|song| song.id.clone())
                .collect::<Vec<_>>();
            let searched_details = remote.details(&search_ids)?;
            let mut best = None;
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
                if availability_from_fields(&candidate, Some(candidate_probe)) != Availability::Free
                {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|(_, current_score): &(Song, f32)| score > *current_score)
                {
                    best = Some((candidate, score));
                }
            }
            if let Some((candidate, _score)) = best {
                (
                    Action::CandidateOnly,
                    Some(candidate),
                    "原歌曲受限；免费同曲候选仅供预览，v0.1 不自动替换".to_string(),
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
                )
            }
        };
        let selected_id = selected.as_ref().map(|song| song.id.clone());
        let duplicate = matches!(action, Action::AddOriginal)
            && selected_id
                .as_ref()
                .is_some_and(|id| existing.contains(id) || !planned_ids.insert(id.clone()));
        let final_action = if duplicate && !matches!(action, Action::Skip) {
            Action::AlreadyPresent
        } else {
            action
        };
        if matches!(final_action, Action::AddOriginal) {
            if let Some(id) = &selected_id {
                candidates_to_add.push(id.clone());
            }
        }
        decisions.push(Decision {
            original_id: original.id.clone(),
            original_title: original.name.clone(),
            original_artist: original.artists.join("、"),
            action: final_action,
            availability: original_availability,
            availability_evidence: original_evidence,
            selected_id,
            selected_title: selected.as_ref().map(|song| song.name.clone()),
            selected_artist: selected.as_ref().map(|song| song.artists.join("、")),
            selected_duration_ms: selected.as_ref().and_then(|song| song.duration_ms),
            selected_version_markers: selected
                .as_ref()
                .map(sorted_version_markers)
                .unwrap_or_default(),
            reason: if duplicate {
                "已在 FreeFM · Auto 或本次计划中，保持幂等".to_string()
            } else {
                reason
            },
        });
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
        decisions,
        client_calls: remote.client_calls(),
        http_requests: remote.http_requests(),
        state_corrupt_recovered,
    })
}
