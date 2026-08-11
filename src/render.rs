use crate::cli::Cli;
use crate::error::AppError;
use serde_json::{Value, json};
use std::fmt::Display;

pub fn json_error(error: &AppError) -> Value {
    let (kind, message) = match error {
        AppError::LoginRequired => ("login_required", error.to_string()),
        AppError::OrdinaryAccountRequired => ("ordinary_account_required", error.to_string()),
        AppError::ConcurrentSync => ("concurrent_sync", error.to_string()),
        AppError::AmbiguousPlaylist => ("ambiguous_playlist", error.to_string()),
        AppError::Timeout => ("timeout", error.to_string()),
        AppError::ApiIncompatible(_) => ("api_incompatible", error.to_string()),
        AppError::StateCorrupt(_) => ("state_corrupt", error.to_string()),
        _ => ("error", error.to_string()),
    };
    json!({"ok": false, "error": {"kind": kind, "message": message}})
}

pub(crate) fn rendered_output(
    cli: &Cli,
    value: &Value,
    human: impl Display,
    failed: bool,
) -> Option<String> {
    if cli.quiet && !failed {
        return None;
    }
    if cli.json {
        Some(serde_json::to_string(value).unwrap_or_else(|_| "{\"ok\":false}".to_string()))
    } else {
        Some(human.to_string())
    }
}

pub fn emit(cli: &Cli, value: &Value, human: impl Display, failed: bool) {
    if let Some(output) = rendered_output(cli, value, human, failed) {
        println!("{output}");
    }
}

fn text(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn title_artist(value: &Value) -> String {
    format!(
        "{} — {}",
        text(value, "original_title", "未命名歌曲"),
        text(value, "original_artist", "未知歌手")
    )
}

/// Renders the already-computed preview decisions without adding requests or
/// reimplementing entitlement logic. JSON output remains the machine contract.
pub fn preview_human(value: &Value) -> String {
    let decisions = value
        .get("decisions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count = value
        .get("private_fm_count")
        .and_then(Value::as_u64)
        .unwrap_or(decisions.len() as u64);
    let mut sections: Vec<(&str, Vec<String>)> = Vec::new();
    let mut added = 0usize;
    let mut candidates = 0usize;
    let mut skipped = 0usize;
    let mut already_present = 0usize;

    for decision in decisions {
        let action = decision
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("skip");
        let label = match action {
            "add_original" => {
                added += 1;
                "加入"
            }
            "trusted_mapping" => {
                added += 1;
                "加入（trusted mapping）"
            }
            "candidate_only" => {
                candidates += 1;
                "候选"
            }
            "already_present" => {
                already_present += 1;
                "已存在"
            }
            _ => {
                skipped += 1;
                "跳过"
            }
        };
        let mut lines = format!(
            "  {}\n  {}",
            title_artist(&decision),
            text(&decision, "reason", "按安全原则跳过")
        );
        if action == "candidate_only" {
            let candidate = format!(
                "{} — {}",
                text(&decision, "selected_title", "未命名候选"),
                text(&decision, "selected_artist", "未知歌手")
            );
            lines = format!(
                "  {}\n  免费候选：{}\n  {}",
                title_artist(&decision),
                candidate,
                text(&decision, "reason", "仅供人工确认")
            );
        }
        if action == "trusted_mapping" {
            let target = format!(
                "{} — {}",
                text(&decision, "selected_title", "未命名版本"),
                text(&decision, "selected_artist", "未知歌手")
            );
            lines = format!(
                "  {}\n  使用已确认版本：{}\n  {}",
                title_artist(&decision),
                target,
                text(&decision, "reason", "已验证当前可播放")
            );
        }
        if let Some((_, entries)) = sections.iter_mut().find(|(existing, _)| *existing == label) {
            entries.push(lines);
        } else {
            sections.push((label, vec![lines]));
        }
    }

    let mut output = format!("Private FM：{count} 首");
    for (label, entries) in sections {
        output.push_str("\n\n");
        output.push_str(label);
        output.push('\n');
        output.push_str(&entries.join("\n"));
    }
    output.push_str(&format!(
        "\n\n汇总：加入 {added} · 候选 {candidates} · 跳过 {skipped} · 已存在 {already_present}"
    ));
    output
}

/// Renders only audit attention items. The audit operation itself stays
/// read-only and the structured JSON response is unchanged.
pub fn audit_human(value: &Value) -> String {
    let summary = value.get("summary").unwrap_or(&Value::Null);
    let still_free = summary
        .get("still_free")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let restricted = summary
        .get("became_restricted")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unavailable = summary
        .get("unavailable")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unknown = summary.get("unknown").and_then(Value::as_u64).unwrap_or(0);
    let attention = restricted + unavailable + unknown;
    if attention == 0 {
        return format!(
            "审计完成：{still_free} 首仍可免费完整播放，无需处理。\naudit 只读，未修改歌单。\n"
        );
    }

    let mut output = format!(
        "审计完成：{still_free} 首仍免费，{attention} 首需要关注。\naudit 只读，未修改歌单。"
    );
    let items = value.get("items").and_then(Value::as_array);
    for status in ["became_restricted", "unavailable", "unknown"] {
        let label = match status {
            "became_restricted" => "受限",
            "unavailable" => "不可用",
            _ => "未知",
        };
        let matching = items
            .into_iter()
            .flat_map(|items| items.iter())
            .filter(|item| item.get("status").and_then(Value::as_str) == Some(status))
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        output.push_str(&format!("\n\n{label}\n"));
        for item in matching {
            output.push_str(&format!(
                "  {} — {}\n  {}\n",
                text(item, "title", "未命名歌曲"),
                text(item, "artist", "未知歌手"),
                text(item, "reason", "播放证据未知")
            ));
        }
    }
    output.trim_end().to_string()
}
