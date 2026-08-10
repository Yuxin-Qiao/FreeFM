use crate::audit::audit;
use crate::auth::{authenticate, new_client};
use crate::cli::{Cli, usage};
use crate::error::{AppError, AppResult};
use crate::plan::{account_uid, account_vip_type, build_plan};
use crate::protocol::{FM_ENDPOINT, PLAYLIST_ADD_ENDPOINT, PLAYLIST_CREATE_ENDPOINT, Remote};
use crate::render::is_login_error;
use crate::storage::{Paths, SyncLock, load_session, load_state, load_trusted};
use crate::sync::{require_login, require_ordinary_account, sync};
use crate::trusted::{review, stdin_confirm};
use serde_json::{Value, json};

pub fn run(cli: Cli) -> AppResult<Value> {
    let paths = Paths::from_cli(&cli);
    match cli.command.as_str() {
        "auth" => authenticate(&paths),
        "status" => {
            let session = load_session(&paths)?;
            let Some(session) = session else {
                return Ok(json!({"ok": true, "authenticated": false, "session_present": false}));
            };
            let client = new_client(Some(&session))?;
            let mut remote = Remote::new(client);
            match remote.status() {
                Ok(body) => Ok(json!({
                    "ok": true,
                    "authenticated": account_uid(&body).is_ok(),
                    "account_vip_type": account_vip_type(&body),
                    "session_present": true,
                    "client_calls": remote.client_calls,
                    "http_requests": remote.http_requests
                })),
                Err(error) if is_login_error(&error) => Ok(json!({
                    "ok": true,
                    "authenticated": false,
                    "session_present": true,
                    "login_required": true,
                    "client_calls": remote.client_calls,
                    "http_requests": remote.http_requests
                })),
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
                    Err(error) if is_login_error(&error) => {
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
