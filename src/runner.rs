use crate::audit::audit;
use crate::auth::{authenticate, new_client};
use crate::cli::{Cli, usage};
use crate::error::{AppError, AppResult};
use crate::plan::{account_uid, account_vip_type, build_plan};
use crate::protocol::{FM_ENDPOINT, PLAYLIST_ADD_ENDPOINT, PLAYLIST_CREATE_ENDPOINT, Remote};
use crate::storage::{
    Paths, StateFile, SyncLock, TrustedStore, load_session, load_state, load_trusted, now_seconds,
};
use crate::sync::{require_login, require_ordinary_account, sync};
use crate::trusted::{review, stdin_confirm, stdin_manage};
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

pub fn run(cli: Cli) -> AppResult<Value> {
    let paths = Paths::from_cli(&cli);
    match cli.command.as_str() {
        "auth" => authenticate(&paths),
        "status" => {
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let (trusted, trusted_corrupt_recovered) = load_trusted(&paths)?;
            let mut value = operational_metadata(
                &state,
                &trusted,
                state_corrupt_recovered,
                trusted_corrupt_recovered,
                now_seconds(),
            );
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
        "preview" | "sync" => {
            let _lock = SyncLock::acquire(&paths)?;
            let session = load_session(&paths)?.ok_or(AppError::LoginRequired)?;
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let (trusted, trusted_corrupt_recovered) = load_trusted(&paths)?;
            let mut remote = Remote::new(new_client(Some(&session))?);
            let account = require_login(&mut remote)?;
            require_ordinary_account(&account)?;
            let mut report = build_plan(
                &mut remote,
                &account.uid,
                &state,
                &trusted,
                &cli.command,
                state_corrupt_recovered,
            )?;
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
        "review" => {
            let _lock = SyncLock::acquire(&paths)?;
            let session = load_session(&paths)?.ok_or(AppError::LoginRequired)?;
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let mut remote = Remote::new(new_client(Some(&session))?);
            let account = require_login(&mut remote)?;
            require_ordinary_account(&account)?;
            review(
                &paths,
                &mut remote,
                &account.uid,
                &state,
                state_corrupt_recovered,
                cli.json,
                stdin_confirm,
                stdin_manage,
            )
        }
        "doctor" => {
            let session = load_session(&paths)?;
            let (_, state_corrupt_recovered) = load_state(&paths)?;
            let mut value = json!({"ok": true, "data_dir": paths.root.display().to_string(), "data_dir_exists": paths.root.exists(), "session_present": session.is_some(), "state_corrupt_recovered": state_corrupt_recovered, "protocol": {"private_fm": FM_ENDPOINT, "playability": "/api/song/enhance/player/url/v1", "search": "/api/cloudsearch/pc", "playlist_create": PLAYLIST_CREATE_ENDPOINT, "playlist_add": PLAYLIST_ADD_ENDPOINT}});
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
    }
}
