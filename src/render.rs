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

pub(crate) fn is_login_error(error: &AppError) -> bool {
    matches!(error, AppError::LoginRequired)
        || matches!(error, AppError::Remote(message) if message.contains("登录类响应代码"))
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
