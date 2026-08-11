use crate::domain::{Availability, availability_from_fields, merge_song_metadata};
use crate::error::AppResult;
use crate::plan::{Action, Candidate, build_plan};
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
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) reason: String,
}

fn render_prompt(prompt: &CandidatePrompt, json: bool) {
    let mut lines = vec![
        format!(
            "原歌曲：{} - {}",
            prompt.original_title, prompt.original_artist
        ),
        format!(
            "原曲时长：{}",
            prompt
                .original_duration_ms
                .map(|duration| format!("{:.1}s", duration as f64 / 1000.0))
                .unwrap_or_else(|| "未知".to_string())
        ),
        format!(
            "找到 {} 个全部通过严格免费验证的候选：",
            prompt.candidates.len()
        ),
    ];
    for (index, candidate) in prompt.candidates.iter().enumerate() {
        let delta = candidate
            .duration_delta_ms
            .map(|value| format!("{:+.1}s", value as f64 / 1000.0))
            .unwrap_or_else(|| "未知".to_string());
        let markers = if candidate.version_markers.is_empty() {
            "无".to_string()
        } else {
            candidate.version_markers.join(", ")
        };
        lines.push(format!(
            "[{}] {} - {}",
            index + 1,
            candidate.title,
            candidate.artist
        ));
        lines.push(format!(
            "    专辑：{}",
            candidate.album.as_deref().unwrap_or("未知")
        ));
        lines.push(format!("    时长差：{delta}；版本标记：{markers}"));
    }
    lines.extend([
        "[0] 跳过".to_string(),
        format!("候选原因：{}", prompt.reason),
        "FreeFM 无法从网易 API 证明这是同一录音，需要你本人确认。".to_string(),
        format!("请选择候选编号 [0-{}]：", prompt.candidates.len()),
    ]);
    for line in lines {
        if json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

pub(crate) fn stdin_choose_candidate(prompt: &CandidatePrompt) -> Option<usize> {
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return None;
    }
    let selected = line.trim().parse::<usize>().ok()?;
    if selected == 0 || selected > prompt.candidates.len() {
        None
    } else {
        Some(selected - 1)
    }
}

pub(crate) fn stdin_confirm() -> bool {
    let mut line = String::new();
    let _ = io::stdout().flush();
    io::stdin()
        .read_line(&mut line)
        .is_ok_and(|_| matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

pub(crate) fn stdin_manage(existing: &[(String, String)]) -> Vec<String> {
    if existing.is_empty() {
        return Vec::new();
    }
    println!("当前 trusted mappings（{} 条）：", existing.len());
    for (index, (original_id, target_id)) in existing.iter().enumerate() {
        println!("  {index}: {original_id} -> {target_id}");
    }
    let mut remove = Vec::new();
    loop {
        println!("输入要移除的条目编号（多个用空格分隔），留空结束：");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        for part in line.split_whitespace() {
            if let Ok(index) = part.parse::<usize>() {
                if let Some((original_id, _)) = existing.get(index) {
                    remove.push(original_id.clone());
                }
            }
        }
    }
    remove
}

/// Interactive review: shows high-similarity free candidates for restricted
/// originals and, only after explicit user confirmation, persists a local
/// trusted mapping. Writes nothing remote; the only write is `trusted.json`.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub(crate) fn review<
    R: RemoteApi,
    F: FnMut() -> bool,
    M: FnMut(&[(String, String)]) -> Vec<String>,
>(
    paths: &Paths,
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    state_corrupt_recovered: bool,
    json: bool,
    confirm: F,
    manage: M,
) -> AppResult<Value> {
    review_with_selector(
        paths,
        remote,
        uid,
        state,
        state_corrupt_recovered,
        json,
        |_| Some(0),
        confirm,
        manage,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn review_with_selector<
    R: RemoteApi,
    S: FnMut(&CandidatePrompt) -> Option<usize>,
    F: FnMut() -> bool,
    M: FnMut(&[(String, String)]) -> Vec<String>,
>(
    paths: &Paths,
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    state_corrupt_recovered: bool,
    json: bool,
    mut select: S,
    mut confirm: F,
    mut manage: M,
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
        if decision.candidates.is_empty() {
            continue;
        }
        let prompt = CandidatePrompt {
            original_id: decision.original_id.clone(),
            original_title: decision.original_title.clone(),
            original_artist: decision.original_artist.clone(),
            original_duration_ms: decision.original_duration_ms,
            candidates: decision.candidates.clone(),
            reason: decision.reason.clone(),
        };
        render_prompt(&prompt, json);
        let Some(candidate_index) = select(&prompt) else {
            skipped.push(json!({
                "original_id": decision.original_id,
                "original_title": decision.original_title,
                "reason": "用户跳过或候选编号无效"
            }));
            continue;
        };
        let Some(candidate) = prompt.candidates.get(candidate_index) else {
            skipped.push(json!({
                "original_id": decision.original_id,
                "original_title": decision.original_title,
                "reason": "候选编号无效"
            }));
            continue;
        };
        if json {
            eprintln!("已选择：{} - {}", candidate.title, candidate.artist);
            eprintln!("确认将该候选保存为 trusted mapping？[y/N]");
        } else {
            println!("已选择：{} - {}", candidate.title, candidate.artist);
            println!("确认将该候选保存为 trusted mapping？[y/N]");
        }
        if !confirm() {
            skipped.push(json!({
                "original_id": decision.original_id,
                "original_title": decision.original_title,
                "reason": "用户未确认"
            }));
            continue;
        }
        // Re-verify the approved target right before persisting.
        let target_details = remote.details(std::slice::from_ref(&candidate.id))?;
        let still_free = target_details.get(&candidate.id).is_some_and(|song| {
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
        trusted.approve(&prompt.original_id, &candidate.id);
        save_trusted(paths, &trusted)?;
        approved.push(json!({
            "original_id": prompt.original_id,
            "original_title": prompt.original_title,
            "target_id": candidate.id,
            "target_title": candidate.title,
            "candidate_index": candidate_index + 1
        }));
    }
    let existing = trusted
        .mappings
        .iter()
        .map(|(original, mapping)| (original.clone(), mapping.target_id.clone()))
        .collect::<Vec<_>>();
    let mut removed = Vec::new();
    for original_id in manage(&existing) {
        if trusted.mappings.remove(&original_id).is_some() {
            removed.push(original_id);
            save_trusted(paths, &trusted)?;
        }
    }
    Ok(json!({
        "ok": true,
        "command": "review",
        "approved": approved,
        "skipped": skipped,
        "invalid_mappings": invalid,
        "removed": removed,
        "approved_count": approved.len(),
        "skipped_count": skipped.len(),
        "invalid_count": invalid.len(),
        "removed_count": removed.len(),
        "trusted_count": trusted.mappings.len(),
        "trusted_corrupt_recovered": trusted_corrupt_recovered,
        "state_corrupt_recovered": state_corrupt_recovered,
        "client_calls": remote.client_calls(),
        "http_requests": remote.http_requests()
    }))
}
