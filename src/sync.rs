use crate::error::{AppError, AppResult};
use crate::plan::{PlanReport, account_uid, account_vip_type, select_playlist};
use crate::protocol::RemoteApi;
use crate::render::is_login_error;
use crate::storage::{Paths, load_state, now_seconds, write_private_json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountContext {
    pub(crate) uid: String,
    pub(crate) vip_type: i64,
}

pub(crate) fn require_login<R: RemoteApi>(remote: &mut R) -> AppResult<AccountContext> {
    let body = match remote.status() {
        Ok(body) => body,
        Err(error) if is_login_error(&error) => return Err(AppError::LoginRequired),
        Err(error) => return Err(error),
    };
    let uid = account_uid(&body)?;
    let vip_type = account_vip_type(&body)
        .ok_or_else(|| AppError::ApiIncompatible("登录状态响应缺少 vipType".to_string()))?;
    Ok(AccountContext { uid, vip_type })
}

pub(crate) fn require_ordinary_account(account: &AccountContext) -> AppResult<()> {
    if account.vip_type == 0 {
        Ok(())
    } else {
        Err(AppError::OrdinaryAccountRequired)
    }
}

pub(crate) fn sync<R: RemoteApi>(
    paths: &Paths,
    mut report: PlanReport,
    uid: &str,
    remote: &mut R,
) -> AppResult<PlanReport> {
    let playlist_id = if let Some(id) = report.playlist_id.clone() {
        if remote.playlist_summary_by_id(&id, uid)?.is_none() {
            return Err(AppError::Remote(
                "同步前复核失败：缓存歌单不再属于当前账号或已改名".to_string(),
            ));
        }
        id
    } else {
        let playlists = remote.user_playlists(uid)?;
        if let Some(existing) = select_playlist(&playlists, uid, None)? {
            existing.id
        } else {
            remote.create_playlist()?
        }
    };
    let existing = remote.playlist_tracks(&playlist_id)?;
    let (mut state, _) = load_state(paths)?;
    let ids_to_add = report
        .would_add_ids
        .iter()
        .filter(|id| !existing.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    report.would_add_ids = ids_to_add.clone();
    if !ids_to_add.is_empty() {
        remote.add_tracks(&playlist_id, &ids_to_add)?;
        let after = remote.playlist_tracks(&playlist_id)?;
        let verified = ids_to_add.iter().all(|id| after.contains(id));
        if !verified {
            return Err(AppError::Remote("歌单写入后复读未找到全部歌曲".to_string()));
        }
        for id in &ids_to_add {
            if !state.added_song_ids.contains(id) {
                state.added_song_ids.push(id.clone());
            }
        }
    }
    state.playlist_id = Some(playlist_id.clone());
    state.last_sync_at = Some(now_seconds());
    write_private_json(&paths.state(), &state)?;
    remote.save_session(paths)?;
    report.command = "sync".to_string();
    report.playlist_id = Some(playlist_id);
    report.playlist_exists = true;
    report.would_create_playlist = false;
    report.client_calls = remote.client_calls();
    report.http_requests = remote.http_requests();
    Ok(report)
}
