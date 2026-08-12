use crate::audit::audit;
use crate::auth::{authenticate, new_client};
use crate::cli::{Cli, usage};
use crate::error::{AppError, AppResult};
use crate::plan::{
    account_uid, account_vip_type, build_plan_with_limit, build_source_plan_with_limit,
};
use crate::protocol::{FM_ENDPOINT, PLAYLIST_ADD_ENDPOINT, PLAYLIST_CREATE_ENDPOINT, Remote};
use crate::source::{load_source, source_diagnostics};
use crate::storage::{
    Paths, StateFile, SyncLock, TrustedStore, load_external_mappings, load_session, load_state,
    load_trusted, now_seconds,
};
use crate::sync::{require_login, require_ordinary_account, sync};
use crate::target::{target_diagnostics, transfer_source};
use crate::trusted::{
    review_with_selector_source, stdin_choose_candidate, stdin_confirm, stdin_manage,
};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(crate) fn operational_metadata(
    state: &StateFile,
    trusted: &TrustedStore,
    state_corrupt_recovered: bool,
    trusted_corrupt_recovered: bool,
    now: u64,
) -> Value {
    let managed_track_count = state.added_song_ids.iter().collect::<HashSet<_>>().len();
    json!({
        "last_sync_at": state.last_sync_at,
        "last_sync_age_seconds": state.last_sync_at.map(|timestamp| now.saturating_sub(timestamp)),
        "managed_track_count": managed_track_count,
        "trusted_mapping_count": trusted.mappings.len(),
        "state_corrupt_recovered": state_corrupt_recovered,
        "trusted_corrupt_recovered": trusted_corrupt_recovered,
    })
}

pub(crate) fn success_envelope(command: &str, mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("schema_version".to_string(), json!(1));
        object
            .entry("ok".to_string())
            .or_insert_with(|| json!(true));
        object.insert("command".to_string(), json!(command));
    }
    value
}

pub fn run(cli: Cli) -> AppResult<Value> {
    let paths = Paths::from_cli(&cli);
    let result = match cli.command.as_str() {
        "auth" => authenticate(&paths),
        "status" => {
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let (trusted, trusted_corrupt_recovered) = load_trusted(&paths)?;
            let (external_mappings, external_mapping_corrupt_recovered) =
                load_external_mappings(&paths)?;
            let mut value = operational_metadata(
                &state,
                &trusted,
                state_corrupt_recovered,
                trusted_corrupt_recovered,
                now_seconds(),
            );
            value["external_mapping_count"] = json!(external_mappings.mappings.len());
            value["external_mapping_corrupt_recovered"] = json!(external_mapping_corrupt_recovered);
            value["ok"] = json!(true);
            let session = load_session(&paths)?;
            let Some(session) = session else {
                value["authenticated"] = json!(false);
                value["session_present"] = json!(false);
                return Ok(value);
            };
            let client = new_client(Some(&session))?;
            let mut remote = Remote::new(client);
            match remote.status() {
                Ok(body) => {
                    value["authenticated"] = json!(account_uid(&body).is_ok());
                    value["account_vip_type"] = json!(account_vip_type(&body));
                    value["session_present"] = json!(true);
                    value["client_calls"] = json!(remote.client_calls);
                    value["http_requests"] = json!(remote.http_requests);
                    Ok(value)
                }
                Err(AppError::LoginRequired) => {
                    value["authenticated"] = json!(false);
                    value["session_present"] = json!(true);
                    value["login_required"] = json!(true);
                    value["client_calls"] = json!(remote.client_calls);
                    value["http_requests"] = json!(remote.http_requests);
                    Ok(value)
                }
                Err(error) => Err(error),
            }
        }
        "sync" if cli.target.is_some() => {
            let _lock = SyncLock::acquire(&paths)?;
            let source = cli
                .source
                .as_deref()
                .ok_or_else(|| {
                    AppError::Usage("sync --target 需要 --source 外部歌单 URL".to_string())
                })
                .and_then(load_source)?;
            let target = cli
                .target
                .as_deref()
                .ok_or_else(|| AppError::Usage("sync --target 需要目标歌单 URL".to_string()))?;
            transfer_source(&paths, &source, target, cli.max_additions)
        }
        "preview" | "sync" => {
            let _lock = SyncLock::acquire(&paths)?;
            let session = load_session(&paths)?.ok_or(AppError::LoginRequired)?;
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let (trusted, trusted_corrupt_recovered) = load_trusted(&paths)?;
            let mut remote = Remote::new(new_client(Some(&session))?);
            let account = require_login(&mut remote)?;
            require_ordinary_account(&account)?;
            let source = cli.source.as_deref().map(load_source).transpose()?;
            let mut report = match source.as_ref() {
                Some(source) => build_source_plan_with_limit(
                    &mut remote,
                    &account.uid,
                    &state,
                    &trusted,
                    &cli.command,
                    state_corrupt_recovered,
                    cli.max_additions,
                    source,
                )?,
                None => build_plan_with_limit(
                    &mut remote,
                    &account.uid,
                    &state,
                    &trusted,
                    &cli.command,
                    state_corrupt_recovered,
                    cli.max_additions,
                )?,
            };
            report.trusted_corrupt_recovered = trusted_corrupt_recovered;
            if cli.command == "sync" {
                Ok(serde_json::to_value(sync(
                    &paths,
                    report,
                    &account.uid,
                    &mut remote,
                )?)?)
            } else {
                Ok(serde_json::to_value(report)?)
            }
        }
        "audit" => {
            let _lock = SyncLock::acquire(&paths)?;
            let session = load_session(&paths)?.ok_or(AppError::LoginRequired)?;
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let mut remote = Remote::new(new_client(Some(&session))?);
            let account = require_login(&mut remote)?;
            require_ordinary_account(&account)?;
            audit(&mut remote, &account.uid, &state, state_corrupt_recovered)
        }
        "review" if cli.target.is_some() => {
            let _lock = SyncLock::acquire(&paths)?;
            let source = cli
                .source
                .as_deref()
                .map(load_source)
                .transpose()?
                .ok_or_else(|| {
                    AppError::Usage("review --target 需要 --source 外部歌单 URL".to_string())
                })?;
            let target = cli
                .target
                .as_deref()
                .ok_or_else(|| AppError::Usage("review --target 需要目标歌单 URL".to_string()))?;
            crate::target::review_source_to_target(
                &paths,
                &source,
                target,
                cli.json,
                stdin_choose_candidate,
                stdin_confirm,
            )
        }
        "review" => {
            let _lock = SyncLock::acquire(&paths)?;
            let session = load_session(&paths)?.ok_or(AppError::LoginRequired)?;
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let mut remote = Remote::new(new_client(Some(&session))?);
            let account = require_login(&mut remote)?;
            require_ordinary_account(&account)?;
            let source = cli.source.as_deref().map(load_source).transpose()?;
            review_with_selector_source(
                &paths,
                &mut remote,
                &account.uid,
                &state,
                state_corrupt_recovered,
                cli.json,
                source.as_ref(),
                stdin_choose_candidate,
                stdin_confirm,
                stdin_manage,
            )
        }
        "doctor" => {
            let session = load_session(&paths)?;
            let (_, state_corrupt_recovered) = load_state(&paths)?;
            let (external_mappings, external_mapping_corrupt_recovered) =
                load_external_mappings(&paths)?;
            let mut value = json!({"ok": true, "data_dir": paths.root.display().to_string(), "data_dir_exists": paths.root.exists(), "session_present": session.is_some(), "state_corrupt_recovered": state_corrupt_recovered, "protocol": {"private_fm": FM_ENDPOINT, "playability": "/api/song/enhance/player/url/v1", "search": "/api/cloudsearch/pc", "playlist_create": PLAYLIST_CREATE_ENDPOINT, "playlist_add": PLAYLIST_ADD_ENDPOINT}, "external_sources": {"spotify": "Spotify Web API", "apple_music": "Apple Music API", "youtube_music": "YouTube Data API v3"}});
            value["external_mapping_count"] = json!(external_mappings.mappings.len());
            value["external_mapping_corrupt_recovered"] = json!(external_mapping_corrupt_recovered);
            if let Some(source) = cli.source.as_deref() {
                value["source_diagnostics"] = source_diagnostics(source)?;
            }
            if let Some(target) = cli.target.as_deref() {
                value["target_diagnostics"] = target_diagnostics(target)?;
            }
            if let Some(session) = session {
                let mut remote = Remote::new(new_client(Some(&session))?);
                match remote.status() {
                    Ok(body) => {
                        value["authenticated"] = json!(account_uid(&body).is_ok());
                        value["account_vip_type"] = json!(account_vip_type(&body));
                        value["client_calls"] = json!(remote.client_calls);
                        value["http_requests"] = json!(remote.http_requests);
                    }
                    Err(AppError::LoginRequired) => {
                        value["authenticated"] = json!(false);
                        value["login_required"] = json!(true);
                        value["client_calls"] = json!(remote.client_calls);
                        value["http_requests"] = json!(remote.http_requests);
                    }
                    Err(error) => return Err(error),
                }
            } else {
                value["authenticated"] = json!(false);
            }
            Ok(value)
        }
        other => Err(AppError::Usage(format!("未知命令：{other}\n\n{}", usage()))),
    };
    result.map(|value| success_envelope(&cli.command, value))
}
