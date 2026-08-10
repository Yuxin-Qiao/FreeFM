use crate::domain::{Availability, availability_from_fields};
use crate::error::AppResult;
use crate::plan::select_playlist;
use crate::protocol::{PLAYLIST_NAME, RemoteApi};
use crate::storage::StateFile;
use serde_json::{Value, json};

const DETAIL_CHUNK: usize = 100;

/// Re-checks every saved `FreeFM · Auto` track with the same strict
/// playability logic `sync` uses at add time. Read-only: never creates,
/// appends, deletes, or reorders anything.
pub(crate) fn audit<R: RemoteApi>(
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    state_corrupt_recovered: bool,
) -> AppResult<Value> {
    let cached = state
        .playlist_id
        .as_deref()
        .map(|id| remote.playlist_summary_by_id(id, uid))
        .transpose()?
        .flatten();
    let playlist_id = match cached {
        Some(playlist) => Some(playlist.id.clone()),
        None => {
            let playlists = remote.user_playlists(uid)?;
            match select_playlist(&playlists, uid, None)? {
                Some(playlist) => Some(playlist.id),
                None => None,
            }
        }
    };
    let Some(playlist_id) = playlist_id else {
        return Ok(json!({
            "ok": true,
            "command": "audit",
            "playlist_name": PLAYLIST_NAME,
            "playlist_exists": false,
            "track_count": 0,
            "checked_count": 0,
            "summary": {
                "still_free": 0,
                "became_restricted": 0,
                "unavailable": 0,
                "unknown": 0
            },
            "needs_attention": false,
            "items": [],
            "client_calls": remote.client_calls(),
            "http_requests": remote.http_requests(),
            "state_corrupt_recovered": state_corrupt_recovered
        }));
    };
    let mut ids = remote
        .playlist_tracks(&playlist_id)?
        .into_iter()
        .collect::<Vec<_>>();
    ids.sort();
    let mut items = Vec::new();
    for chunk in ids.chunks(DETAIL_CHUNK) {
        let details = remote.details(chunk)?;
        for id in chunk {
            let Some(song) = details.get(id).cloned() else {
                items.push(json!({
                    "song_id": id,
                    "title": "",
                    "artist": "",
                    "status": "unknown",
                    "reason": "无法从曲库获取该歌曲详情"
                }));
                continue;
            };
            let preliminary = availability_from_fields(&song, None);
            let (status, reason) = match preliminary {
                Availability::Restricted => {
                    ("became_restricted", "privilege 表明现在需要 VIP/购买")
                }
                Availability::Unavailable => ("unavailable", "privilege 表明现在不可用"),
                _ => {
                    let probe = remote.playback_probe(&song.id)?;
                    match availability_from_fields(&song, Some(probe)) {
                        Availability::Free => ("still_free", ""),
                        Availability::Restricted => {
                            ("became_restricted", "播放探测表明现在需要 VIP/购买")
                        }
                        Availability::Unavailable => ("unavailable", "播放探测返回不可用"),
                        Availability::Unknown => ("unknown", "播放证据缺失、矛盾或响应结构未知"),
                    }
                }
            };
            items.push(json!({
                "song_id": id,
                "title": song.name,
                "artist": song.artists.join("、"),
                "status": status,
                "reason": reason
            }));
        }
    }
    let count = |status: &str| items.iter().filter(|item| item["status"] == status).count();
    let still_free = count("still_free");
    let became_restricted = count("became_restricted");
    let unavailable = count("unavailable");
    let unknown = count("unknown");
    let needs_attention = became_restricted + unavailable + unknown > 0;
    Ok(json!({
        "ok": true,
        "command": "audit",
        "playlist_name": PLAYLIST_NAME,
        "playlist_id": playlist_id,
        "playlist_exists": true,
        "track_count": items.len(),
        "checked_count": items.len(),
        "summary": {
            "still_free": still_free,
            "became_restricted": became_restricted,
            "unavailable": unavailable,
            "unknown": unknown
        },
        "needs_attention": needs_attention,
        "items": items,
        "client_calls": remote.client_calls(),
        "http_requests": remote.http_requests(),
        "state_corrupt_recovered": state_corrupt_recovered
    }))
}
