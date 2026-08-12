//! Read-only playlist metadata adapters for supported external music services.
//!
//! These adapters never download or play media. They only read playlist
//! metadata. Source metadata never unlocks playback, substitutes recordings,
//! or bypasses the explicit review boundary used by cross-service imports.

use crate::error::{AppError, AppResult};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::AUTHORIZATION;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::time::Duration;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const SPOTIFY_TOKEN_ENV: &str = "FREEFM_SPOTIFY_TOKEN";
const SPOTIFY_MARKET_ENV: &str = "FREEFM_SPOTIFY_MARKET";
const APPLE_TOKEN_ENV: &str = "FREEFM_APPLE_MUSIC_DEVELOPER_TOKEN";
const APPLE_USER_TOKEN_ENV: &str = "FREEFM_APPLE_MUSIC_USER_TOKEN";
const YOUTUBE_KEY_ENV: &str = "FREEFM_YOUTUBE_API_KEY";
const YOUTUBE_TOKEN_ENV: &str = "FREEFM_YOUTUBE_ACCESS_TOKEN";
const SPOTIFY_API: &str = "https://api.spotify.com/v1";
const APPLE_API: &str = "https://api.music.apple.com/v1";
const YOUTUBE_API: &str = "https://www.googleapis.com/youtube/v3";
const MAX_SOURCE_PAGES: usize = 10_000;

#[derive(Debug, Clone)]
struct SourceEndpoints {
    spotify_api: String,
    apple_api: String,
    youtube_api: String,
}

impl Default for SourceEndpoints {
    fn default() -> Self {
        Self {
            spotify_api: SPOTIFY_API.to_string(),
            apple_api: APPLE_API.to_string(),
            youtube_api: YOUTUBE_API.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SourceCredentials {
    spotify_token: Option<String>,
    spotify_market: Option<String>,
    apple_developer_token: Option<String>,
    apple_user_token: Option<String>,
    youtube_api_key: Option<String>,
    youtube_access_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    Spotify,
    AppleMusic,
    YoutubeMusic,
}

impl SourceKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Spotify => "Spotify",
            Self::AppleMusic => "Apple Music",
            Self::YoutubeMusic => "YouTube Music",
        }
    }

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Spotify => "spotify",
            Self::AppleMusic => "apple_music",
            Self::YoutubeMusic => "youtube_music",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRef {
    pub(crate) kind: SourceKind,
    pub(crate) id: String,
    pub(crate) storefront: Option<String>,
    pub(crate) apple_library: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SourceTrack {
    pub(crate) id: String,
    /// A source-specific canonical id usable by a same-platform target.
    /// Apple library songs can expose their catalog id separately from the
    /// library resource id; other adapters currently use `id` directly.
    pub(crate) canonical_id: Option<String>,
    pub(crate) isrc: Option<String>,
    pub(crate) name: String,
    pub(crate) artists: Vec<String>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) album: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SourcePlaylist {
    pub(crate) kind: SourceKind,
    pub(crate) id: String,
    pub(crate) storefront: Option<String>,
    pub(crate) apple_library: bool,
    pub(crate) tracks: Vec<SourceTrack>,
    pub(crate) skipped_count: usize,
}

struct SourceHttp {
    client: Client,
    endpoints: SourceEndpoints,
}

impl SourceHttp {
    fn new() -> AppResult<Self> {
        Self::with_endpoints(SourceEndpoints::default())
    }

    fn with_endpoints(endpoints: SourceEndpoints) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("FreeFM/0.1 (playlist metadata)")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::SourceRemote("无法初始化外部平台 HTTP 客户端".to_string()))?;
        Ok(Self { client, endpoints })
    }

    fn json(&self, request: RequestBuilder, kind: SourceKind) -> AppResult<Value> {
        let response = request.send().map_err(|error| {
            if error.is_timeout() {
                AppError::SourceTimeout
            } else {
                AppError::SourceRemote(format!("{} API 请求失败", kind.label()))
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => AppError::SourceAuthRequired(format!(
                    "{} API 凭证无效、缺失或无权读取此歌单",
                    kind.label()
                )),
                429 => AppError::SourceRemote(format!("{} API 请求过于频繁", kind.label())),
                code => AppError::SourceRemote(format!("{} API HTTP {code}", kind.label())),
            });
        }
        response.json::<Value>().map_err(|_| {
            AppError::SourceApiIncompatible(format!("{} API 响应不是 JSON", kind.label()))
        })
    }
}

pub(crate) fn parse_source_url(input: &str) -> AppResult<SourceRef> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AppError::SourceUrlInvalid("URL 为空".to_string()));
    }
    if let Some(id) = input.strip_prefix("spotify:playlist:") {
        return valid_id(id, SourceKind::Spotify).map(|id| SourceRef {
            kind: SourceKind::Spotify,
            id,
            storefront: None,
            apple_library: false,
        });
    }
    let url = Url::parse(input)
        .map_err(|_| AppError::SourceUrlInvalid("请提供完整的 https 歌单 URL".to_string()))?;
    if url.scheme() != "https" {
        return Err(AppError::SourceUrlInvalid(
            "只接受 https 歌单 URL".to_string(),
        ));
    }
    let host = url
        .host_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| AppError::SourceUrlInvalid("URL 缺少主机名".to_string()))?;
    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if matches!(host.as_str(), "open.spotify.com" | "www.spotify.com") {
        let id = path_after(&segments, "playlist")?;
        return valid_id(id, SourceKind::Spotify).map(|id| SourceRef {
            kind: SourceKind::Spotify,
            id,
            storefront: None,
            apple_library: false,
        });
    }

    if host == "music.apple.com" {
        let storefront = segments
            .first()
            .filter(|value| value.len() == 2 && value.chars().all(|ch| ch.is_ascii_alphabetic()))
            .map(|value| value.to_ascii_lowercase())
            .ok_or_else(|| {
                AppError::SourceUrlInvalid("Apple Music URL 缺少 storefront".to_string())
            })?;
        let apple_library = segments
            .windows(2)
            .any(|window| window == ["library", "playlist"]);
        let id = if apple_library {
            path_after(&segments, "playlist")?
        } else {
            segments
                .iter()
                .rev()
                .find(|segment| segment.starts_with("pl."))
                .ok_or_else(|| {
                    AppError::SourceUrlInvalid("Apple Music URL 缺少 playlist id".to_string())
                })?
        };
        return valid_id(id, SourceKind::AppleMusic).map(|id| SourceRef {
            kind: SourceKind::AppleMusic,
            id,
            storefront: Some(storefront),
            apple_library,
        });
    }

    if matches!(
        host.as_str(),
        "music.youtube.com" | "youtube.com" | "www.youtube.com" | "m.youtube.com"
    ) {
        let id = url
            .query_pairs()
            .find(|(key, _)| key == "list")
            .map(|(_, value)| value.into_owned())
            .ok_or_else(|| {
                AppError::SourceUrlInvalid("YouTube Music URL 缺少 list 参数".to_string())
            })?;
        return valid_id(&id, SourceKind::YoutubeMusic).map(|id| SourceRef {
            kind: SourceKind::YoutubeMusic,
            id,
            storefront: None,
            apple_library: false,
        });
    }

    Err(AppError::SourceUrlInvalid(
        "支持 open.spotify.com、music.apple.com 和 music.youtube.com 歌单 URL".to_string(),
    ))
}

fn path_after<'a>(segments: &'a [&str], marker: &str) -> AppResult<&'a str> {
    segments
        .iter()
        .position(|segment| *segment == marker)
        .and_then(|index| segments.get(index + 1).copied())
        .ok_or_else(|| AppError::SourceUrlInvalid(format!("URL 缺少 {marker} id")))
}

fn valid_id(id: &str, kind: SourceKind) -> AppResult<String> {
    let valid = !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'));
    if valid {
        Ok(id.to_string())
    } else {
        Err(AppError::SourceUrlInvalid(format!(
            "{} playlist id 无效",
            kind.label()
        )))
    }
}

fn env_credential(name: &str, label: &str) -> AppResult<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::SourceAuthRequired(format!("请设置环境变量 {name} 读取 {label} 歌单"))
        })
}

impl SourceCredentials {
    fn from_env(source: &SourceRef) -> AppResult<Self> {
        let mut credentials = Self::default();
        match source.kind {
            SourceKind::Spotify => {
                credentials.spotify_token = Some(env_credential(SPOTIFY_TOKEN_ENV, "Spotify")?);
                credentials.spotify_market = env::var(SPOTIFY_MARKET_ENV)
                    .ok()
                    .filter(|value| !value.trim().is_empty());
            }
            SourceKind::AppleMusic => {
                credentials.apple_developer_token =
                    Some(env_credential(APPLE_TOKEN_ENV, "Apple Music")?);
                if source.apple_library {
                    credentials.apple_user_token =
                        Some(env_credential(APPLE_USER_TOKEN_ENV, "Apple Music 资料库")?);
                }
            }
            SourceKind::YoutubeMusic => {
                credentials.youtube_access_token = env::var(YOUTUBE_TOKEN_ENV)
                    .ok()
                    .filter(|value| !value.trim().is_empty());
                if credentials.youtube_access_token.is_none() {
                    credentials.youtube_api_key =
                        Some(env_credential(YOUTUBE_KEY_ENV, "YouTube Music")?);
                }
            }
        }
        Ok(credentials)
    }
}

pub(crate) fn load_source(input: &str) -> AppResult<SourcePlaylist> {
    let source = parse_source_url(input)?;
    let credentials = SourceCredentials::from_env(&source)?;
    let http = SourceHttp::new()?;
    load_source_with(&http, &source, &credentials)
}

pub(crate) fn source_diagnostics(input: &str) -> AppResult<Value> {
    let source = parse_source_url(input)?;
    let configured = |name: &str| {
        env::var(name)
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    };
    let (ready, credentials) = match source.kind {
        SourceKind::Spotify => (
            configured(SPOTIFY_TOKEN_ENV),
            json!({
                "spotify_token": configured(SPOTIFY_TOKEN_ENV),
                "spotify_market_optional": configured(SPOTIFY_MARKET_ENV),
            }),
        ),
        SourceKind::AppleMusic => {
            let developer = configured(APPLE_TOKEN_ENV);
            let user = configured(APPLE_USER_TOKEN_ENV);
            (
                developer && (!source.apple_library || user),
                json!({
                    "apple_music_developer_token": developer,
                    "apple_music_user_token": if source.apple_library { Some(user) } else { None },
                }),
            )
        }
        SourceKind::YoutubeMusic => {
            let key = configured(YOUTUBE_KEY_ENV);
            let token = configured(YOUTUBE_TOKEN_ENV);
            (
                key || token,
                json!({
                    "youtube_api_key": key,
                    "youtube_access_token": token,
                }),
            )
        }
    };
    Ok(json!({
        "source_kind": source.kind,
        "source_playlist_id": source.id,
        "apple_library": source.apple_library,
        "ready": ready,
        "credentials": credentials,
    }))
}

fn load_source_with(
    http: &SourceHttp,
    source: &SourceRef,
    credentials: &SourceCredentials,
) -> AppResult<SourcePlaylist> {
    match source.kind {
        SourceKind::Spotify => load_spotify(http, source, credentials),
        SourceKind::AppleMusic => load_apple_music(http, source, credentials),
        SourceKind::YoutubeMusic => load_youtube_music(http, source, credentials),
    }
}

fn load_spotify(
    http: &SourceHttp,
    source: &SourceRef,
    credentials: &SourceCredentials,
) -> AppResult<SourcePlaylist> {
    let token = credentials
        .spotify_token
        .as_deref()
        .ok_or_else(|| AppError::SourceAuthRequired("Spotify 凭证缺失".to_string()))?;
    let endpoint = format!(
        "{}/playlists/{}/items",
        http.endpoints.spotify_api.trim_end_matches('/'),
        source.id
    );
    let mut offset = 0u32;
    let mut page_count = 0usize;
    let mut tracks = Vec::new();
    let mut skipped_count = 0;
    loop {
        page_count += 1;
        if page_count > MAX_SOURCE_PAGES {
            return Err(AppError::SourceApiIncompatible(
                "Spotify 分页超过安全上限".to_string(),
            ));
        }
        let mut request = http
            .client
            .get(&endpoint)
            .bearer_auth(token)
            .query(&[("limit", 50u32), ("offset", offset)]);
        if let Some(market) = credentials.spotify_market.as_deref() {
            request = request.query(&[("market", market)]);
        }
        let body = http.json(request, source.kind)?;
        let items = body.get("items").and_then(Value::as_array).ok_or_else(|| {
            AppError::SourceApiIncompatible("Spotify 歌单响应缺少 items 数组".to_string())
        })?;
        for item in items {
            match parse_spotify_item(item) {
                Some(track) => tracks.push(track),
                None => skipped_count += 1,
            }
        }
        if items.len() < 50 {
            break;
        }
        offset = offset.saturating_add(items.len() as u32);
        if body
            .get("total")
            .and_then(Value::as_u64)
            .is_some_and(|total| offset as u64 >= total)
        {
            break;
        }
    }
    Ok(SourcePlaylist {
        kind: source.kind,
        id: source.id.clone(),
        storefront: source.storefront.clone(),
        apple_library: source.apple_library,
        tracks,
        skipped_count,
    })
}

fn parse_spotify_item(item: &Value) -> Option<SourceTrack> {
    let track = item
        .get("track")
        .or_else(|| item.get("item"))?
        .as_object()?;
    if track.get("type").and_then(Value::as_str) != Some("track")
        || item.get("is_local").and_then(Value::as_bool) == Some(true)
        || track.get("is_local").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let id = track.get("id")?.as_str()?.to_string();
    let name = non_empty_string(track.get("name")?.as_str()?)?;
    let artists = track
        .get("artists")?
        .as_array()?
        .iter()
        .filter_map(|artist| {
            artist
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|artist| !artist.is_empty())
        .collect::<Vec<_>>();
    if artists.is_empty() {
        return None;
    }
    Some(SourceTrack {
        id,
        canonical_id: None,
        isrc: track
            .get("external_ids")
            .and_then(|external_ids| external_ids.pointer("/isrc"))
            .and_then(Value::as_str)
            .map(str::to_string),
        name,
        artists,
        duration_ms: track.get("duration_ms").and_then(Value::as_i64),
        album: track
            .get("album")
            .and_then(|album| album.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn load_apple_music(
    http: &SourceHttp,
    source: &SourceRef,
    credentials: &SourceCredentials,
) -> AppResult<SourcePlaylist> {
    let developer_token = credentials
        .apple_developer_token
        .as_deref()
        .ok_or_else(|| {
            AppError::SourceAuthRequired("Apple Music developer token 缺失".to_string())
        })?;
    let storefront = source.storefront.as_deref().unwrap_or("us");
    let endpoint = if source.apple_library {
        format!(
            "{}/me/library/playlists/{}",
            http.endpoints.apple_api.trim_end_matches('/'),
            source.id
        )
    } else {
        format!(
            "{}/catalog/{storefront}/playlists/{}",
            http.endpoints.apple_api.trim_end_matches('/'),
            source.id
        )
    };
    let user_token = credentials.apple_user_token.as_deref();
    let mut body = http.json(
        apple_authorized_request(
            http.client
                .get(&endpoint)
                .header(AUTHORIZATION, format!("Bearer {developer_token}")),
            user_token,
        ),
        source.kind,
    )?;
    let mut tracks = Vec::new();
    let mut skipped_count = 0;
    if body.pointer("/data/0/relationships/tracks/data").is_some() {
        parse_apple_items(&body, &mut tracks, &mut skipped_count)?;
    } else if let Some(href) = body
        .pointer("/data/0/relationships/tracks/href")
        .and_then(Value::as_str)
    {
        body = http.json(
            apple_authorized_request(
                http.client
                    .get(apple_next_url(href, &http.endpoints.apple_api)?)
                    .header(AUTHORIZATION, format!("Bearer {developer_token}")),
                user_token,
            ),
            source.kind,
        )?;
        parse_apple_items(&body, &mut tracks, &mut skipped_count)?;
    } else {
        let relationship_endpoint = if source.apple_library {
            format!(
                "{}/me/library/playlists/{}/tracks",
                http.endpoints.apple_api.trim_end_matches('/'),
                source.id
            )
        } else {
            format!(
                "{}/catalog/{storefront}/playlists/{}/tracks",
                http.endpoints.apple_api.trim_end_matches('/'),
                source.id
            )
        };
        body = http.json(
            apple_authorized_request(
                http.client
                    .get(relationship_endpoint)
                    .header(AUTHORIZATION, format!("Bearer {developer_token}")),
                user_token,
            ),
            source.kind,
        )?;
        parse_apple_items(&body, &mut tracks, &mut skipped_count)?;
    }
    let mut seen_next = HashSet::new();
    let mut page_count = 1usize;
    while let Some(next) = apple_next(&body) {
        page_count += 1;
        if page_count > MAX_SOURCE_PAGES {
            return Err(AppError::SourceApiIncompatible(
                "Apple Music 分页超过安全上限".to_string(),
            ));
        }
        let next = apple_next_url(next, &http.endpoints.apple_api)?;
        if !seen_next.insert(next.clone()) {
            return Err(AppError::SourceApiIncompatible(
                "Apple Music 分页出现循环".to_string(),
            ));
        }
        body = http.json(
            apple_authorized_request(
                http.client
                    .get(next)
                    .header(AUTHORIZATION, format!("Bearer {developer_token}")),
                user_token,
            ),
            source.kind,
        )?;
        parse_apple_items(&body, &mut tracks, &mut skipped_count)?;
    }
    Ok(SourcePlaylist {
        kind: source.kind,
        id: source.id.clone(),
        storefront: source.storefront.clone(),
        apple_library: source.apple_library,
        tracks,
        skipped_count,
    })
}

fn parse_apple_items(
    body: &Value,
    tracks: &mut Vec<SourceTrack>,
    skipped_count: &mut usize,
) -> AppResult<()> {
    let items = if let Some(items) = body.pointer("/data/0/relationships/tracks/data") {
        items.as_array().ok_or_else(|| {
            AppError::SourceApiIncompatible("Apple Music tracks 不是数组".to_string())
        })?
    } else {
        body.get("data").and_then(Value::as_array).ok_or_else(|| {
            AppError::SourceApiIncompatible("Apple Music 响应缺少 data 数组".to_string())
        })?
    };
    for item in items {
        match parse_apple_item(item) {
            Some(track) => tracks.push(track),
            None => *skipped_count += 1,
        }
    }
    Ok(())
}

pub(crate) fn parse_apple_item(item: &Value) -> Option<SourceTrack> {
    let attributes = item.get("attributes")?.as_object()?;
    if item
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| !matches!(kind, "songs" | "library-songs"))
        || (item.get("type").is_none()
            && attributes.get("kind").and_then(Value::as_str) != Some("song"))
    {
        return None;
    }
    let id = item.get("id")?.as_str()?.to_string();
    let canonical_id = (item.get("type").and_then(Value::as_str) == Some("library-songs"))
        .then(|| {
            attributes
                .get("playParams")
                .and_then(|play_params| play_params.pointer("/catalogId"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        })
        .flatten();
    let name = non_empty_string(attributes.get("name")?.as_str()?)?;
    let artist = non_empty_string(attributes.get("artistName")?.as_str()?)?;
    Some(SourceTrack {
        id,
        canonical_id,
        isrc: attributes
            .get("isrc")
            .and_then(Value::as_str)
            .map(str::to_string),
        name,
        artists: vec![artist],
        duration_ms: attributes.get("durationInMillis").and_then(Value::as_i64),
        album: attributes
            .get("albumName")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn apple_next(body: &Value) -> Option<&str> {
    body.pointer("/data/0/relationships/tracks/next")
        .and_then(Value::as_str)
        .or_else(|| body.get("next").and_then(Value::as_str))
}

fn apple_authorized_request(request: RequestBuilder, user_token: Option<&str>) -> RequestBuilder {
    if let Some(token) = user_token {
        request.header("Music-User-Token", token)
    } else {
        request
    }
}

fn apple_next_url(next: &str, api_base: &str) -> AppResult<String> {
    let base = Url::parse(api_base)
        .map_err(|_| AppError::SourceApiIncompatible("Apple Music API 主机配置无效".to_string()))?;
    let url = if next.starts_with('/') {
        let host = base.host_str().ok_or_else(|| {
            AppError::SourceApiIncompatible("Apple Music API 主机配置无效".to_string())
        })?;
        let port = base
            .port()
            .map_or_else(String::new, |port| format!(":{port}"));
        format!("{}://{host}{port}{next}", base.scheme())
    } else {
        next.to_string()
    };
    let parsed = Url::parse(&url)
        .map_err(|_| AppError::SourceApiIncompatible("Apple Music 分页 URL 无效".to_string()))?;
    if parsed.scheme() != base.scheme()
        || parsed.host_str() != base.host_str()
        || parsed.port_or_known_default() != base.port_or_known_default()
    {
        return Err(AppError::SourceApiIncompatible(
            "Apple Music 分页 URL 主机不受信任".to_string(),
        ));
    }
    Ok(url)
}

#[derive(Debug)]
struct YoutubeItem {
    id: String,
    title: String,
    channel: String,
}

fn load_youtube_music(
    http: &SourceHttp,
    source: &SourceRef,
    credentials: &SourceCredentials,
) -> AppResult<SourcePlaylist> {
    let access_token = credentials.youtube_access_token.as_deref();
    let key = credentials.youtube_api_key.as_deref();
    if access_token.is_none() && key.is_none() {
        return Err(AppError::SourceAuthRequired(
            "YouTube 需要 API key 或 OAuth access token".to_string(),
        ));
    }
    let endpoint = format!(
        "{}/playlistItems",
        http.endpoints.youtube_api.trim_end_matches('/')
    );
    let mut page_token = None;
    let mut seen_tokens = HashSet::new();
    let mut page_count = 0usize;
    let mut items = Vec::new();
    let mut skipped_count = 0;
    loop {
        page_count += 1;
        if page_count > MAX_SOURCE_PAGES {
            return Err(AppError::SourceApiIncompatible(
                "YouTube 分页超过安全上限".to_string(),
            ));
        }
        let mut request = youtube_authorized_request(
            http.client.get(&endpoint).query(&[
                ("part", "snippet,contentDetails"),
                ("playlistId", source.id.as_str()),
                ("maxResults", "50"),
            ]),
            access_token,
            key,
        );
        if let Some(token) = page_token.as_deref() {
            request = request.query(&[("pageToken", token)]);
        }
        let body = http.json(request, source.kind)?;
        let page_items = body.get("items").and_then(Value::as_array).ok_or_else(|| {
            AppError::SourceApiIncompatible("YouTube playlistItems 响应缺少 items 数组".to_string())
        })?;
        for item in page_items {
            let parsed = item
                .pointer("/snippet/resourceId/videoId")
                .or_else(|| item.pointer("/contentDetails/videoId"))
                .and_then(Value::as_str)
                .zip(item.pointer("/snippet/title").and_then(Value::as_str))
                .zip(
                    item.pointer("/snippet/channelTitle")
                        .and_then(Value::as_str),
                )
                .map(|((id, title), channel)| YoutubeItem {
                    id: id.to_string(),
                    title: title.to_string(),
                    channel: channel.to_string(),
                });
            if let Some(item) = parsed {
                items.push(item);
            } else {
                skipped_count += 1;
            }
        }
        let next = body
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string);
        let Some(next) = next else { break };
        if !seen_tokens.insert(next.clone()) {
            return Err(AppError::SourceApiIncompatible(
                "YouTube 分页出现循环".to_string(),
            ));
        }
        page_token = Some(next);
    }

    let mut tracks = Vec::new();
    for chunk in items.chunks(50) {
        let ids = chunk
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let body = http.json(
            youtube_authorized_request(
                http.client
                    .get(format!(
                        "{}/videos",
                        http.endpoints.youtube_api.trim_end_matches('/')
                    ))
                    .query(&[("part", "snippet,contentDetails"), ("id", ids.as_str())]),
                access_token,
                key,
            ),
            source.kind,
        )?;
        let videos = body.get("items").and_then(Value::as_array).ok_or_else(|| {
            AppError::SourceApiIncompatible("YouTube videos 响应缺少 items 数组".to_string())
        })?;
        let by_id = videos
            .iter()
            .filter_map(|video| {
                video
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| (id, video))
            })
            .collect::<HashMap<_, _>>();
        for item in chunk {
            let Some(video) = by_id.get(item.id.as_str()) else {
                skipped_count += 1;
                continue;
            };
            let Some(duration) = video
                .pointer("/contentDetails/duration")
                .and_then(Value::as_str)
                .and_then(parse_iso8601_duration_ms)
            else {
                skipped_count += 1;
                continue;
            };
            let title = video
                .pointer("/snippet/title")
                .and_then(Value::as_str)
                .unwrap_or(&item.title);
            let channel = video
                .pointer("/snippet/channelTitle")
                .and_then(Value::as_str)
                .unwrap_or(&item.channel);
            let (name, artists) = youtube_title_metadata(title, channel);
            if name.is_empty() || artists.is_empty() {
                skipped_count += 1;
                continue;
            }
            tracks.push(SourceTrack {
                id: item.id.clone(),
                canonical_id: None,
                isrc: None,
                name,
                artists,
                duration_ms: Some(duration),
                album: None,
            });
        }
    }
    Ok(SourcePlaylist {
        kind: source.kind,
        id: source.id.clone(),
        storefront: source.storefront.clone(),
        apple_library: source.apple_library,
        tracks,
        skipped_count,
    })
}

fn youtube_authorized_request(
    request: RequestBuilder,
    access_token: Option<&str>,
    key: Option<&str>,
) -> RequestBuilder {
    if let Some(token) = access_token {
        request.bearer_auth(token)
    } else {
        request.query(&[("key", key.unwrap_or_default())])
    }
}

pub(crate) fn youtube_title_metadata(title: &str, channel: &str) -> (String, Vec<String>) {
    let title = trim_video_suffixes(title.trim());
    if let Some((artist, rest)) = title.split_once(" - ") {
        let topic_prefix = "Topic - ";
        if rest.len() > topic_prefix.len()
            && rest[..topic_prefix.len()].eq_ignore_ascii_case(topic_prefix)
        {
            let name = trim_video_suffixes(rest[topic_prefix.len()..].trim());
            if !artist.trim().is_empty() && !name.is_empty() {
                return (name.to_string(), vec![artist.trim().to_string()]);
            }
        }
    }
    for separator in [" - ", " – ", " — ", " | ", " · "] {
        if let Some((artist, name)) = title.split_once(separator) {
            let artist = artist.trim();
            let name = trim_video_suffixes(name.trim());
            if !artist.is_empty() && !name.is_empty() {
                return (name.to_string(), vec![artist.to_string()]);
            }
        }
    }
    let channel = channel.trim();
    let topic_suffix = " - Topic";
    if channel.len() > topic_suffix.len()
        && channel[channel.len() - topic_suffix.len()..].eq_ignore_ascii_case(topic_suffix)
    {
        let artist = channel[..channel.len() - topic_suffix.len()].trim();
        if !artist.is_empty() {
            return (title.to_string(), vec![artist.to_string()]);
        }
    }
    (title.to_string(), Vec::new())
}

fn trim_video_suffixes(value: &str) -> &str {
    let mut value = value.trim();
    for suffix in [
        " (Official Audio)",
        " (Official Video)",
        " [Official Audio]",
        " [Official Video]",
        " (Audio)",
    ] {
        if value.len() >= suffix.len()
            && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        {
            value = value[..value.len() - suffix.len()].trim_end();
        }
    }
    value
}

pub(crate) fn parse_iso8601_duration_ms(value: &str) -> Option<i64> {
    let value = value.strip_prefix("PT")?;
    let mut digits = String::new();
    let mut total_ms = 0i64;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        let number = digits.parse::<i64>().ok()?;
        digits.clear();
        total_ms = total_ms.checked_add(match ch {
            'H' => number.checked_mul(3_600_000)?,
            'M' => number.checked_mul(60_000)?,
            'S' => number.checked_mul(1_000)?,
            _ => return None,
        })?;
    }
    (total_ms > 0).then_some(total_ms)
}

fn non_empty_string(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    fn test_server(responses: Vec<(u16, String)>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                requests.push(String::from_utf8_lossy(&request).to_string());
                let reason = if status == 200 { "OK" } else { "Unauthorized" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        (address, handle)
    }

    fn test_http(address: &str) -> SourceHttp {
        SourceHttp::with_endpoints(SourceEndpoints {
            spotify_api: format!("{address}/spotify"),
            apple_api: format!("{address}/apple"),
            youtube_api: format!("{address}/youtube"),
        })
        .unwrap()
    }

    #[test]
    fn parses_supported_playlist_urls_without_network() {
        let spotify = parse_source_url("https://open.spotify.com/playlist/abc_123").unwrap();
        assert_eq!(spotify.kind, SourceKind::Spotify);
        assert_eq!(spotify.id, "abc_123");

        let apple =
            parse_source_url("https://music.apple.com/cn/playlist/name/pl.u-abc_123").unwrap();
        assert_eq!(apple.kind, SourceKind::AppleMusic);
        assert_eq!(apple.storefront.as_deref(), Some("cn"));
        assert_eq!(apple.id, "pl.u-abc_123");
        assert!(!apple.apple_library);

        let library =
            parse_source_url("https://music.apple.com/cn/library/playlist/p.abc_123").unwrap();
        assert!(library.apple_library);
        assert_eq!(library.id, "p.abc_123");

        let youtube =
            parse_source_url("https://music.youtube.com/playlist?list=PL_abc-123").unwrap();
        assert_eq!(youtube.kind, SourceKind::YoutubeMusic);
        assert_eq!(youtube.id, "PL_abc-123");
    }

    #[test]
    fn rejects_unsupported_or_unsafe_source_urls() {
        assert!(parse_source_url("https://example.com/playlist/abc").is_err());
        assert!(parse_source_url("http://open.spotify.com/playlist/abc").is_err());
        assert!(parse_source_url("https://open.spotify.com/playlist/a%2Fb").is_err());
    }

    #[test]
    fn source_diagnostics_only_reports_configuration_booleans() {
        let value =
            source_diagnostics("https://music.apple.com/us/library/playlist/p.abc").unwrap();
        assert_eq!(value["source_kind"], "apple_music");
        assert_eq!(value["source_playlist_id"], "p.abc");
        assert_eq!(value["apple_library"], true);
        assert!(value["credentials"]["apple_music_developer_token"].is_boolean());
        assert!(value["credentials"]["apple_music_user_token"].is_boolean());
        assert!(!value.to_string().contains("TOKEN"));
    }

    #[test]
    fn parses_spotify_and_apple_track_metadata() {
        let spotify = parse_spotify_item(&json!({
            "track": {"type":"track","id":"s1","name":"Song","duration_ms":181000,
                "artists":[{"name":"Artist"}],"album":{"name":"Album"},
                "external_ids":{"isrc":"US-AAA-00-00001"}}
        }))
        .unwrap();
        assert_eq!(spotify.name, "Song");
        assert_eq!(spotify.duration_ms, Some(181000));
        assert_eq!(spotify.isrc.as_deref(), Some("US-AAA-00-00001"));

        let spotify_new_shape = parse_spotify_item(&json!({
            "item": {"type":"track","id":"s2","name":"Song 2","duration_ms":182000,
                "artists":[{"name":"Artist"}]}
        }))
        .unwrap();
        assert_eq!(spotify_new_shape.id, "s2");

        let apple = parse_apple_item(&json!({
            "id":"a1","type":"songs","attributes":{"kind":"song","name":"Song",
                "artistName":"Artist","albumName":"Album","durationInMillis":181000,
                "isrc":"US-AAA-00-00001"}
        }))
        .unwrap();
        assert_eq!(apple.artists, vec!["Artist"]);
        assert_eq!(apple.album.as_deref(), Some("Album"));
        assert_eq!(apple.isrc.as_deref(), Some("US-AAA-00-00001"));

        let library_apple = parse_apple_item(&json!({
            "id":"library-a1","type":"library-songs","attributes":{"name":"Song",
                "artistName":"Artist","playParams":{"catalogId":"catalog-a1"}}
        }))
        .unwrap();
        assert_eq!(library_apple.canonical_id.as_deref(), Some("catalog-a1"));
    }

    #[test]
    fn parses_youtube_durations_and_title_conservatively() {
        assert_eq!(parse_iso8601_duration_ms("PT1H2M3S"), Some(3_723_000));
        assert_eq!(parse_iso8601_duration_ms("PT0S"), None);
        assert_eq!(
            youtube_title_metadata("Artist - Song (Official Audio)", "Artist - Topic"),
            ("Song".to_string(), vec!["Artist".to_string()])
        );
        assert_eq!(
            youtube_title_metadata("Artist - Topic - Song", "Artist - Topic"),
            ("Song".to_string(), vec!["Artist".to_string()])
        );
        assert_eq!(
            youtube_title_metadata("Song", "Curator Channel"),
            ("Song".to_string(), Vec::<String>::new())
        );
    }

    #[test]
    fn spotify_loader_uses_bearer_auth_and_parses_a_real_page() {
        let (address, server) = test_server(vec![(
            200,
            json!({
                "total": 1,
                "items": [{"track": {"type":"track","id":"s1","name":"Song",
                    "duration_ms":181000,"artists":[{"name":"Artist"}],"album":{"name":"Album"}}}]
            })
            .to_string(),
        )]);
        let source = SourceRef {
            kind: SourceKind::Spotify,
            id: "pl1".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = SourceCredentials {
            spotify_token: Some("spotify-secret".to_string()),
            spotify_market: Some("US".to_string()),
            ..SourceCredentials::default()
        };
        let playlist = load_source_with(&test_http(&address), &source, &credentials).unwrap();
        assert_eq!(playlist.tracks.len(), 1);
        assert_eq!(playlist.tracks[0].artists, vec!["Artist"]);
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /spotify/playlists/pl1/items?"));
        assert!(requests[0].contains("market=US"));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer spotify-secret")
        );
    }

    #[test]
    fn apple_loader_parses_catalog_relationship_tracks() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"data":[{"id":"pl1","relationships":{"tracks":{"data":[{
                    "type":"songs","id":"a1","attributes":{"name":"Song","artistName":"Artist",
                        "albumName":"Album","durationInMillis":181000}
                }],"next":"/apple/catalog/us/playlists/pl1/tracks?offset=1"}}}]})
                .to_string(),
            ),
            (
                200,
                json!({"data":[{"type":"songs","id":"a2","attributes":{"name":"Song 2",
                    "artistName":"Artist","durationInMillis":182000}}]})
                .to_string(),
            ),
        ]);
        let source = SourceRef {
            kind: SourceKind::AppleMusic,
            id: "pl1".to_string(),
            storefront: Some("us".to_string()),
            apple_library: false,
        };
        let credentials = SourceCredentials {
            apple_developer_token: Some("apple-secret".to_string()),
            ..SourceCredentials::default()
        };
        let playlist = load_source_with(&test_http(&address), &source, &credentials).unwrap();
        assert_eq!(playlist.tracks.len(), 2);
        assert_eq!(playlist.tracks[0].name, "Song");
        assert_eq!(playlist.tracks[0].duration_ms, Some(181000));
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /apple/catalog/us/playlists/pl1"));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer apple-secret")
        );
        assert!(requests[1].contains("GET /apple/catalog/us/playlists/pl1/tracks?offset=1"));
    }

    #[test]
    fn apple_library_loader_sends_music_user_token() {
        let (address, server) = test_server(vec![(
            200,
            json!({"data":[{"id":"p1","relationships":{"tracks":{"data":[{
                "type":"library-songs","id":"a1","attributes":{"name":"Song",
                    "artistName":"Artist","durationInMillis":181000}
            }]}}}]})
            .to_string(),
        )]);
        let source = SourceRef {
            kind: SourceKind::AppleMusic,
            id: "p1".to_string(),
            storefront: Some("us".to_string()),
            apple_library: true,
        };
        let credentials = SourceCredentials {
            apple_developer_token: Some("apple-developer-secret".to_string()),
            apple_user_token: Some("apple-user-secret".to_string()),
            ..SourceCredentials::default()
        };
        let playlist = load_source_with(&test_http(&address), &source, &credentials).unwrap();
        assert_eq!(playlist.tracks.len(), 1);
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /apple/me/library/playlists/p1"));
        let request = requests[0].to_ascii_lowercase();
        assert!(request.contains("music-user-token: apple-user-secret"));
    }

    #[test]
    fn apple_loader_fetches_direct_relationship_when_playlist_omits_tracks() {
        let (address, server) = test_server(vec![
            (200, json!({"data":[{"id":"p1"}]}).to_string()),
            (
                200,
                json!({"data":[{"type":"songs","id":"a1","attributes":{"name":"Song",
                    "artistName":"Artist","durationInMillis":181000}}]})
                .to_string(),
            ),
        ]);
        let source = SourceRef {
            kind: SourceKind::AppleMusic,
            id: "p1".to_string(),
            storefront: Some("us".to_string()),
            apple_library: false,
        };
        let credentials = SourceCredentials {
            apple_developer_token: Some("apple-secret".to_string()),
            ..SourceCredentials::default()
        };
        let playlist = load_source_with(&test_http(&address), &source, &credentials).unwrap();
        assert_eq!(playlist.tracks[0].name, "Song");
        let requests = server.join().unwrap();
        assert!(requests[1].contains("GET /apple/catalog/us/playlists/p1/tracks"));
    }

    #[test]
    fn youtube_loader_fetches_playlist_items_then_video_durations() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"items":[{"snippet":{"resourceId":{"videoId":"v1"},
                    "title":"Artist - Song","channelTitle":"Artist"}}]})
                .to_string(),
            ),
            (
                200,
                json!({"items":[{"id":"v1","snippet":{"title":"Artist - Song",
                    "channelTitle":"Artist"},"contentDetails":{"duration":"PT3M1S"}}]})
                .to_string(),
            ),
        ]);
        let source = SourceRef {
            kind: SourceKind::YoutubeMusic,
            id: "PL1".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = SourceCredentials {
            youtube_api_key: Some("youtube-secret".to_string()),
            ..SourceCredentials::default()
        };
        let playlist = load_source_with(&test_http(&address), &source, &credentials).unwrap();
        assert_eq!(playlist.tracks[0].name, "Song");
        assert_eq!(playlist.tracks[0].duration_ms, Some(181000));
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /youtube/playlistItems?"));
        assert!(requests[0].contains("key=youtube-secret"));
        assert!(requests[1].contains("GET /youtube/videos?"));
    }

    #[test]
    fn youtube_loader_can_use_oauth_without_putting_a_key_in_the_url() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"items":[{"snippet":{"resourceId":{"videoId":"v1"},
                    "title":"Artist - Song","channelTitle":"Artist"}}]})
                .to_string(),
            ),
            (
                200,
                json!({"items":[{"id":"v1","snippet":{"title":"Artist - Song",
                    "channelTitle":"Artist"},"contentDetails":{"duration":"PT3M1S"}}]})
                .to_string(),
            ),
        ]);
        let source = SourceRef {
            kind: SourceKind::YoutubeMusic,
            id: "PL1".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = SourceCredentials {
            youtube_access_token: Some("youtube-oauth-secret".to_string()),
            ..SourceCredentials::default()
        };
        let playlist = load_source_with(&test_http(&address), &source, &credentials).unwrap();
        assert_eq!(playlist.tracks.len(), 1);
        let requests = server.join().unwrap();
        assert!(!requests[0].contains("key="));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer youtube-oauth-secret")
        );
    }

    #[test]
    fn source_http_errors_do_not_echo_response_body_or_credentials() {
        let (address, server) =
            test_server(vec![(401, "{\"error\":\"response-secret\"}".to_string())]);
        let source = SourceRef {
            kind: SourceKind::Spotify,
            id: "pl1".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = SourceCredentials {
            spotify_token: Some("token-secret".to_string()),
            ..SourceCredentials::default()
        };
        let error = load_source_with(&test_http(&address), &source, &credentials).unwrap_err();
        let message = error.to_string();
        assert!(!message.contains("response-secret"));
        assert!(!message.contains("token-secret"));
        assert!(matches!(error, AppError::SourceAuthRequired(_)));
        let _ = server.join().unwrap();
    }
}
