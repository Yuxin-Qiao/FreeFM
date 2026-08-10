use crate::domain::{Availability, availability_from_fields, merge_song_metadata};
use crate::error::AppResult;
use crate::plan::{Action, build_plan};
use crate::protocol::RemoteApi;
use crate::storage::{Paths, StateFile, load_trusted, save_trusted};
use serde_json::{Value, json};
use std::io::{self, Write};

#[derive(Debug, Clone)]
pub(crate) struct CandidatePrompt {
    pub(crate) original_id: String,
    pub(crate) original_title: String,
    pub(crate) original_artist: String,
    pub(crate) original_duration_ms: Option<i64>,
    pub(crate) candidate_id: String,
    pub(crate) candidate_title: String,
    pub(crate) candidate_artist: String,
    pub(crate) candidate_duration_ms: Option<i64>,
    pub(crate) candidate_album: Option<String>,
    pub(crate) version_markers: Vec<String>,
    pub(crate) reason: String,
}

fn render_prompt(prompt: &CandidatePrompt, json: bool) {
    let duration_diff = match (prompt.original_duration_ms, prompt.candidate_duration_ms) {
        (Some(left), Some(right)) => format!(
            "{} 秒（原 {:.1}s / 候选 {:.1}s）",
            (right - left) as f64 / 1000.0,
            left as f64 / 1000.0,
            right as f64 / 1000.0
        ),
        _ => "未知".to_string(),
    };
    let lines = [
        format!("原歌曲：{} - {}", prompt.original_title, prompt.original_artist),
        format!(
            "候选：{} - {}",
            prompt.candidate_title, prompt.candidate_artist
        ),
        format!("时长差异：{duration_diff}"),
        format!("专辑：{}", prompt.candidate_album.as_deref().unwrap_or("未知")),
        format!(
            "版本标记：{}",
            if prompt.version_markers.is_empty() {
                "无".to_string()
            } else {
                prompt.version_markers.join(", ")
            }
        ),
        format!("候选原因：{}", prompt.reason),
        "FreeFM 无法从网易 API 证明这是同一录音，需要你本人确认。".to_string(),
        "确认后仅在本机记录 trusted mapping；之后该原曲再次出现时会确定性使用此免费版本，且每次仍会重新验证其可播性。".to_string(),
        "确认该候选作为免费版本？[y/N] ".to_string(),
    ];
    for line in lines {
        if json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

pub(crate) fn stdin_confirm() -> bool {
    let mut line = String::new();
    let _ = io::stdout().flush();
    io::stdin()
        .read_line(&mut line)
        .is_ok_and(|_| matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

/// Interactive review: shows high-similarity free candidates for restricted
/// originals and, only after explicit user confirmation, persists a local
/// trusted mapping. Writes nothing remote; the only write is `trusted.json`.
pub(crate) fn review<R: RemoteApi, F: FnMut() -> bool>(
    paths: &Paths,
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    state_corrupt_recovered: bool,
    json: bool,
    mut confirm: F,
) -> AppResult<Value> {
    let (mut trusted, trusted_corrupt_recovered) = load_trusted(paths)?;
    let report = build_plan(
        remote,
        uid,
        state,
        &trusted,
        "preview",
        state_corrupt_recovered,
    )?;
    let mut approved = Vec::new();
    let mut skipped = Vec::new();
    let mut invalid = Vec::new();
    for decision in report.decisions.iter() {
        if decision.trusted_invalid && !matches!(decision.action, Action::CandidateOnly) {
            invalid.push(json!({
                "original_id": decision.original_id,
                "original_title": decision.original_title,
                "reason": "已有 trusted mapping 失效（免费版本现在不可播），需要重新 review"
            }));
            continue;
        }
        if !matches!(decision.action, Action::CandidateOnly) {
            continue;
        }
        let Some(candidate_id) = decision.selected_id.clone() else {
            continue;
        };
        let prompt = CandidatePrompt {
            original_id: decision.original_id.clone(),
            original_title: decision.original_title.clone(),
            original_artist: decision.original_artist.clone(),
            original_duration_ms: decision.original_duration_ms,
            candidate_id: candidate_id.clone(),
            candidate_title: decision.selected_title.clone().unwrap_or_default(),
            candidate_artist: decision.selected_artist.clone().unwrap_or_default(),
            candidate_duration_ms: decision.selected_duration_ms,
            candidate_album: decision.selected_album.clone(),
            version_markers: decision.selected_version_markers.clone(),
            reason: decision.reason.clone(),
        };
        render_prompt(&prompt, json);
        if !confirm() {
            skipped.push(json!({
                "original_id": decision.original_id,
                "original_title": decision.original_title,
                "reason": "用户未确认"
            }));
            continue;
        }
        // Re-verify the approved target right before persisting.
        let target_details = remote.details(std::slice::from_ref(&candidate_id))?;
        let still_free = target_details.get(&candidate_id).is_some_and(|song| {
            let merged = merge_song_metadata(song, None);
            let probe = remote.playback_probe(&merged.id);
            probe.is_ok_and(|probe| {
                availability_from_fields(&merged, Some(probe)) == Availability::Free
            })
        });
        if !still_free {
            skipped.push(json!({
                "original_id": decision.original_id,
                "original_title": decision.original_title,
                "reason": "候选在确认时已不可免费播放，未保存"
            }));
            continue;
        }
        trusted.approve(&prompt.original_id, &prompt.candidate_id);
        save_trusted(paths, &trusted)?;
        approved.push(json!({
            "original_id": prompt.original_id,
            "original_title": prompt.original_title,
            "target_id": prompt.candidate_id,
            "target_title": prompt.candidate_title
        }));
    }
    Ok(json!({
        "ok": true,
        "command": "review",
        "approved": approved,
        "skipped": skipped,
        "invalid_mappings": invalid,
        "approved_count": approved.len(),
        "skipped_count": skipped.len(),
        "invalid_count": invalid.len(),
        "trusted_count": trusted.mappings.len(),
        "trusted_corrupt_recovered": trusted_corrupt_recovered,
        "state_corrupt_recovered": state_corrupt_recovered,
        "client_calls": remote.client_calls(),
        "http_requests": remote.http_requests()
    }))
}
