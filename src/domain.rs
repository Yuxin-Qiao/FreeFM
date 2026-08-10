use crate::error::{AppError, AppResult};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone)]
pub(crate) struct Song {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) artists: Vec<String>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) album: Option<String>,
    pub(crate) fee: Option<i64>,
    pub(crate) privilege: Option<Value>,
}

pub(crate) fn song_from_value(value: &Value) -> AppResult<Song> {
    let id = value_id(value.get("id"))
        .ok_or_else(|| AppError::ApiIncompatible("歌曲缺少 id".to_string()))?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::ApiIncompatible("歌曲缺少 name".to_string()))?
        .to_string();
    let artist_values = value
        .get("ar")
        .or_else(|| value.get("artists"))
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::ApiIncompatible("歌曲缺少歌手信息".to_string()))?;
    let artists = artist_values
        .iter()
        .filter_map(|artist| {
            artist
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    if artists.is_empty() {
        return Err(AppError::ApiIncompatible("歌曲歌手信息为空".to_string()));
    }
    let album = value
        .get("al")
        .or_else(|| value.get("album"))
        .and_then(|album| album.get("name"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(Song {
        id,
        name,
        artists,
        duration_ms: item_i64(value, "dt"),
        album,
        fee: strict_item_i64(value, "fee"),
        privilege: value.get("privilege").cloned(),
    })
}

pub(crate) fn merge_song_metadata(base: &Song, detail: Option<&Song>) -> Song {
    let Some(detail) = detail else {
        return base.clone();
    };
    let mut merged = detail.clone();
    if merged.privilege.is_none() {
        merged.privilege = base.privilege.clone();
    }
    if merged.fee.is_none() {
        merged.fee = base.fee;
    }
    if merged.duration_ms.is_none() {
        merged.duration_ms = base.duration_ms;
    }
    if merged.album.is_none() {
        merged.album = base.album.clone();
    }
    merged
}

pub(crate) fn playlist_summary(value: &Value) -> Option<PlaylistSummary> {
    Some(PlaylistSummary {
        id: value_id(value.get("id"))?,
        name: value.get("name")?.as_str()?.to_string(),
        track_count: item_i64(value, "trackCount").unwrap_or(0),
        owner_id: value_id(value.get("userId"))
            .or_else(|| value.pointer("/creator/userId").and_then(value_id_ref)),
    })
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PlaylistSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) track_count: i64,
    pub(crate) owner_id: Option<String>,
}

pub(crate) fn value_id(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| {
        value
            .as_i64()
            .map(|id| id.to_string())
            .or_else(|| value.as_str().map(ToOwned::to_owned))
    })
}

pub(crate) fn value_id_ref(value: &Value) -> Option<String> {
    value_id(Some(value))
}

pub(crate) fn item_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
    })
}

pub(crate) fn strict_item_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(crate) fn privilege_i64(song: &Song, key: &str) -> Option<i64> {
    song.privilege
        .as_ref()
        .and_then(|p| strict_item_i64(p, key))
}

fn has_active_free_trial(song: &Song) -> bool {
    song.privilege
        .as_ref()
        .and_then(|privilege| privilege.get("freeTrialPrivilege"))
        .and_then(Value::as_object)
        .is_some_and(|trial| {
            trial
                .get("resConsumable")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || trial
                    .get("userConsumable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Availability {
    Free,
    Restricted,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Probe {
    pub(crate) has_url: bool,
    pub(crate) fee: Option<i64>,
    pub(crate) free_trial: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AvailabilityEvidence {
    privilege_fee: Option<i64>,
    privilege_st: Option<i64>,
    privilege_pl: Option<i64>,
    active_free_trial: bool,
    probe_has_url: bool,
    probe_fee: Option<i64>,
    probe_free_trial: bool,
}

pub(crate) fn availability_evidence(song: &Song, probe: Probe) -> AvailabilityEvidence {
    AvailabilityEvidence {
        privilege_fee: privilege_i64(song, "fee").or(song.fee),
        privilege_st: privilege_i64(song, "st"),
        privilege_pl: privilege_i64(song, "pl"),
        active_free_trial: has_active_free_trial(song),
        probe_has_url: probe.has_url,
        probe_fee: probe.fee,
        probe_free_trial: probe.free_trial,
    }
}

pub(crate) fn availability_from_fields(song: &Song, probe: Option<Probe>) -> Availability {
    let privilege_fee = privilege_i64(song, "fee");
    let state = privilege_i64(song, "st");
    let play_bitrate = privilege_i64(song, "pl").or_else(|| privilege_i64(song, "playMaxbr"));
    if state.is_some_and(|state| state < 0) {
        return Availability::Unavailable;
    }
    if let Some(fee) = privilege_fee {
        if fee != 0 {
            return Availability::Restricted;
        }
    } else {
        return Availability::Unknown;
    }
    if has_active_free_trial(song) {
        return Availability::Restricted;
    }
    if privilege_i64(song, "pl").is_some_and(|pl| pl <= 0) {
        return Availability::Unavailable;
    }
    if state.is_none() || play_bitrate.is_none() {
        return Availability::Unknown;
    }
    if let Some(probe) = probe {
        if probe.free_trial {
            return Availability::Restricted;
        }
        match probe.fee {
            Some(fee) if fee != 0 => return Availability::Restricted,
            None => return Availability::Unknown,
            Some(0) => {}
            Some(_) => return Availability::Restricted,
        }
        return if probe.has_url {
            Availability::Free
        } else {
            Availability::Unavailable
        };
    }
    Availability::Unknown
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|ch| {
            !ch.is_whitespace()
                && !matches!(
                    ch,
                    '(' | ')'
                        | '['
                        | ']'
                        | '（'
                        | '）'
                        | '【'
                        | '】'
                        | '-'
                        | '_'
                        | '·'
                        | '—'
                        | '–'
                        | '.'
                        | '。'
                        | ','
                )
        })
        .flat_map(char::to_lowercase)
        .collect()
}

fn version_markers(song: &Song) -> HashSet<String> {
    let text = format!(
        "{} {}",
        song.name,
        song.album.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    let markers = [
        "live",
        "现场",
        "remix",
        "混音",
        "cover",
        "翻唱",
        "dj",
        "伴奏",
        "instrumental",
        "acoustic",
        "spedup",
        "sped-up",
        "slowed",
        "demo",
        "edit",
        "remaster",
        "混音版",
        "现场版",
    ];
    markers
        .iter()
        .filter(|marker| text.contains(**marker))
        .map(|marker| marker.to_string())
        .collect()
}

pub(crate) fn sorted_version_markers(song: &Song) -> Vec<String> {
    let mut markers = version_markers(song).into_iter().collect::<Vec<_>>();
    markers.sort();
    markers
}

pub(crate) fn same_recording_score(original: &Song, candidate: &Song) -> Option<f32> {
    if normalize(&original.name) != normalize(&candidate.name) {
        return None;
    }
    if original.artists.len() != candidate.artists.len()
        || original
            .artists
            .iter()
            .zip(&candidate.artists)
            .any(|(a, b)| normalize(a) != normalize(b))
    {
        return None;
    }
    let original_markers = version_markers(original);
    let candidate_markers = version_markers(candidate);
    if original_markers != candidate_markers {
        return None;
    }
    let (Some(left), Some(right)) = (original.duration_ms, candidate.duration_ms) else {
        return None;
    };
    if left <= 0 || right <= 0 || (left - right).abs() > 1500 {
        return None;
    }
    let score = if left == right { 1.0 } else { 0.96 };
    Some(score)
}
