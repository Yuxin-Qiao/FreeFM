use crate::auth::refresh_session;
use crate::domain::{
    PlaylistSummary, Probe, Song, playlist_summary, song_from_value, strict_item_i64, value_id,
    value_id_ref,
};
use crate::error::{AppError, AppResult};
use crate::storage::Paths;
use netease_music::{
    NeteaseError, NeteaseMusicClient, PlaylistDetailParams, PlaylistTrackAllParams, SearchParams,
    SongDetailParams, SongQualityLevel, SongUrlV1Params, UserPlaylistParams,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub(crate) const FM_ENDPOINT: &str = "https://music.163.com/api/v1/radio/get";
pub(crate) const PLAYLIST_CREATE_ENDPOINT: &str = "https://music.163.com/api/playlist/create";
pub(crate) const PLAYLIST_ADD_ENDPOINT: &str =
    "https://music.163.com/api/playlist/manipulate/tracks";
pub(crate) const PLAYLIST_NAME: &str = "FreeFM · Auto";
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Remote {
    client: NeteaseMusicClient,
    pub(crate) client_calls: u64,
    pub(crate) http_requests: u64,
}

impl Remote {
    pub(crate) fn new(client: NeteaseMusicClient) -> Self {
        Self {
            client,
            client_calls: 0,
            http_requests: 0,
        }
    }
    fn call<F>(&mut self, endpoint: &str, f: F) -> AppResult<Value>
    where
        F: FnOnce(&NeteaseMusicClient) -> Result<netease_music::ApiResponse, NeteaseError>,
    {
        self.client_calls += 1;
        self.http_requests += 1;
        let response = f(&self.client).map_err(|error| match AppError::from(error) {
            AppError::Timeout => AppError::Timeout,
            AppError::Netease(error) => AppError::Remote(format!("{endpoint}: {error}")),
            other => other,
        })?;
        if response.status >= 500 {
            return Err(AppError::Remote(format!(
                "{endpoint}: HTTP {}",
                response.status
            )));
        }
        if matches!(response.code, Some(code) if code != 200) {
            return match remote_code_error(response.code) {
                AppError::LoginRequired => {
                    let safe_detail = response
                        .body
                        .get("msg")
                        .and_then(Value::as_str)
                        .or_else(|| response.body.get("message").and_then(Value::as_str))
                        .unwrap_or("无公开错误说明");
                    Err(AppError::Remote(format!(
                        "{endpoint}: 登录类响应代码 {}（{}）",
                        response.code.unwrap_or_default(),
                        safe_detail
                    )))
                }
                error => Err(error),
            };
        }
        if response.body.is_null() {
            return Err(AppError::ApiIncompatible(format!(
                "{endpoint}: 响应不是 JSON"
            )));
        }
        Ok(response.body)
    }
    pub(crate) fn status(&mut self) -> AppResult<Value> {
        self.call("login_status", |client| client.login_status())
    }
    fn private_fm(&mut self) -> AppResult<Vec<Song>> {
        let body = self.call("private_fm", |client| {
            client.raw_weapi(FM_ENDPOINT, json!({}))
        })?;
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::ApiIncompatible("私人 FM 响应缺少 data 数组".to_string()))?;
        data.iter().map(song_from_value).collect()
    }
    fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let body = self.call("song_detail", |client| {
            client.song_detail(SongDetailParams { ids: ids.to_vec() })
        })?;
        let songs = body
            .get("songs")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::ApiIncompatible("歌曲详情响应缺少 songs 数组".to_string()))?;
        let privileges = body
            .get("privileges")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|privilege| {
                        value_id(privilege.get("id")).map(|id| (id, privilege.clone()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut result = HashMap::new();
        for value in songs {
            let mut song = song_from_value(value)?;
            if song.privilege.is_none() {
                if let Some(privilege) = privileges.get(&song.id) {
                    song.privilege = Some(privilege.clone());
                }
            }
            result.insert(song.id.clone(), song);
        }
        Ok(result)
    }
    fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>> {
        let body = self.call("search", |client| {
            client.search(SearchParams {
                keywords: keywords.to_string(),
                limit: Some(30),
                offset: Some(0),
                ..Default::default()
            })
        })?;
        let songs = body
            .pointer("/result/songs")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                AppError::ApiIncompatible("搜索响应缺少 result.songs 数组".to_string())
            })?;
        songs.iter().map(song_from_value).collect()
    }
    fn playback_probe(&mut self, id: &str) -> AppResult<Probe> {
        let body = self.call("playback_probe", |client| {
            client.song_url_v1(SongUrlV1Params {
                id: id.to_string(),
                level: Some(SongQualityLevel::Standard),
                encode_type: Some("mp3".to_string()),
            })
        })?;
        let Some(item) = body
            .get("data")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        else {
            return Ok(Probe {
                has_url: false,
                fee: None,
                free_trial: false,
            });
        };
        Ok(Probe {
            has_url: item.get("url").is_some_and(|value| {
                !value.is_null() && value.as_str().is_some_and(|s| !s.is_empty())
            }),
            fee: strict_item_i64(item, "fee"),
            free_trial: item
                .get("freeTrialInfo")
                .is_some_and(|value| !value.is_null()),
        })
    }
    fn user_playlists(&mut self, uid: &str) -> AppResult<Vec<PlaylistSummary>> {
        let mut result = Vec::new();
        let mut seen_ids = HashSet::new();
        let mut offset = 0;
        loop {
            let body = self.call("user_playlist", |client| {
                client.user_playlist(UserPlaylistParams {
                    uid: uid.to_string(),
                    limit: Some(100),
                    offset: Some(offset),
                })
            })?;
            let playlists = body
                .get("playlist")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AppError::ApiIncompatible("用户歌单响应缺少 playlist 数组".to_string())
                })?;
            let page_len = playlists.len();
            let mut new_count = 0;
            for playlist in playlists {
                if let Some(summary) = playlist_summary(playlist) {
                    if seen_ids.insert(summary.id.clone()) {
                        result.push(summary);
                        new_count += 1;
                    }
                }
            }
            let more = body.get("more").and_then(Value::as_bool);
            if page_len < 100 || new_count == 0 || more == Some(false) {
                break;
            }
            offset += 100;
        }
        Ok(result)
    }
    fn playlist_summary_by_id(
        &mut self,
        playlist_id: &str,
        uid: &str,
    ) -> AppResult<Option<PlaylistSummary>> {
        let body = self.call("playlist_detail", |client| {
            client.playlist_detail(PlaylistDetailParams {
                id: playlist_id.to_string(),
                s: Some(8),
            })
        })?;
        let Some(playlist) = body.get("playlist") else {
            return Ok(None);
        };
        let Some(summary) = playlist_summary(playlist) else {
            return Ok(None);
        };
        if summary.name == PLAYLIST_NAME && summary.owner_id.as_deref() == Some(uid) {
            Ok(Some(summary))
        } else {
            Ok(None)
        }
    }
    fn playlist_tracks(&mut self, id: &str) -> AppResult<HashSet<String>> {
        let body = self.call("playlist_track_all", |client| {
            client.playlist_track_all(PlaylistTrackAllParams {
                id: id.to_string(),
                s: Some(8),
            })
        })?;
        self.http_requests += playlist_detail_extra_requests(&body);
        Ok(playlist_track_ids(&body))
    }
    fn create_playlist(&mut self) -> AppResult<String> {
        let body = self.call("playlist_create", |client| {
            client.raw_weapi(
                PLAYLIST_CREATE_ENDPOINT,
                json!({"name": PLAYLIST_NAME, "privacy": "0", "type": "NORMAL"}),
            )
        })?;
        body.pointer("/playlist/id")
            .and_then(value_id_ref)
            .or_else(|| body.get("id").and_then(value_id_ref))
            .ok_or_else(|| AppError::ApiIncompatible("创建歌单响应缺少 id".to_string()))
    }
    fn add_tracks(&mut self, playlist_id: &str, ids: &[String]) -> AppResult<()> {
        let track_ids = serde_json::to_string(ids)?;
        self.call("playlist_manipulate_tracks", |client| {
            client.raw_weapi(
                PLAYLIST_ADD_ENDPOINT,
                json!({"pid": playlist_id, "trackIds": track_ids, "op": "add"}),
            )
        })?;
        Ok(())
    }
}

pub(crate) fn playlist_track_ids(body: &Value) -> HashSet<String> {
    let mut ids = HashSet::new();
    if let Some(tracks) = body.pointer("/playlist/tracks").and_then(Value::as_array) {
        for track in tracks {
            if let Some(id) = value_id(track.get("id")) {
                ids.insert(id);
            }
        }
    }
    if let Some(track_ids) = body.pointer("/playlist/trackIds").and_then(Value::as_array) {
        for track in track_ids {
            if let Some(id) = value_id(track.get("id")) {
                ids.insert(id);
            }
        }
    }
    ids
}

pub(crate) fn playlist_detail_extra_requests(body: &Value) -> u64 {
    body.pointer("/playlist/trackIds")
        .and_then(Value::as_array)
        .map_or(0, |ids| ids.len().div_ceil(500) as u64)
}

pub(crate) trait RemoteApi {
    fn status(&mut self) -> AppResult<Value>;
    fn private_fm(&mut self) -> AppResult<Vec<Song>>;
    fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>>;
    fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>>;
    fn playback_probe(&mut self, id: &str) -> AppResult<Probe>;
    fn user_playlists(&mut self, uid: &str) -> AppResult<Vec<PlaylistSummary>>;
    fn playlist_summary_by_id(
        &mut self,
        playlist_id: &str,
        uid: &str,
    ) -> AppResult<Option<PlaylistSummary>>;
    fn playlist_tracks(&mut self, id: &str) -> AppResult<HashSet<String>>;
    fn create_playlist(&mut self) -> AppResult<String>;
    fn add_tracks(&mut self, playlist_id: &str, ids: &[String]) -> AppResult<()>;
    fn client_calls(&self) -> u64;
    fn http_requests(&self) -> u64;
    fn save_session(&self, paths: &Paths) -> AppResult<()>;
}

impl RemoteApi for Remote {
    fn status(&mut self) -> AppResult<Value> {
        Remote::status(self)
    }
    fn private_fm(&mut self) -> AppResult<Vec<Song>> {
        Remote::private_fm(self)
    }
    fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>> {
        Remote::details(self, ids)
    }
    fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>> {
        Remote::search(self, keywords)
    }
    fn playback_probe(&mut self, id: &str) -> AppResult<Probe> {
        Remote::playback_probe(self, id)
    }
    fn user_playlists(&mut self, uid: &str) -> AppResult<Vec<PlaylistSummary>> {
        Remote::user_playlists(self, uid)
    }
    fn playlist_summary_by_id(
        &mut self,
        playlist_id: &str,
        uid: &str,
    ) -> AppResult<Option<PlaylistSummary>> {
        Remote::playlist_summary_by_id(self, playlist_id, uid)
    }
    fn playlist_tracks(&mut self, id: &str) -> AppResult<HashSet<String>> {
        Remote::playlist_tracks(self, id)
    }
    fn create_playlist(&mut self) -> AppResult<String> {
        Remote::create_playlist(self)
    }
    fn add_tracks(&mut self, playlist_id: &str, ids: &[String]) -> AppResult<()> {
        Remote::add_tracks(self, playlist_id, ids)
    }
    fn client_calls(&self) -> u64 {
        self.client_calls
    }
    fn http_requests(&self) -> u64 {
        self.http_requests
    }
    fn save_session(&self, paths: &Paths) -> AppResult<()> {
        refresh_session(paths, &self.client)
    }
}

pub(crate) fn remote_code_error(code: Option<i64>) -> AppError {
    match code {
        Some(301) | Some(401) | Some(403) => AppError::LoginRequired,
        Some(code) => AppError::Remote(format!("返回代码 {code}")),
        None => AppError::ApiIncompatible("响应缺少 code".to_string()),
    }
}
