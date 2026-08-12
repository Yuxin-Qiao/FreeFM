//! Append-only target adapters for external playlist transfers.
//!
//! A transfer is intentionally narrower than a search-based importer: it can
//! copy a stable item id within the same service, or use a mapping explicitly
//! confirmed by review for a cross-service transfer. Search results are never
//! treated as mappings without that confirmation.

use crate::error::{AppError, AppResult};
use crate::plan::Candidate;
use crate::source::{
    SourceKind, SourcePlaylist, SourceRef, SourceTrack, parse_apple_item,
    parse_iso8601_duration_ms, youtube_title_metadata,
};
use crate::storage::{ExternalMappingStore, Paths, load_external_mappings, save_external_mappings};
use crate::trusted::CandidatePrompt;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
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
const YOUTUBE_TOKEN_ENV: &str = "FREEFM_YOUTUBE_ACCESS_TOKEN";
const SPOTIFY_API: &str = "https://api.spotify.com/v1";
const APPLE_API: &str = "https://api.music.apple.com/v1";
const YOUTUBE_API: &str = "https://www.googleapis.com/youtube/v3";
const MAX_TARGET_PAGES: usize = 10_000;
const MAX_APPEND_BATCH: usize = 100;

#[derive(Debug, Clone)]
struct TargetEndpoints {
    spotify_api: String,
    apple_api: String,
    youtube_api: String,
}

impl Default for TargetEndpoints {
    fn default() -> Self {
        Self {
            spotify_api: SPOTIFY_API.to_string(),
            apple_api: APPLE_API.to_string(),
            youtube_api: YOUTUBE_API.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TargetCredentials {
    spotify_token: Option<String>,
    spotify_market: Option<String>,
    apple_developer_token: Option<String>,
    apple_user_token: Option<String>,
    youtube_access_token: Option<String>,
}

struct TargetHttp {
    client: Client,
    endpoints: TargetEndpoints,
}

impl TargetHttp {
    fn new() -> AppResult<Self> {
        Self::with_endpoints(TargetEndpoints::default())
    }

    fn with_endpoints(endpoints: TargetEndpoints) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("FreeFM/0.1 (append-only playlist transfer)")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::TargetRemote("无法初始化目标平台 HTTP 客户端".to_string()))?;
        Ok(Self { client, endpoints })
    }

    fn json(&self, request: RequestBuilder, kind: SourceKind) -> AppResult<Value> {
        let response = self.send(request, kind)?;
        response.json::<Value>().map_err(|_| {
            AppError::TargetApiIncompatible(format!("{} API 响应不是 JSON", kind.label()))
        })
    }

    fn send(
        &self,
        request: RequestBuilder,
        kind: SourceKind,
    ) -> AppResult<reqwest::blocking::Response> {
        let response = request.send().map_err(|error| {
            if error.is_timeout() {
                AppError::TargetTimeout
            } else {
                AppError::TargetRemote(format!("{} API 请求失败", kind.label()))
            }
        })?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        Err(match status.as_u16() {
            401 | 403 => AppError::TargetAuthRequired(format!(
                "{} API 凭证无效、缺失或没有目标歌单写入权限",
                kind.label()
            )),
            429 => AppError::TargetRemote(format!("{} API 请求过于频繁", kind.label())),
            code => AppError::TargetRemote(format!("{} API HTTP {code}", kind.label())),
        })
    }
}

#[derive(Debug, Clone)]
struct TargetPlaylist {
    source: SourceRef,
    existing_ids: HashSet<String>,
}

pub(crate) fn transfer_source(
    paths: &Paths,
    source: &SourcePlaylist,
    target_input: &str,
    max_additions: Option<usize>,
) -> AppResult<Value> {
    let target = parse_target_url(target_input)?;
    if source.kind == SourceKind::AppleMusic && !target.apple_library {
        return Err(AppError::TargetReadOnly(
            "Apple Music 公开资料库歌单只能读取；目标必须是 /library/playlist/ 歌单".to_string(),
        ));
    }
    require_external_mappings(paths, source, &target)?;
    let credentials = TargetCredentials::from_env(&target)?;
    let http = TargetHttp::new()?;
    transfer_source_with(paths, source, &target, &credentials, &http, max_additions)
}

#[allow(clippy::too_many_arguments)]
fn transfer_source_with(
    paths: &Paths,
    source: &SourcePlaylist,
    target: &SourceRef,
    credentials: &TargetCredentials,
    http: &TargetHttp,
    max_additions: Option<usize>,
) -> AppResult<Value> {
    let same_platform = source.kind == target.kind;
    let (mappings, mappings_recovered) = if same_platform {
        (ExternalMappingStore::default(), false)
    } else {
        load_external_mappings(paths)?
    };
    if !same_platform {
        let missing_mappings = source
            .tracks
            .iter()
            .filter(|track| {
                !mappings
                    .mappings
                    .get(&external_mapping_key(source, target, &track.id))
                    .is_some_and(|mapping| mapping_matches(mapping, source, target))
            })
            .count();
        if missing_mappings > 0 {
            return Err(AppError::TargetMappingRequired(format!(
                "{} 首来源歌曲尚未确认 {} → {} 映射；请先运行 review --source ... --target ...",
                missing_mappings,
                source.kind.label(),
                target.kind.label()
            )));
        }
    }
    let mut target_playlist = load_target_with(http, target, credentials)?;
    let mut planned = Vec::new();
    let mut already_present = 0usize;
    let mut unsupported = 0usize;
    let limit = max_additions.unwrap_or(usize::MAX);
    let mut seen_in_source = HashSet::new();
    for track in &source.tracks {
        let id = if same_platform {
            direct_target_id(source, track)
        } else {
            mappings
                .mappings
                .get(&external_mapping_key(source, target, &track.id))
                .filter(|mapping| mapping_matches(mapping, source, target))
                .map(|mapping| mapping.target_id.clone())
        };
        let Some(id) = id else {
            if same_platform {
                unsupported += 1;
            }
            continue;
        };
        if !seen_in_source.insert(id.clone()) || target_playlist.existing_ids.contains(&id) {
            already_present += 1;
            continue;
        }
        if planned.len() >= limit {
            break;
        }
        planned.push(id);
    }

    let mut added = Vec::new();
    for chunk in planned.chunks(MAX_APPEND_BATCH) {
        let append_result = append_target(http, &target_playlist.source, credentials, chunk);
        let reread = match append_result {
            Ok(()) => load_target_with(http, target, credentials).map_err(|error| {
                AppError::TargetWriteUncertain(format!(
                    "追加请求已发出，但无法复读目标歌单确认结果：{error}"
                ))
            })?,
            Err(error) if matches!(&error, AppError::TargetTimeout | AppError::TargetRemote(_)) => {
                // A transport error can race with a successful provider-side
                // write. Read the target immediately so callers do not need
                // to blindly retry a request whose outcome is unknown.
                load_target_with(http, target, credentials).map_err(|reread_error| {
                    AppError::TargetWriteUncertain(format!(
                        "追加请求结果不确定（{error}），且无法复读目标歌单：{reread_error}"
                    ))
                })?
            }
            Err(error) => return Err(error),
        };
        let missing = chunk
            .iter()
            .filter(|id| !reread.existing_ids.contains(*id))
            .count();
        if missing > 0 {
            return Err(AppError::TargetWriteUncertain(format!(
                "追加请求已发出，但复读后仍缺少 {missing} 首目标曲目；请勿自动重试"
            )));
        }
        target_playlist = reread;
        for id in chunk {
            added.push(id.clone());
        }
    }

    Ok(json!({
        "ok": true,
        "transfer_kind": if same_platform {
            "same_platform_direct_id"
        } else {
            "cross_platform_reviewed_mapping"
        },
        "source_kind": source.kind,
        "source_playlist_id": source.id,
        "target_kind": target.kind,
        "target_playlist_id": target.id,
        "target_apple_library": target.apple_library,
        "source_track_count": source.tracks.len(),
        "source_skipped_count": source.skipped_count,
        "already_present_count": already_present,
        "unsupported_count": unsupported,
        "would_add_ids": added,
        "added_count": added.len(),
        "max_additions": max_additions,
        "mapping_corrupt_recovered": mappings_recovered,
        "append_only": true,
    }))
}

fn direct_target_id(source: &SourcePlaylist, track: &crate::source::SourceTrack) -> Option<String> {
    if source.kind == SourceKind::AppleMusic && source.apple_library {
        return track.canonical_id.clone();
    }
    Some(track.id.clone())
}

fn external_mapping_key(source: &SourcePlaylist, target: &SourceRef, track_id: &str) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        source.kind.slug(),
        source.id,
        source.storefront.as_deref().unwrap_or("-"),
        target.kind.slug(),
        target.id,
        target.storefront.as_deref().unwrap_or("-"),
        track_id,
    )
}

fn mapping_matches(
    mapping: &crate::storage::ExternalMapping,
    source: &SourcePlaylist,
    target: &SourceRef,
) -> bool {
    mapping.source_kind == source.kind.slug()
        && mapping.source_playlist_id == source.id
        && mapping.source_storefront.as_deref() == source.storefront.as_deref()
        && mapping.target_kind == target.kind.slug()
        && mapping.target_playlist_id == target.id
        && mapping.target_storefront.as_deref() == target.storefront.as_deref()
        && valid_item_id(&mapping.target_id)
}

fn require_external_mappings(
    paths: &Paths,
    source: &SourcePlaylist,
    target: &SourceRef,
) -> AppResult<()> {
    if source.kind == target.kind {
        return Ok(());
    }
    let (mappings, _) = load_external_mappings(paths)?;
    let missing_mappings = source
        .tracks
        .iter()
        .filter(|track| {
            !mappings
                .mappings
                .get(&external_mapping_key(source, target, &track.id))
                .is_some_and(|mapping| mapping_matches(mapping, source, target))
        })
        .count();
    if missing_mappings > 0 {
        return Err(AppError::TargetMappingRequired(format!(
            "{} 首来源歌曲尚未确认 {} → {} 映射；请先运行 review --source ... --target ...",
            missing_mappings,
            source.kind.label(),
            target.kind.label()
        )));
    }
    Ok(())
}

fn valid_item_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn parse_target_url(input: &str) -> AppResult<SourceRef> {
    let source = crate::source::parse_source_url(input)
        .map_err(|error| AppError::TargetUrlInvalid(error.to_string()))?;
    if source.kind == SourceKind::AppleMusic && !source.apple_library {
        return Err(AppError::TargetReadOnly(
            "Apple Music 目录歌单不可作为写入目标".to_string(),
        ));
    }
    Ok(source)
}

/// Search a target catalog for review-only candidates. This function never
/// writes a playlist and never chooses a candidate on its own.
fn search_target_tracks(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
    original: &SourceTrack,
) -> AppResult<Vec<SourceTrack>> {
    let query = format!("{} {}", original.artists.join(" "), original.name);
    match target.kind {
        SourceKind::Spotify => {
            let token = credentials
                .spotify_token
                .as_deref()
                .ok_or_else(|| AppError::TargetAuthRequired("Spotify token 缺失".to_string()))?;
            let endpoint = format!(
                "{}/search",
                http.endpoints.spotify_api.trim_end_matches('/')
            );
            let mut request = http.client.get(endpoint).bearer_auth(token).query(&[
                ("q", query.as_str()),
                ("type", "track"),
                ("limit", "10"),
            ]);
            if let Some(market) = credentials.spotify_market.as_deref() {
                request = request.query(&[("market", market)]);
            }
            let body = http.json(request, target.kind)?;
            let items = body
                .pointer("/tracks/items")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AppError::TargetApiIncompatible(
                        "Spotify 搜索响应缺少 tracks.items 数组".to_string(),
                    )
                })?;
            Ok(items.iter().filter_map(parse_spotify_search_item).collect())
        }
        SourceKind::AppleMusic => {
            let developer = credentials
                .apple_developer_token
                .as_deref()
                .ok_or_else(|| {
                    AppError::TargetAuthRequired("Apple Music developer token 缺失".to_string())
                })?;
            let storefront = target.storefront.as_deref().unwrap_or("us");
            let body = http.json(
                http.client
                    .get(format!(
                        "{}/catalog/{storefront}/search",
                        http.endpoints.apple_api.trim_end_matches('/')
                    ))
                    .header(AUTHORIZATION, format!("Bearer {developer}"))
                    .query(&[
                        ("term", query.as_str()),
                        ("types", "songs"),
                        ("limit", "10"),
                    ]),
                target.kind,
            )?;
            let items = body
                .pointer("/results/songs/data")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AppError::TargetApiIncompatible(
                        "Apple Music 搜索响应缺少 results.songs.data 数组".to_string(),
                    )
                })?;
            Ok(items.iter().filter_map(parse_apple_item).collect())
        }
        SourceKind::YoutubeMusic => {
            let token = credentials.youtube_access_token.as_deref().ok_or_else(|| {
                AppError::TargetAuthRequired(
                    "YouTube 搜索和写入必须使用 OAuth access token".to_string(),
                )
            })?;
            let base = http.endpoints.youtube_api.trim_end_matches('/');
            let search = http.json(
                http.client
                    .get(format!("{base}/search"))
                    .bearer_auth(token)
                    .query(&[
                        ("part", "snippet"),
                        ("q", query.as_str()),
                        ("type", "video"),
                        ("maxResults", "10"),
                    ]),
                target.kind,
            )?;
            let items = search
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AppError::TargetApiIncompatible("YouTube 搜索响应缺少 items 数组".to_string())
                })?;
            let search_items = items
                .iter()
                .filter_map(|item| {
                    let id = item.pointer("/id/videoId").and_then(Value::as_str)?;
                    let title = item.pointer("/snippet/title").and_then(Value::as_str)?;
                    let channel = item
                        .pointer("/snippet/channelTitle")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    Some((id.to_string(), title.to_string(), channel.to_string()))
                })
                .collect::<Vec<_>>();
            if search_items.is_empty() {
                return Ok(Vec::new());
            }
            let ids = search_items
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let videos = http.json(
                http.client
                    .get(format!("{base}/videos"))
                    .bearer_auth(token)
                    .query(&[("part", "snippet,contentDetails"), ("id", ids.as_str())]),
                target.kind,
            )?;
            let by_id = videos
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AppError::TargetApiIncompatible(
                        "YouTube videos 响应缺少 items 数组".to_string(),
                    )
                })?
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str).map(|id| (id, item)))
                .collect::<HashMap<_, _>>();
            Ok(search_items
                .into_iter()
                .filter_map(|(id, fallback_title, fallback_channel)| {
                    let video = by_id.get(id.as_str()).copied();
                    let title = video
                        .and_then(|item| item.pointer("/snippet/title").and_then(Value::as_str))
                        .unwrap_or(fallback_title.as_str());
                    let channel = video
                        .and_then(|item| {
                            item.pointer("/snippet/channelTitle")
                                .and_then(Value::as_str)
                        })
                        .unwrap_or(fallback_channel.as_str());
                    let (name, artists) = youtube_title_metadata(title, channel);
                    if name.is_empty() || artists.is_empty() {
                        return None;
                    }
                    let duration_ms = video.and_then(|item| {
                        item.pointer("/contentDetails/duration")
                            .and_then(Value::as_str)
                            .and_then(parse_iso8601_duration_ms)
                    });
                    Some(SourceTrack {
                        id,
                        canonical_id: None,
                        isrc: None,
                        name,
                        artists,
                        duration_ms,
                        album: None,
                    })
                })
                .collect())
        }
    }
}

fn parse_spotify_search_item(item: &Value) -> Option<SourceTrack> {
    if item.get("type").and_then(Value::as_str) != Some("track") {
        return None;
    }
    let id = item.get("id")?.as_str()?.to_string();
    let name = item.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }
    let artists = item
        .get("artists")?
        .as_array()?
        .iter()
        .filter_map(|artist| artist.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|artist| !artist.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if artists.is_empty() {
        return None;
    }
    Some(SourceTrack {
        id,
        canonical_id: None,
        isrc: item
            .pointer("/external_ids/isrc")
            .and_then(Value::as_str)
            .map(str::to_string),
        name: name.to_string(),
        artists,
        duration_ms: item.get("duration_ms").and_then(Value::as_i64),
        album: item
            .pointer("/album/name")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn target_candidates(original: &SourceTrack, raw: Vec<SourceTrack>) -> Vec<Candidate> {
    let mut candidates = raw
        .into_iter()
        .filter_map(|track| {
            let score = recording_match_score(original, &track)?;
            let duration_delta_ms = original
                .duration_ms
                .zip(track.duration_ms)
                .map(|(left, right)| right - left);
            Some(Candidate {
                id: track.id,
                title: track.name,
                artist: track.artists.join(", "),
                duration_ms: track.duration_ms,
                duration_delta_ms,
                album: track.album,
                version_markers: Vec::new(),
                score,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.id.clone()))
        .take(3)
        .collect()
}

fn recording_match_score(original: &SourceTrack, candidate: &SourceTrack) -> Option<f32> {
    let title_matches = normalize_for_match(&original.name) == normalize_for_match(&candidate.name);
    let artist_matches = original.artists.iter().any(|artist| {
        candidate
            .artists
            .iter()
            .any(|other| normalize_for_match(artist) == normalize_for_match(other))
    });
    if !title_matches || !artist_matches {
        return None;
    }
    let mut score = 0.8f32;
    if original.isrc.is_some() && original.isrc == candidate.isrc {
        score += 0.18;
    }
    if let Some(delta) = original
        .duration_ms
        .zip(candidate.duration_ms)
        .map(|(left, right)| (right - left).abs())
    {
        if delta > 15_000 {
            return None;
        }
        score += if delta <= 2_000 { 0.02 } else { 0.005 };
    }
    Some(score.min(1.0))
}

fn normalize_for_match(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn render_target_prompt(prompt: &CandidatePrompt, target: SourceKind, json: bool) {
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
        format!("{} 候选（仅供人工确认）：", target.label()),
    ];
    for (index, candidate) in prompt.candidates.iter().enumerate() {
        let delta = candidate
            .duration_delta_ms
            .map(|value| format!("{:+.1}s", value as f64 / 1000.0))
            .unwrap_or_else(|| "未知".to_string());
        lines.push(format!(
            "[{}] {} - {}",
            index + 1,
            candidate.title,
            candidate.artist
        ));
        lines.push(format!(
            "    专辑：{}；时长差：{delta}",
            candidate.album.as_deref().unwrap_or("未知")
        ));
    }
    lines.extend([
        "[0] 跳过".to_string(),
        format!("候选原因：{}", prompt.reason),
        "FreeFM 不会自动判断跨平台候选，请你本人确认。".to_string(),
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn review_source_to_target<
    S: FnMut(&CandidatePrompt) -> Option<usize>,
    F: FnMut() -> bool,
>(
    paths: &Paths,
    source: &SourcePlaylist,
    target_input: &str,
    json: bool,
    select: S,
    confirm: F,
) -> AppResult<Value> {
    let target = parse_target_url(target_input)?;
    let credentials = TargetCredentials::from_env(&target)?;
    let http = TargetHttp::new()?;
    review_source_to_target_with(
        paths,
        source,
        &target,
        &credentials,
        &http,
        json,
        select,
        confirm,
    )
}

#[allow(clippy::too_many_arguments)]
fn review_source_to_target_with<S: FnMut(&CandidatePrompt) -> Option<usize>, F: FnMut() -> bool>(
    paths: &Paths,
    source: &SourcePlaylist,
    target: &SourceRef,
    credentials: &TargetCredentials,
    http: &TargetHttp,
    json: bool,
    mut select: S,
    mut confirm: F,
) -> AppResult<Value> {
    if source.kind == target.kind {
        return Ok(json!({
            "ok": true,
            "review_kind": "same_platform_direct_id",
            "source_kind": source.kind,
            "target_kind": target.kind,
            "approved_count": 0,
            "skipped_count": 0,
            "reason": "同平台复制使用稳定曲目 id，不需要跨平台映射 review",
        }));
    }
    let _target_playlist = load_target_with(http, target, credentials)?;
    let (mut mappings, mappings_recovered) = load_external_mappings(paths)?;
    let mut approved_count = 0usize;
    let mut existing_count = 0usize;
    let mut skipped_count = 0usize;
    let mut seen = HashSet::new();

    for track in &source.tracks {
        let source_key = external_mapping_key(source, target, &track.id);
        if !seen.insert(source_key.clone()) {
            continue;
        }
        if mappings
            .mappings
            .get(&source_key)
            .is_some_and(|mapping| mapping_matches(mapping, source, target))
        {
            existing_count += 1;
            continue;
        }
        let candidates = target_candidates(
            track,
            search_target_tracks(http, target, credentials, track)?,
        );
        if candidates.is_empty() {
            skipped_count += 1;
            continue;
        }
        let prompt = CandidatePrompt {
            original_id: source_key.clone(),
            original_title: track.name.clone(),
            original_artist: track.artists.join(", "),
            original_duration_ms: track.duration_ms,
            candidates,
            reason: "标题、歌手和可用时长信息达到 review 候选门槛".to_string(),
        };
        render_target_prompt(&prompt, target.kind, json);
        let Some(index) = select(&prompt) else {
            skipped_count += 1;
            continue;
        };
        let Some(candidate) = prompt.candidates.get(index) else {
            skipped_count += 1;
            continue;
        };
        if json {
            eprintln!("已选择：{} - {}", candidate.title, candidate.artist);
            eprintln!("确认保存跨平台 mapping？[y/N]");
        } else {
            println!("已选择：{} - {}", candidate.title, candidate.artist);
            println!("确认保存跨平台 mapping？[y/N]");
        }
        if !confirm() {
            skipped_count += 1;
            continue;
        }
        mappings.approve(
            &source_key,
            source.kind.slug(),
            &source.id,
            source.storefront.as_deref(),
            target.kind.slug(),
            &target.id,
            target.storefront.as_deref(),
            &candidate.id,
        );
        save_external_mappings(paths, &mappings)?;
        approved_count += 1;
    }

    Ok(json!({
        "ok": true,
        "review_kind": "cross_platform_mapping",
        "source_kind": source.kind,
        "source_playlist_id": source.id,
        "target_kind": target.kind,
        "target_playlist_id": target.id,
        "approved_count": approved_count,
        "existing_mapping_count": existing_count,
        "skipped_count": skipped_count,
        "mapping_corrupt_recovered": mappings_recovered,
        "remote_write": false,
    }))
}

impl TargetCredentials {
    fn from_env(target: &SourceRef) -> AppResult<Self> {
        let mut credentials = Self::default();
        match target.kind {
            SourceKind::Spotify => {
                credentials.spotify_token = Some(required_env(
                    SPOTIFY_TOKEN_ENV,
                    "Spotify playlist-read-private plus playlist-modify-public 或 playlist-modify-private scope",
                )?);
                credentials.spotify_market = env::var(SPOTIFY_MARKET_ENV)
                    .ok()
                    .filter(|value| !value.trim().is_empty());
            }
            SourceKind::AppleMusic => {
                credentials.apple_developer_token = Some(required_env(
                    APPLE_TOKEN_ENV,
                    "Apple Music developer token",
                )?);
                credentials.apple_user_token = Some(required_env(
                    APPLE_USER_TOKEN_ENV,
                    "Apple Music Music User Token",
                )?);
            }
            SourceKind::YoutubeMusic => {
                credentials.youtube_access_token = Some(required_env(
                    YOUTUBE_TOKEN_ENV,
                    "YouTube OAuth access token",
                )?);
            }
        }
        Ok(credentials)
    }
}

fn required_env(name: &str, capability: &str) -> AppResult<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::TargetAuthRequired(format!("请设置环境变量 {name}（需要 {capability}）"))
        })
}

/// Report target credential readiness without contacting a provider or
/// exposing credential values. Remote ownership and Apple `canEdit` are still
/// checked only by the read-before-write path.
pub(crate) fn target_diagnostics(input: &str) -> AppResult<Value> {
    let target = parse_target_url(input)?;
    let configured = |name: &str| {
        env::var(name)
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    };
    let (ready, credentials, required_scopes) = match target.kind {
        SourceKind::Spotify => (
            configured(SPOTIFY_TOKEN_ENV),
            json!({
                "spotify_token": configured(SPOTIFY_TOKEN_ENV),
                "spotify_market_optional": configured(SPOTIFY_MARKET_ENV),
            }),
            json!([
                "playlist-read-private",
                "playlist-modify-public or playlist-modify-private"
            ]),
        ),
        SourceKind::AppleMusic => (
            configured(APPLE_TOKEN_ENV) && configured(APPLE_USER_TOKEN_ENV),
            json!({
                "apple_music_developer_token": configured(APPLE_TOKEN_ENV),
                "apple_music_user_token": configured(APPLE_USER_TOKEN_ENV),
                "can_edit_remote_check": "required"
            }),
            json!(["Music User Token for /me/library and playlist writes"]),
        ),
        SourceKind::YoutubeMusic => (
            configured(YOUTUBE_TOKEN_ENV),
            json!({
                "youtube_access_token": configured(YOUTUBE_TOKEN_ENV),
                "youtube_api_key_not_sufficient_for_write": true
            }),
            json!(["https://www.googleapis.com/auth/youtube.force-ssl"]),
        ),
    };
    Ok(json!({
        "target_kind": target.kind,
        "target_playlist_id": target.id,
        "target_storefront": target.storefront,
        "target_apple_library": target.apple_library,
        "ready": ready,
        "remote_write": true,
        "credentials": credentials,
        "required_scopes": required_scopes,
        "remote_checks_before_write": ["target ownership", "current items", "append response reread"],
        "oauth_login": "not_started_by_freefm; provide a short-lived provider token through the environment",
    }))
}

fn load_target_with(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
) -> AppResult<TargetPlaylist> {
    let existing_ids = match target.kind {
        SourceKind::Spotify => load_spotify_target(http, target, credentials)?,
        SourceKind::AppleMusic => load_apple_target(http, target, credentials)?,
        SourceKind::YoutubeMusic => load_youtube_target(http, target, credentials)?,
    };
    Ok(TargetPlaylist {
        source: target.clone(),
        existing_ids,
    })
}

fn load_spotify_target(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
) -> AppResult<HashSet<String>> {
    let token = credentials
        .spotify_token
        .as_deref()
        .ok_or_else(|| AppError::TargetAuthRequired("Spotify token 缺失".to_string()))?;
    let base = http.endpoints.spotify_api.trim_end_matches('/');
    let me = http.json(
        http.client.get(format!("{base}/me")).bearer_auth(token),
        target.kind,
    )?;
    let user_id = me
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            AppError::TargetApiIncompatible("Spotify /me 响应缺少用户 id".to_string())
        })?;
    let playlist = http.json(
        http.client
            .get(format!("{base}/playlists/{}", target.id))
            .bearer_auth(token),
        target.kind,
    )?;
    let owner_id = playlist
        .pointer("/owner/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            AppError::TargetApiIncompatible("Spotify 歌单响应缺少 owner id".to_string())
        })?;
    if owner_id != user_id {
        return Err(AppError::TargetNotOwned(
            "Spotify 目标歌单不属于当前 OAuth 用户".to_string(),
        ));
    }
    let endpoint = format!("{base}/playlists/{}/items", target.id);
    let mut offset = 0u32;
    let mut page_count = 0usize;
    let mut existing = HashSet::new();
    loop {
        page_count += 1;
        if page_count > MAX_TARGET_PAGES {
            return Err(AppError::TargetApiIncompatible(
                "Spotify 目标歌单分页超过安全上限".to_string(),
            ));
        }
        let body = http.json(
            http.client
                .get(&endpoint)
                .bearer_auth(token)
                .query(&[("limit", 50u32), ("offset", offset)]),
            target.kind,
        )?;
        let items = body.get("items").and_then(Value::as_array).ok_or_else(|| {
            AppError::TargetApiIncompatible("Spotify 目标歌单响应缺少 items 数组".to_string())
        })?;
        for item in items {
            if let Some(id) = item
                .get("item")
                .or_else(|| item.get("track"))
                .and_then(|track| track.get("id"))
                .and_then(Value::as_str)
            {
                existing.insert(id.to_string());
            }
        }
        if items.len() < 50
            || body
                .get("total")
                .and_then(Value::as_u64)
                .is_some_and(|total| offset as u64 + items.len() as u64 >= total)
        {
            break;
        }
        offset = offset.saturating_add(items.len() as u32);
    }
    Ok(existing)
}

fn load_apple_target(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
) -> AppResult<HashSet<String>> {
    let developer = credentials
        .apple_developer_token
        .as_deref()
        .ok_or_else(|| {
            AppError::TargetAuthRequired("Apple Music developer token 缺失".to_string())
        })?;
    let user = credentials.apple_user_token.as_deref().ok_or_else(|| {
        AppError::TargetAuthRequired("Apple Music Music User Token 缺失".to_string())
    })?;
    let base = http.endpoints.apple_api.trim_end_matches('/');
    let playlist = http.json(
        apple_request(
            http.client
                .get(format!("{base}/me/library/playlists/{}", target.id))
                .header(AUTHORIZATION, format!("Bearer {developer}")),
            user,
        ),
        target.kind,
    )?;
    let resource = playlist.pointer("/data/0").ok_or_else(|| {
        AppError::TargetApiIncompatible("Apple Music 目标歌单响应缺少 data".to_string())
    })?;
    if resource.get("type").and_then(Value::as_str) != Some("library-playlists")
        || resource.get("id").and_then(Value::as_str) != Some(target.id.as_str())
    {
        return Err(AppError::TargetNotOwned(
            "Apple Music 目标不是当前 Music User Token 下的资料库歌单".to_string(),
        ));
    }
    match resource
        .pointer("/attributes/canEdit")
        .and_then(Value::as_bool)
    {
        Some(true) => {}
        Some(false) => {
            return Err(AppError::TargetReadOnly(
                "Apple Music 目标歌单不可编辑".to_string(),
            ));
        }
        None => {
            return Err(AppError::TargetApiIncompatible(
                "Apple Music 目标歌单响应缺少 canEdit".to_string(),
            ));
        }
    }
    let mut url = format!("{base}/me/library/playlists/{}/tracks", target.id);
    let mut seen_urls = HashSet::new();
    let mut existing = HashSet::new();
    for page in 0..MAX_TARGET_PAGES {
        if !seen_urls.insert(url.clone()) {
            return Err(AppError::TargetApiIncompatible(
                "Apple Music 目标歌单分页出现循环".to_string(),
            ));
        }
        let body = http.json(
            apple_request(
                http.client
                    .get(&url)
                    .header(AUTHORIZATION, format!("Bearer {developer}")),
                user,
            ),
            target.kind,
        )?;
        let items = body.get("data").and_then(Value::as_array).ok_or_else(|| {
            AppError::TargetApiIncompatible(
                "Apple Music 目标歌单 tracks 缺少 data 数组".to_string(),
            )
        })?;
        for item in items {
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                existing.insert(id.to_string());
            }
            if let Some(catalog_id) = item
                .pointer("/attributes/playParams/catalogId")
                .and_then(Value::as_str)
            {
                existing.insert(catalog_id.to_string());
            }
        }
        let Some(next) = body.get("next").and_then(Value::as_str) else {
            break;
        };
        url = apple_next_url(next, &http.endpoints.apple_api)?;
        if page + 1 == MAX_TARGET_PAGES {
            return Err(AppError::TargetApiIncompatible(
                "Apple Music 目标歌单分页超过安全上限".to_string(),
            ));
        }
    }
    Ok(existing)
}

fn load_youtube_target(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
) -> AppResult<HashSet<String>> {
    let token = credentials.youtube_access_token.as_deref().ok_or_else(|| {
        AppError::TargetAuthRequired("YouTube 写入必须使用 OAuth access token".to_string())
    })?;
    let base = http.endpoints.youtube_api.trim_end_matches('/');
    let channel = http.json(
        http.client
            .get(format!("{base}/channels"))
            .bearer_auth(token)
            .query(&[("part", "id"), ("mine", "true")]),
        target.kind,
    )?;
    let channel_ids = channel
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::TargetApiIncompatible("YouTube mine channel 响应缺少 items 数组".to_string())
        })?
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    if channel_ids.is_empty() {
        return Err(AppError::TargetApiIncompatible(
            "YouTube mine channel 响应缺少 id".to_string(),
        ));
    }
    let playlist = http.json(
        http.client
            .get(format!("{base}/playlists"))
            .bearer_auth(token)
            .query(&[("part", "snippet"), ("id", target.id.as_str())]),
        target.kind,
    )?;
    let playlist_channel = playlist
        .pointer("/items/0/snippet/channelId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::TargetApiIncompatible("YouTube 目标歌单响应缺少 channelId".to_string())
        })?;
    if !channel_ids.contains(playlist_channel) {
        return Err(AppError::TargetNotOwned(
            "YouTube 目标歌单不属于当前 OAuth 用户的频道".to_string(),
        ));
    }
    let mut page_token = None;
    let mut seen_tokens = HashSet::new();
    let mut existing = HashSet::new();
    for page in 0..MAX_TARGET_PAGES {
        let mut request = http
            .client
            .get(format!("{base}/playlistItems"))
            .bearer_auth(token)
            .query(&[
                ("part", "snippet,contentDetails"),
                ("playlistId", target.id.as_str()),
                ("maxResults", "50"),
            ]);
        if let Some(page_token) = page_token.as_deref() {
            request = request.query(&[("pageToken", page_token)]);
        }
        let body = http.json(request, target.kind)?;
        let items = body.get("items").and_then(Value::as_array).ok_or_else(|| {
            AppError::TargetApiIncompatible("YouTube 目标歌单响应缺少 items 数组".to_string())
        })?;
        for item in items {
            if let Some(id) = item
                .pointer("/snippet/resourceId/videoId")
                .or_else(|| item.pointer("/contentDetails/videoId"))
                .and_then(Value::as_str)
            {
                existing.insert(id.to_string());
            }
        }
        let Some(next) = body
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
        else {
            break;
        };
        if !seen_tokens.insert(next.clone()) {
            return Err(AppError::TargetApiIncompatible(
                "YouTube 目标歌单分页出现循环".to_string(),
            ));
        }
        if page + 1 == MAX_TARGET_PAGES {
            return Err(AppError::TargetApiIncompatible(
                "YouTube 目标歌单分页超过安全上限".to_string(),
            ));
        }
        page_token = Some(next);
    }
    Ok(existing)
}

fn append_target(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
    ids: &[String],
) -> AppResult<()> {
    match target.kind {
        SourceKind::Spotify => append_spotify(http, target, credentials, ids),
        SourceKind::AppleMusic => append_apple(http, target, credentials, ids),
        SourceKind::YoutubeMusic => append_youtube(http, target, credentials, ids),
    }
}

fn append_spotify(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
    ids: &[String],
) -> AppResult<()> {
    let token = credentials
        .spotify_token
        .as_deref()
        .ok_or_else(|| AppError::TargetAuthRequired("Spotify token 缺失".to_string()))?;
    let uris = ids
        .iter()
        .map(|id| format!("spotify:track:{id}"))
        .collect::<Vec<_>>();
    http.send(
        http.client
            .post(format!(
                "{}/playlists/{}/items",
                http.endpoints.spotify_api.trim_end_matches('/'),
                target.id
            ))
            .bearer_auth(token)
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({"uris": uris})),
        target.kind,
    )?;
    Ok(())
}

fn append_apple(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
    ids: &[String],
) -> AppResult<()> {
    let developer = credentials
        .apple_developer_token
        .as_deref()
        .ok_or_else(|| {
            AppError::TargetAuthRequired("Apple Music developer token 缺失".to_string())
        })?;
    let user = credentials.apple_user_token.as_deref().ok_or_else(|| {
        AppError::TargetAuthRequired("Apple Music Music User Token 缺失".to_string())
    })?;
    let data = ids
        .iter()
        .map(|id| json!({"id": id, "type": "songs"}))
        .collect::<Vec<_>>();
    http.send(
        apple_request(
            http.client
                .post(format!(
                    "{}/me/library/playlists/{}/tracks",
                    http.endpoints.apple_api.trim_end_matches('/'),
                    target.id
                ))
                .header(AUTHORIZATION, format!("Bearer {developer}"))
                .header(CONTENT_TYPE, "application/json")
                .json(&json!({"data": data})),
            user,
        ),
        target.kind,
    )?;
    Ok(())
}

fn append_youtube(
    http: &TargetHttp,
    target: &SourceRef,
    credentials: &TargetCredentials,
    ids: &[String],
) -> AppResult<()> {
    let token = credentials
        .youtube_access_token
        .as_deref()
        .ok_or_else(|| AppError::TargetAuthRequired("YouTube access token 缺失".to_string()))?;
    for id in ids {
        http.send(
            http.client
                .post(format!(
                    "{}/playlistItems",
                    http.endpoints.youtube_api.trim_end_matches('/')
                ))
                .bearer_auth(token)
                .query(&[("part", "snippet")])
                .header(CONTENT_TYPE, "application/json")
                .json(&json!({
                    "snippet": {
                        "playlistId": target.id,
                        "resourceId": {"kind": "youtube#video", "videoId": id}
                    }
                })),
            target.kind,
        )?;
    }
    Ok(())
}

fn apple_request(request: RequestBuilder, user_token: &str) -> RequestBuilder {
    request.header("Music-User-Token", user_token)
}

fn apple_next_url(next: &str, api_base: &str) -> AppResult<String> {
    let base = Url::parse(api_base)
        .map_err(|_| AppError::TargetApiIncompatible("Apple Music API 主机配置无效".to_string()))?;
    let url = if next.starts_with('/') {
        let host = base.host_str().ok_or_else(|| {
            AppError::TargetApiIncompatible("Apple Music API 主机配置无效".to_string())
        })?;
        let port = base
            .port()
            .map_or_else(String::new, |port| format!(":{port}"));
        format!("{}://{host}{port}{next}", base.scheme())
    } else {
        next.to_string()
    };
    let parsed = Url::parse(&url)
        .map_err(|_| AppError::TargetApiIncompatible("Apple Music 分页 URL 无效".to_string()))?;
    if parsed.scheme() != base.scheme()
        || parsed.host_str() != base.host_str()
        || parsed.port_or_known_default() != base.port_or_known_default()
    {
        return Err(AppError::TargetApiIncompatible(
            "Apple Music 分页 URL 主机不受信任".to_string(),
        ));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceTrack;
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
                let reason = if status == 204 { "No Content" } else { "OK" };
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

    fn test_http(address: &str) -> TargetHttp {
        TargetHttp::with_endpoints(TargetEndpoints {
            spotify_api: format!("{address}/spotify"),
            apple_api: format!("{address}/apple"),
            youtube_api: format!("{address}/youtube"),
        })
        .unwrap()
    }

    fn source(kind: SourceKind, apple_library: bool, id: &str) -> SourcePlaylist {
        SourcePlaylist {
            kind,
            id: "source-playlist".to_string(),
            storefront: None,
            apple_library,
            tracks: vec![SourceTrack {
                id: id.to_string(),
                canonical_id: None,
                isrc: None,
                name: "Song".to_string(),
                artists: vec!["Artist".to_string()],
                duration_ms: Some(180_000),
                album: None,
            }],
            skipped_count: 0,
        }
    }

    #[test]
    fn spotify_target_verifies_owner_reads_items_and_appends_json_uris() {
        let (address, server) = test_server(vec![
            (200, json!({"id":"me"}).to_string()),
            (200, json!({"owner":{"id":"me"}}).to_string()),
            (
                200,
                json!({"total":1,"items":[{"item":{"id":"existing"}}]}).to_string(),
            ),
            (201, json!({"snapshot_id":"opaque"}).to_string()),
        ]);
        let target = SourceRef {
            kind: SourceKind::Spotify,
            id: "target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            spotify_token: Some("spotify-secret".to_string()),
            ..TargetCredentials::default()
        };
        let playlist = load_target_with(&test_http(&address), &target, &credentials).unwrap();
        assert!(playlist.existing_ids.contains("existing"));
        append_target(
            &test_http(&address),
            &target,
            &credentials,
            &["new".to_string()],
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert!(requests[2].contains("GET /spotify/playlists/target/items?"));
        assert!(requests[3].contains("POST /spotify/playlists/target/items"));
        assert!(
            requests[3]
                .to_ascii_lowercase()
                .contains("authorization: bearer spotify-secret")
        );
        assert!(requests[3].contains("spotify:track:new"));
    }

    #[test]
    fn apple_target_requires_library_owner_context_and_sends_user_token() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"data":[{"type":"library-playlists","id":"p1","attributes":{"canEdit":true}}]}).to_string(),
            ),
            (
                200,
                json!({"data":[{"type":"songs","id":"a1","attributes":{"playParams":{"catalogId":"a1"}}}]}).to_string(),
            ),
            (204, String::new()),
        ]);
        let target = SourceRef {
            kind: SourceKind::AppleMusic,
            id: "p1".to_string(),
            storefront: Some("us".to_string()),
            apple_library: true,
        };
        let credentials = TargetCredentials {
            apple_developer_token: Some("apple-developer-secret".to_string()),
            apple_user_token: Some("apple-user-secret".to_string()),
            ..TargetCredentials::default()
        };
        let playlist = load_target_with(&test_http(&address), &target, &credentials).unwrap();
        assert!(playlist.existing_ids.contains("a1"));
        append_target(
            &test_http(&address),
            &target,
            &credentials,
            &["a2".to_string()],
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert!(requests[1].contains("GET /apple/me/library/playlists/p1/tracks"));
        assert!(requests[2].contains("POST /apple/me/library/playlists/p1/tracks"));
        assert!(
            requests[2]
                .to_ascii_lowercase()
                .contains("music-user-token: apple-user-secret")
        );
        assert!(requests[2].contains("\"type\":\"songs\""));
    }

    #[test]
    fn apple_target_rejects_missing_or_false_can_edit() {
        for attributes in [json!({}), json!({"canEdit": false})] {
            let (address, server) = test_server(vec![(
                200,
                json!({"data":[{"type":"library-playlists","id":"p1","attributes":attributes}]})
                    .to_string(),
            )]);
            let target = SourceRef {
                kind: SourceKind::AppleMusic,
                id: "p1".to_string(),
                storefront: Some("us".to_string()),
                apple_library: true,
            };
            let credentials = TargetCredentials {
                apple_developer_token: Some("apple-developer-secret".to_string()),
                apple_user_token: Some("apple-user-secret".to_string()),
                ..TargetCredentials::default()
            };
            let error = load_target_with(&test_http(&address), &target, &credentials).unwrap_err();
            assert!(matches!(
                error,
                AppError::TargetApiIncompatible(_) | AppError::TargetReadOnly(_)
            ));
            let _ = server.join().unwrap();
        }
    }

    #[test]
    fn youtube_target_verifies_channel_and_inserts_video() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"items":[{"id":"channel-one"},{"id":"channel-two"}]}).to_string(),
            ),
            (
                200,
                json!({"items":[{"snippet":{"channelId":"channel-two"}}]}).to_string(),
            ),
            (
                200,
                json!({"items":[{"snippet":{"resourceId":{"videoId":"old"}}}]}).to_string(),
            ),
            (200, json!({"id":"new"}).to_string()),
        ]);
        let target = SourceRef {
            kind: SourceKind::YoutubeMusic,
            id: "PL_target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            youtube_access_token: Some("youtube-secret".to_string()),
            ..TargetCredentials::default()
        };
        let playlist = load_target_with(&test_http(&address), &target, &credentials).unwrap();
        assert!(playlist.existing_ids.contains("old"));
        append_target(
            &test_http(&address),
            &target,
            &credentials,
            &["new".to_string()],
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /youtube/channels?"));
        assert!(requests[1].contains("GET /youtube/playlists?"));
        assert!(requests[2].contains("GET /youtube/playlistItems?"));
        assert!(requests[3].contains("POST /youtube/playlistItems?part=snippet"));
        assert!(requests[3].contains("youtube#video"));
        assert!(
            requests[3]
                .to_ascii_lowercase()
                .contains("authorization: bearer youtube-secret")
        );
    }

    #[test]
    fn cross_platform_review_searches_candidate_and_persists_only_explicit_confirmation() {
        let (address, server) = test_server(vec![
            (200, json!({"items":[{"id":"channel"}]}).to_string()),
            (
                200,
                json!({"items":[{"snippet":{"channelId":"channel"}}]}).to_string(),
            ),
            (200, json!({"items":[]}).to_string()),
            (
                200,
                json!({"items":[{"id":{"videoId":"v1"},"snippet":{"title":"Artist - Song","channelTitle":"Artist"}}]}).to_string(),
            ),
            (
                200,
                json!({"items":[{"id":"v1","snippet":{"title":"Artist - Song","channelTitle":"Artist"},"contentDetails":{"duration":"PT3M"}}]}).to_string(),
            ),
        ]);
        let source = source(SourceKind::Spotify, false, "s1");
        let target = SourceRef {
            kind: SourceKind::YoutubeMusic,
            id: "PL_target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            youtube_access_token: Some("youtube-secret".to_string()),
            ..TargetCredentials::default()
        };
        let paths = Paths {
            root: std::env::temp_dir().join(format!("freefm-target-review-{}", std::process::id())),
        };
        let value = review_source_to_target_with(
            &paths,
            &source,
            &target,
            &credentials,
            &test_http(&address),
            true,
            |_| Some(0),
            || true,
        )
        .unwrap();
        assert_eq!(value["approved_count"], 1);
        let (mappings, recovered) = load_external_mappings(&paths).unwrap();
        assert!(!recovered);
        assert_eq!(
            mappings.mappings[&external_mapping_key(&source, &target, "s1")].target_id,
            "v1"
        );
        let requests = server.join().unwrap();
        assert!(requests[3].contains("GET /youtube/search?"));
        assert!(requests[4].contains("GET /youtube/videos?"));
        assert!(
            requests[3]
                .to_ascii_lowercase()
                .contains("authorization: bearer youtube-secret")
        );
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[test]
    fn spotify_search_reads_track_shape_and_market_without_exposing_token() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"tracks":{"items":[{"type":"track","id":"sp2","name":"Song","duration_ms":180000,"artists":[{"name":"Artist"}],"album":{"name":"Album"},"external_ids":{"isrc":"US-AAA-00-00001"}}]}}).to_string(),
            ),
        ]);
        let target = SourceRef {
            kind: SourceKind::Spotify,
            id: "target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            spotify_token: Some("spotify-secret".to_string()),
            spotify_market: Some("US".to_string()),
            ..TargetCredentials::default()
        };
        let source_track = source(SourceKind::YoutubeMusic, false, "v1")
            .tracks
            .remove(0);
        let tracks =
            search_target_tracks(&test_http(&address), &target, &credentials, &source_track)
                .unwrap();
        assert_eq!(tracks[0].id, "sp2");
        assert_eq!(tracks[0].isrc.as_deref(), Some("US-AAA-00-00001"));
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /spotify/search?"));
        assert!(requests[0].contains("market=US"));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer spotify-secret")
        );
    }

    #[test]
    fn apple_search_reads_catalog_songs_with_developer_auth() {
        let (address, server) = test_server(vec![
            (
                200,
                json!({"results":{"songs":{"data":[{"id":"a2","type":"songs","attributes":{"name":"Song","artistName":"Artist","albumName":"Album","durationInMillis":180000,"isrc":"US-AAA-00-00001"}}]}}}).to_string(),
            ),
        ]);
        let target = SourceRef {
            kind: SourceKind::AppleMusic,
            id: "p1".to_string(),
            storefront: Some("cn".to_string()),
            apple_library: true,
        };
        let credentials = TargetCredentials {
            apple_developer_token: Some("apple-developer-secret".to_string()),
            apple_user_token: Some("apple-user-secret".to_string()),
            ..TargetCredentials::default()
        };
        let source_track = source(SourceKind::Spotify, false, "s1").tracks.remove(0);
        let tracks =
            search_target_tracks(&test_http(&address), &target, &credentials, &source_track)
                .unwrap();
        assert_eq!(tracks[0].id, "a2");
        assert_eq!(tracks[0].isrc.as_deref(), Some("US-AAA-00-00001"));
        let requests = server.join().unwrap();
        assert!(requests[0].contains("GET /apple/catalog/cn/search?"));
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer apple-developer-secret")
        );
    }

    #[test]
    fn cross_platform_sync_uses_only_reviewed_mapping_before_append() {
        let (address, server) = test_server(vec![
            (200, json!({"items":[{"id":"channel"}]}).to_string()),
            (
                200,
                json!({"items":[{"snippet":{"channelId":"channel"}}]}).to_string(),
            ),
            (200, json!({"items":[]}).to_string()),
            (200, json!({"id":"v1"}).to_string()),
            (200, json!({"items":[{"id":"channel"}]}).to_string()),
            (
                200,
                json!({"items":[{"snippet":{"channelId":"channel"}}]}).to_string(),
            ),
            (
                200,
                json!({"items":[{"snippet":{"resourceId":{"videoId":"v1"}}}]}).to_string(),
            ),
        ]);
        let source = source(SourceKind::Spotify, false, "s1");
        let target = SourceRef {
            kind: SourceKind::YoutubeMusic,
            id: "PL_target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            youtube_access_token: Some("youtube-secret".to_string()),
            ..TargetCredentials::default()
        };
        let paths = Paths {
            root: std::env::temp_dir().join(format!("freefm-target-sync-{}", std::process::id())),
        };
        let mut mappings = ExternalMappingStore::default();
        mappings.approve(
            &external_mapping_key(&source, &target, "s1"),
            "spotify",
            "source-playlist",
            None,
            "youtube_music",
            "PL_target",
            None,
            "v1",
        );
        save_external_mappings(&paths, &mappings).unwrap();
        let value = transfer_source_with(
            &paths,
            &source,
            &target,
            &credentials,
            &test_http(&address),
            None,
        )
        .unwrap();
        assert_eq!(value["transfer_kind"], "cross_platform_reviewed_mapping");
        assert_eq!(value["added_count"], 1);
        let requests = server.join().unwrap();
        assert!(requests[3].contains("POST /youtube/playlistItems?part=snippet"));
        assert!(requests[3].contains("videoId\":\"v1"));
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[test]
    fn mapping_context_is_bound_to_both_playlists_and_storefronts() {
        let source = source(SourceKind::Spotify, false, "s1");
        let target = SourceRef {
            kind: SourceKind::AppleMusic,
            id: "p1".to_string(),
            storefront: Some("us".to_string()),
            apple_library: true,
        };
        let mapping = crate::storage::ExternalMapping {
            source_kind: "spotify".to_string(),
            source_playlist_id: "source-playlist".to_string(),
            source_storefront: None,
            target_kind: "apple_music".to_string(),
            target_playlist_id: "p1".to_string(),
            target_storefront: Some("us".to_string()),
            target_id: "a1".to_string(),
            approved_at: 0,
        };
        assert!(mapping_matches(&mapping, &source, &target));

        let mut other_target = target.clone();
        other_target.id = "p2".to_string();
        assert!(!mapping_matches(&mapping, &source, &other_target));
        other_target.id = target.id.clone();
        other_target.storefront = Some("cn".to_string());
        assert!(!mapping_matches(&mapping, &source, &other_target));
    }

    #[test]
    fn append_returns_uncertain_when_reread_does_not_confirm_remote_write() {
        let (address, server) = test_server(vec![
            (200, json!({"id":"me"}).to_string()),
            (200, json!({"owner":{"id":"me"}}).to_string()),
            (200, json!({"total":0,"items":[]}).to_string()),
            (201, json!({"snapshot_id":"opaque"}).to_string()),
            (200, json!({"id":"me"}).to_string()),
            (200, json!({"owner":{"id":"me"}}).to_string()),
            (200, json!({"total":0,"items":[]}).to_string()),
        ]);
        let source = source(SourceKind::Spotify, false, "new");
        let target = SourceRef {
            kind: SourceKind::Spotify,
            id: "target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            spotify_token: Some("spotify-secret".to_string()),
            ..TargetCredentials::default()
        };
        let paths = Paths {
            root: std::env::temp_dir()
                .join(format!("freefm-target-uncertain-{}", std::process::id())),
        };
        let error = transfer_source_with(
            &paths,
            &source,
            &target,
            &credentials,
            &test_http(&address),
            None,
        )
        .unwrap_err();
        assert!(matches!(error, AppError::TargetWriteUncertain(_)));
        let _ = server.join().unwrap();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[test]
    fn target_diagnostics_only_reports_redacted_configuration() {
        let value =
            target_diagnostics("https://music.youtube.com/playlist?list=PL_target").unwrap();
        assert_eq!(value["target_kind"], "youtube_music");
        assert_eq!(value["target_playlist_id"], "PL_target");
        assert_eq!(value["remote_write"], true);
        assert!(value["credentials"]["youtube_access_token"].is_boolean());
        assert!(!value.to_string().contains("secret"));
    }

    #[test]
    fn cross_platform_target_fails_closed_before_reading_or_writing() {
        let source = source(SourceKind::Spotify, false, "s1");
        let paths = Paths {
            root: std::env::temp_dir()
                .join(format!("freefm-target-mapping-{}", std::process::id())),
        };
        let error = transfer_source(
            &paths,
            &source,
            "https://music.youtube.com/playlist?list=PL_target",
            None,
        )
        .unwrap_err();
        assert!(matches!(error, AppError::TargetMappingRequired(_)));
    }

    #[test]
    fn target_http_errors_do_not_echo_response_body_or_credentials() {
        let (address, server) = test_server(vec![(403, "target-secret-body".to_string())]);
        let target = SourceRef {
            kind: SourceKind::Spotify,
            id: "target".to_string(),
            storefront: None,
            apple_library: false,
        };
        let credentials = TargetCredentials {
            spotify_token: Some("spotify-secret".to_string()),
            ..TargetCredentials::default()
        };
        let error = load_target_with(&test_http(&address), &target, &credentials).unwrap_err();
        let message = error.to_string();
        assert!(matches!(error, AppError::TargetAuthRequired(_)));
        assert!(!message.contains("target-secret-body"));
        assert!(!message.contains("spotify-secret"));
        let _ = server.join().unwrap();
    }
}
