//! FreeFM library: one-shot CLI logic for safely syncing free-playable
//! NetEase Private FM tracks. No daemon, scheduler, or model call lives here.

#[cfg(not(unix))]
compile_error!("FreeFM v0.1 supports macOS and Linux only");

pub const VERSION: &str = "0.1.0";

mod audit;
mod auth;
mod cli;
mod domain;
mod error;
mod plan;
mod protocol;
mod render;
mod runner;
mod storage;
mod sync;
mod trusted;

pub use cli::Cli;
pub use error::AppError;
pub use render::{emit, json_error};
pub use runner::run;
pub use storage::Paths;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::audit::audit;
    use crate::cli::usage;
    use crate::domain::{
        Availability, PlaylistSummary, Probe, Song, availability_from_fields, merge_song_metadata,
        privilege_i64, same_recording_score, song_from_value,
    };
    use crate::error::AppResult;
    use crate::plan::{Action, account_uid, build_plan, select_playlist};
    use crate::protocol::{
        PLAYLIST_NAME, RemoteApi, playlist_detail_extra_requests, playlist_track_ids,
    };
    use crate::render::{is_login_error, rendered_output};
    use crate::storage::{
        SessionFile, StateFile, StoredCookie, SyncLock, TrustedStore, load_session, load_state,
        load_trusted, now_seconds, restrict_dir, save_trusted, write_private_json,
    };
    use crate::sync::{AccountContext, require_login, require_ordinary_account, sync};
    use crate::trusted::review;
    use serde_json::{Value, json};
    use std::collections::{HashMap, HashSet};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn song(id: &str, name: &str, artist: &str, duration: i64, fee: i64) -> Song {
        Song {
            id: id.to_string(),
            name: name.to_string(),
            artists: vec![artist.to_string()],
            duration_ms: Some(duration),
            album: Some("Album".to_string()),
            fee: Some(fee),
            privilege: Some(json!({"fee": fee, "st": 0, "pl": if fee == 0 { 320000 } else { 0 }})),
        }
    }

    #[derive(Clone, Copy)]
    enum FakeFailure {
        Timeout,
        Server,
        Incompatible,
    }

    impl FakeFailure {
        fn error(self) -> AppError {
            match self {
                Self::Timeout => AppError::Timeout,
                Self::Server => AppError::Remote("fake HTTP 500".to_string()),
                Self::Incompatible => AppError::ApiIncompatible("fake non-JSON".to_string()),
            }
        }
    }

    struct FakeRemote {
        status_body: Option<Value>,
        status_failure: bool,
        fm: Vec<Song>,
        details: HashMap<String, Song>,
        probes: HashMap<String, Probe>,
        playlists: Vec<PlaylistSummary>,
        tracks: HashSet<String>,
        calls: u64,
        create_calls: u32,
        add_calls: u32,
        track_calls: u32,
        private_fm_failure: Option<FakeFailure>,
        create_after_side_effect_failure: bool,
        add_after_side_effect_failure: bool,
        track_failure_on_call: Option<u32>,
        save_session_failure: bool,
    }

    impl FakeRemote {
        fn free() -> Self {
            let source = song("1", "Free", "Artist", 180_000, 0);
            Self {
                status_body: Some(json!({"account": {"id": "u"}, "profile": {"vipType": 0}})),
                status_failure: false,
                fm: vec![source.clone()],
                details: HashMap::from([(source.id.clone(), source)]),
                probes: HashMap::from([(
                    "1".to_string(),
                    Probe {
                        has_url: true,
                        fee: Some(0),
                        free_trial: false,
                    },
                )]),
                playlists: Vec::new(),
                tracks: HashSet::new(),
                calls: 0,
                create_calls: 0,
                add_calls: 0,
                track_calls: 0,
                private_fm_failure: None,
                create_after_side_effect_failure: false,
                add_after_side_effect_failure: false,
                track_failure_on_call: None,
                save_session_failure: false,
            }
        }

        fn restricted_with_candidate() -> Self {
            let source = song("1", "Same", "Artist", 180_000, 1);
            let candidate = song("2", "Same", "Artist", 180_200, 0);
            Self {
                status_body: Some(json!({"account": {"id": "u"}, "profile": {"vipType": 0}})),
                status_failure: false,
                fm: vec![source.clone()],
                details: HashMap::from([
                    (source.id.clone(), source),
                    (candidate.id.clone(), candidate.clone()),
                ]),
                probes: HashMap::from([
                    (
                        "1".to_string(),
                        Probe {
                            has_url: false,
                            fee: Some(1),
                            free_trial: false,
                        },
                    ),
                    (
                        "2".to_string(),
                        Probe {
                            has_url: true,
                            fee: Some(0),
                            free_trial: false,
                        },
                    ),
                ]),
                playlists: Vec::new(),
                tracks: HashSet::new(),
                calls: 0,
                create_calls: 0,
                add_calls: 0,
                track_calls: 0,
                private_fm_failure: None,
                create_after_side_effect_failure: false,
                add_after_side_effect_failure: false,
                track_failure_on_call: None,
                save_session_failure: false,
            }
        }
    }

    impl RemoteApi for FakeRemote {
        fn status(&mut self) -> AppResult<Value> {
            self.calls += 1;
            if self.status_failure {
                Err(AppError::LoginRequired)
            } else {
                self.status_body
                    .clone()
                    .ok_or_else(|| AppError::ApiIncompatible("fake status missing".to_string()))
            }
        }

        fn private_fm(&mut self) -> AppResult<Vec<Song>> {
            self.calls += 1;
            if let Some(failure) = self.private_fm_failure {
                return Err(failure.error());
            }
            Ok(self.fm.clone())
        }

        fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>> {
            self.calls += 1;
            Ok(ids
                .iter()
                .filter_map(|id| self.details.get(id).cloned().map(|song| (id.clone(), song)))
                .collect())
        }

        fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>> {
            self.calls += 1;
            Ok(self
                .details
                .values()
                .filter(|song| song.name == keywords && song.id != "1")
                .cloned()
                .collect())
        }

        fn playback_probe(&mut self, id: &str) -> AppResult<Probe> {
            self.calls += 1;
            Ok(self.probes.get(id).copied().unwrap_or(Probe {
                has_url: false,
                fee: None,
                free_trial: false,
            }))
        }

        fn user_playlists(&mut self, _uid: &str) -> AppResult<Vec<PlaylistSummary>> {
            self.calls += 1;
            Ok(self.playlists.clone())
        }

        fn playlist_summary_by_id(
            &mut self,
            playlist_id: &str,
            uid: &str,
        ) -> AppResult<Option<PlaylistSummary>> {
            self.calls += 1;
            Ok(self
                .playlists
                .iter()
                .find(|playlist| {
                    playlist.id == playlist_id
                        && playlist.name == PLAYLIST_NAME
                        && playlist.owner_id.as_deref() == Some(uid)
                })
                .cloned())
        }

        fn playlist_tracks(&mut self, _id: &str) -> AppResult<HashSet<String>> {
            self.calls += 1;
            self.track_calls += 1;
            if self.track_failure_on_call == Some(self.track_calls) {
                return Err(AppError::Remote("fake reread failure".to_string()));
            }
            Ok(self.tracks.clone())
        }

        fn create_playlist(&mut self) -> AppResult<String> {
            self.calls += 1;
            self.create_calls += 1;
            let id = "p1".to_string();
            self.playlists.push(PlaylistSummary {
                id: id.clone(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("u".to_string()),
            });
            if self.create_after_side_effect_failure {
                return Err(AppError::Remote("fake post-create failure".to_string()));
            }
            Ok(id)
        }

        fn add_tracks(&mut self, _playlist_id: &str, ids: &[String]) -> AppResult<()> {
            self.calls += 1;
            self.add_calls += 1;
            self.tracks.extend(ids.iter().cloned());
            if self.add_after_side_effect_failure {
                return Err(AppError::Remote("fake post-add failure".to_string()));
            }
            Ok(())
        }

        fn client_calls(&self) -> u64 {
            self.calls
        }

        fn http_requests(&self) -> u64 {
            self.calls
        }

        fn save_session(&self, _paths: &Paths) -> AppResult<()> {
            if self.save_session_failure {
                Err(AppError::Remote("fake session-save failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn free_song_is_distinguished_from_vip_unavailable_and_unknown() {
        let free = song("1", "Song", "Artist", 180_000, 0);
        let vip = song("2", "Song", "Artist", 180_000, 1);
        let unavailable = Song {
            privilege: Some(json!({"fee": 0, "st": -200, "pl": 0})),
            ..free.clone()
        };
        let unknown = Song {
            privilege: None,
            fee: None,
            ..free.clone()
        };
        assert_eq!(availability_from_fields(&free, None), Availability::Unknown);
        assert_eq!(
            availability_from_fields(&vip, None),
            Availability::Restricted
        );
        assert_eq!(
            availability_from_fields(&unavailable, None),
            Availability::Unavailable
        );
        assert_eq!(
            availability_from_fields(&unknown, None),
            Availability::Unknown
        );
    }

    #[test]
    fn detail_metadata_does_not_discard_fm_privilege() {
        let base = song("1", "Song", "Artist", 180_000, 0);
        let detail = Song {
            privilege: None,
            fee: Some(0),
            ..base.clone()
        };
        let merged = merge_song_metadata(&base, Some(&detail));
        assert_eq!(privilege_i64(&merged, "fee"), Some(0));
        assert_eq!(privilege_i64(&merged, "pl"), Some(320000));
        assert_eq!(
            availability_from_fields(
                &merged,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false,
                })
            ),
            Availability::Free
        );
    }

    #[test]
    fn playback_probe_never_turns_free_trial_into_full_free() {
        let candidate = song("1", "Song", "Artist", 180_000, 0);
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: true
                })
            ),
            Availability::Restricted
        );
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false
                })
            ),
            Availability::Free
        );
    }

    #[test]
    fn same_recording_rejects_wrong_artist_and_version_mismatch() {
        let original = song("1", "Song", "Artist", 180_000, 1);
        let correct = song("2", "Song", "Artist", 180_300, 0);
        let wrong_artist = song("3", "Song", "Other", 180_000, 0);
        let live = Song {
            name: "Song (Live)".to_string(),
            ..correct.clone()
        };
        assert!(same_recording_score(&original, &correct).is_some());
        assert!(same_recording_score(&original, &wrong_artist).is_none());
        assert!(same_recording_score(&original, &live).is_none());
    }

    #[test]
    fn same_recording_rejects_remix_cover_and_instrumental() {
        let original = song("1", "Song", "Artist", 180_000, 1);
        for title in [
            "Song Remix",
            "Song 翻唱",
            "Song (Acoustic)",
            "Song 伴奏",
            "Song - Live",
        ] {
            let candidate = Song {
                name: title.to_string(),
                ..song("2", title, "Artist", 180_000, 0)
            };
            assert!(
                same_recording_score(&original, &candidate).is_none(),
                "{title}"
            );
        }
    }

    #[test]
    fn fixture_variant_negative_titles_are_rejected() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/fm-responses.json")).unwrap();
        let original = song("1", "Same Song", "Same Artist", 190_000, 1);
        for title in fixture["edge_cases"]["variant_negative_titles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
        {
            let candidate = song("2", title, "Same Artist", 190_000, 0);
            assert!(
                same_recording_score(&original, &candidate).is_none(),
                "{title}"
            );
        }
    }

    #[test]
    fn session_parser_requires_music_u_without_printing_values() {
        let valid = serde_json::from_str::<SessionFile>(
            r#"{"cookies":[{"name":"MUSIC_U","value":"secret"}]}"#,
        )
        .unwrap();
        assert!(valid.cookies.iter().any(|cookie| cookie.name == "MUSIC_U"));
        let invalid =
            serde_json::from_str::<SessionFile>(r#"{"cookies":[{"name":"foo","value":"bar"}]}"#)
                .unwrap();
        assert!(
            invalid
                .cookies
                .iter()
                .all(|cookie| cookie.name != "MUSIC_U")
        );
    }

    #[test]
    fn corrupted_session_is_rejected_without_recovery() {
        let root = std::env::temp_dir().join(format!(
            "freefm-session-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("session.json"), b"not-json").unwrap();
        let result = load_session(&Paths { root: root.clone() });
        assert!(matches!(result, Err(AppError::StateCorrupt(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupted_state_recovers_to_empty_state() {
        let dir = std::env::temp_dir().join(format!("freefm-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("state.json"), b"not-json").unwrap();
        let (state, recovered) = load_state(&Paths { root: dir.clone() }).unwrap();
        assert!(recovered);
        assert!(state.playlist_id.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_ids_are_set_based_for_idempotent_append() {
        let existing = HashSet::from(["1".to_string()]);
        assert!(existing.contains("1"));
        let mut planned = HashSet::new();
        assert!(planned.insert("2".to_string()));
        assert!(!planned.insert("2".to_string()));
    }

    #[test]
    fn remote_named_playlist_is_authoritative_over_stale_state() {
        let playlists = vec![
            PlaylistSummary {
                id: "old".to_string(),
                name: "Renamed FreeFM".to_string(),
                track_count: 1,
                owner_id: Some("7".to_string()),
            },
            PlaylistSummary {
                id: "other".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 2,
                owner_id: Some("7".to_string()),
            },
        ];
        assert_eq!(
            select_playlist(&playlists, "7", Some("missing"))
                .unwrap()
                .unwrap()
                .id,
            "other"
        );
        assert!(
            select_playlist(&playlists[..1], "7", Some("old"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn multiple_owned_same_name_playlists_fail_closed() {
        let playlists = vec![
            PlaylistSummary {
                id: "one".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("7".to_string()),
            },
            PlaylistSummary {
                id: "two".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("7".to_string()),
            },
        ];
        assert!(matches!(
            select_playlist(&playlists, "7", None),
            Err(AppError::AmbiguousPlaylist)
        ));
    }

    #[test]
    fn playlist_track_parser_handles_more_than_500_ids() {
        let track_ids = (0..501).map(|id| json!({"id": id})).collect::<Vec<_>>();
        let body = json!({"playlist": {"trackIds": track_ids}});
        assert_eq!(playlist_track_ids(&body).len(), 501);
        assert_eq!(playlist_detail_extra_requests(&body), 2);
    }

    #[test]
    fn unknown_candidate_entitlement_is_resolved_by_capability_probe() {
        let candidate = Song {
            id: "candidate".to_string(),
            name: "Song".to_string(),
            artists: vec!["Artist".to_string()],
            duration_ms: Some(180_000),
            album: Some("Album".to_string()),
            fee: Some(0),
            privilege: Some(json!({"fee": 0, "st": 0, "pl": 320000})),
        };
        assert_eq!(
            availability_from_fields(&candidate, None),
            Availability::Unknown
        );
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false,
                })
            ),
            Availability::Free
        );
    }

    #[test]
    fn timeout_and_login_errors_are_safe_categories() {
        assert_eq!(json_error(&AppError::Timeout)["error"]["kind"], "timeout");
        assert_eq!(
            json_error(&AppError::LoginRequired)["error"]["kind"],
            "login_required"
        );
        assert!(is_login_error(&AppError::Remote(
            "login_status: 登录类响应代码 301".to_string()
        )));
        assert!(!is_login_error(&AppError::ApiIncompatible(
            "状态字段变化".to_string()
        )));
        let secret = json_error(&AppError::Remote("request failed".to_string())).to_string();
        assert!(!secret.contains("MUSIC_U"));
    }

    #[test]
    fn fixture_shape_covers_free_vip_unavailable_and_wrong_match() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/fm-responses.json")).unwrap();
        let songs = fixture
            .pointer("/private_fm/data")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(songs.len(), 4);
        assert_eq!(
            fixture
                .pointer("/search_wrong_artist/result/songs")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            fixture
                .pointer("/edge_cases/variant_negative_titles")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            10
        );
    }

    #[test]
    fn fixture_correct_free_version_is_high_confidence_and_playable() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/fm-responses.json")).unwrap();
        let original = song_from_value(&fixture["private_fm"]["data"][3]).unwrap();
        let candidate =
            song_from_value(&fixture["search_correct_free_version"]["result"]["songs"][0]).unwrap();
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false
                })
            ),
            Availability::Free
        );
        assert_eq!(same_recording_score(&original, &candidate), Some(0.96));
    }

    #[test]
    fn cli_help_and_version_are_success_paths() {
        assert!(matches!(
            Cli::parse(["freefm".to_string()]),
            Err(AppError::Help(_))
        ));
        assert!(matches!(
            Cli::parse(["freefm".to_string(), "--version".to_string()]),
            Err(AppError::Version)
        ));
        assert_eq!(AppError::Version.to_string(), "FreeFM 0.1.0");
        assert!(usage().starts_with("FreeFM\n\n用法：freefm "));
        assert!(usage().contains("tui      打开轻量交互界面"));
        assert!(matches!(
            Cli::parse([
                "freefm".to_string(),
                "tui".to_string(),
                "--quiet".to_string()
            ]),
            Err(AppError::Usage(_))
        ));
    }

    #[test]
    fn quiet_wins_on_success_but_json_errors_remain_visible() {
        let cli = Cli {
            command: "sync".to_string(),
            json: true,
            quiet: true,
            data_dir: None,
        };
        let value = json!({"ok": true});
        assert!(rendered_output(&cli, &value, "ok", false).is_none());
        let error = json!({"ok": false, "error": {"kind": "timeout"}});
        let output = rendered_output(&cli, &error, "timeout", true).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap(), error);
    }

    #[test]
    fn preview_plans_free_song_without_remote_write() {
        let mut remote = FakeRemote::free();
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &TrustedStore::default(),
            "preview",
            false,
        )
        .unwrap();
        assert_eq!(report.would_add_ids, vec!["1"]);
        assert!(report.would_create_playlist);
        assert_eq!(remote.create_calls, 0);
        assert_eq!(remote.add_calls, 0);
    }

    #[test]
    fn sync_is_append_only_and_second_run_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "freefm-flow-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        let paths = Paths { root: root.clone() };
        let mut remote = FakeRemote::free();
        let (state, recovered) = load_state(&paths).unwrap();
        let report = build_plan(
            &mut remote,
            "u",
            &state,
            &TrustedStore::default(),
            "sync",
            recovered,
        )
        .unwrap();
        sync(&paths, report, "u", &mut remote).unwrap();
        let (state, recovered) = load_state(&paths).unwrap();
        assert!(!recovered);
        let report = build_plan(
            &mut remote,
            "u",
            &state,
            &TrustedStore::default(),
            "sync",
            recovered,
        )
        .unwrap();
        assert!(report.would_add_ids.is_empty());
        sync(&paths, report, "u", &mut remote).unwrap();
        assert_eq!(remote.create_calls, 1);
        assert_eq!(remote.add_calls, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plan_transport_failures_never_write() {
        for failure in [
            FakeFailure::Timeout,
            FakeFailure::Server,
            FakeFailure::Incompatible,
        ] {
            let mut remote = FakeRemote::free();
            remote.private_fm_failure = Some(failure);
            let result = build_plan(
                &mut remote,
                "u",
                &StateFile::default(),
                &TrustedStore::default(),
                "sync",
                false,
            );
            assert!(result.is_err());
            assert_eq!(remote.create_calls, 0);
            assert_eq!(remote.add_calls, 0);
        }
    }

    #[test]
    fn post_create_failure_recovers_without_duplicate_playlist() {
        let root = std::env::temp_dir().join(format!(
            "freefm-create-recovery-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = Paths { root: root.clone() };
        let mut remote = FakeRemote::free();
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &TrustedStore::default(),
            "sync",
            false,
        )
        .unwrap();
        remote.create_after_side_effect_failure = true;
        assert!(sync(&paths, report, "u", &mut remote).is_err());
        remote.create_after_side_effect_failure = false;
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &TrustedStore::default(),
            "sync",
            false,
        )
        .unwrap();
        sync(&paths, report, "u", &mut remote).unwrap();
        assert_eq!(remote.create_calls, 1);
        assert_eq!(remote.add_calls, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn post_add_and_reread_failures_recover_without_duplicate_track() {
        for fail_reread in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "freefm-add-recovery-{}-{}-{fail_reread}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let paths = Paths { root: root.clone() };
            let mut remote = FakeRemote::free();
            let report = build_plan(
                &mut remote,
                "u",
                &StateFile::default(),
                &TrustedStore::default(),
                "sync",
                false,
            )
            .unwrap();
            if fail_reread {
                remote.track_failure_on_call = Some(2);
            } else {
                remote.add_after_side_effect_failure = true;
            }
            assert!(sync(&paths, report, "u", &mut remote).is_err());
            remote.track_failure_on_call = None;
            remote.add_after_side_effect_failure = false;
            let report = build_plan(
                &mut remote,
                "u",
                &StateFile::default(),
                &TrustedStore::default(),
                "sync",
                false,
            )
            .unwrap();
            assert!(report.would_add_ids.is_empty());
            sync(&paths, report, "u", &mut remote).unwrap();
            assert_eq!(remote.create_calls, 1);
            assert_eq!(remote.add_calls, 1);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn state_write_failure_recovers_from_remote_truth() {
        let parent = std::env::temp_dir().join(format!(
            "freefm-state-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&parent).unwrap();
        let invalid_root = parent.join("not-a-directory");
        fs::write(&invalid_root, b"x").unwrap();
        let invalid_paths = Paths { root: invalid_root };
        let mut remote = FakeRemote::free();
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &TrustedStore::default(),
            "sync",
            false,
        )
        .unwrap();
        assert!(sync(&invalid_paths, report, "u", &mut remote).is_err());

        let valid_paths = Paths {
            root: parent.join("valid"),
        };
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &TrustedStore::default(),
            "sync",
            false,
        )
        .unwrap();
        assert!(report.would_add_ids.is_empty());
        sync(&valid_paths, report, "u", &mut remote).unwrap();
        assert_eq!(remote.create_calls, 1);
        assert_eq!(remote.add_calls, 1);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn restricted_song_candidate_is_preview_only() {
        let mut remote = FakeRemote::restricted_with_candidate();
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &TrustedStore::default(),
            "preview",
            false,
        )
        .unwrap();
        assert!(report.would_add_ids.is_empty());
        assert!(matches!(report.decisions[0].action, Action::CandidateOnly));
        assert_eq!(report.decisions[0].selected_id.as_deref(), Some("2"));
    }

    #[test]
    fn non_ordinary_account_is_rejected_before_planning() {
        let account = AccountContext {
            uid: "u".to_string(),
            vip_type: 1,
        };
        assert!(matches!(
            require_ordinary_account(&account),
            Err(AppError::OrdinaryAccountRequired)
        ));
        assert_eq!(
            json_error(&AppError::OrdinaryAccountRequired)["error"]["kind"],
            "ordinary_account_required"
        );
    }

    #[test]
    fn login_expiry_stops_before_private_fm() {
        let mut remote = FakeRemote::free();
        remote.status_failure = true;
        assert!(matches!(
            require_login(&mut remote),
            Err(AppError::LoginRequired)
        ));
        assert_eq!(remote.calls, 1);
    }

    #[test]
    fn missing_or_string_entitlement_never_becomes_free() {
        let mut candidate = song("1", "Song", "Artist", 180_000, 0);
        candidate.privilege = Some(json!({"fee": "0", "st": 0, "pl": 320000}));
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false,
                })
            ),
            Availability::Unknown
        );
        candidate.privilege = Some(json!({"fee": 0, "st": 0, "pl": 320000}));
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: None,
                    free_trial: false,
                })
            ),
            Availability::Unknown
        );
    }

    #[test]
    fn lock_is_exclusive_and_removed_on_drop() {
        let root = std::env::temp_dir().join(format!(
            "freefm-lock-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        let paths = Paths { root: root.clone() };
        let lock = SyncLock::acquire(&paths).unwrap();
        assert!(matches!(
            SyncLock::acquire(&paths),
            Err(AppError::ConcurrentSync)
        ));
        drop(lock);
        assert!(paths.lock().exists());
        let second = SyncLock::acquire(&paths).unwrap();
        drop(second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn lock_process_helper() {
        let Some(root) = env::var_os("FREEFM_LOCK_HELPER_ROOT") else {
            return;
        };
        let paths = Paths {
            root: PathBuf::from(root),
        };
        let _lock = SyncLock::acquire(&paths).unwrap();
        fs::write(paths.root.join("ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn flock_blocks_another_process_and_recovers_after_kill() {
        let root = std::env::temp_dir().join(format!(
            "freefm-process-lock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = Paths { root: root.clone() };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "tests::lock_process_helper"])
            .env("FREEFM_LOCK_HELPER_ROOT", &root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let ready = root.join("ready");
        for _ in 0..100 {
            if ready.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(ready.exists(), "lock helper did not become ready");
        assert!(matches!(
            SyncLock::acquire(&paths),
            Err(AppError::ConcurrentSync)
        ));
        child.kill().unwrap();
        child.wait().unwrap();
        let recovered = SyncLock::acquire(&paths).unwrap();
        drop(recovered);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn session_expiry_301_401_403_and_null_account_fail_closed() {
        assert!(matches!(
            account_uid(&json!({"code": 301, "msg": "未登录"})),
            Err(AppError::LoginRequired)
        ));
        assert!(matches!(
            account_uid(&json!({"code": 401, "msg": "Token expired"})),
            Err(AppError::LoginRequired)
        ));
        assert!(matches!(
            account_uid(&json!({"code": 403, "msg": "Forbidden"})),
            Err(AppError::LoginRequired)
        ));
        assert!(matches!(
            account_uid(&json!({"code": 200, "data": {"code": 301}})),
            Err(AppError::LoginRequired)
        ));
        assert!(matches!(
            account_uid(&json!({"code": 200, "account": null, "profile": null})),
            Err(AppError::LoginRequired)
        ));
    }

    #[test]
    fn reauth_preserves_state_json_and_playlist_binding() {
        let root = std::env::temp_dir().join(format!(
            "freefm-reauth-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = Paths { root: root.clone() };
        restrict_dir(&paths.root).unwrap();

        // 1. Initial sync state saved
        let initial_state = StateFile {
            playlist_id: Some("target-playlist-123".to_string()),
            last_sync_at: Some(1700000000),
            added_song_ids: Vec::new(),
        };
        write_private_json(&paths.state(), &initial_state).unwrap();

        // 2. Initial session saved
        let session1 = SessionFile {
            cookies: vec![StoredCookie {
                name: "MUSIC_U".to_string(),
                value: "old_token_123".to_string(),
            }],
        };
        write_private_json(&paths.session(), &session1).unwrap();

        // 3. Simulate re-authentication saving a new session token
        let session2 = SessionFile {
            cookies: vec![StoredCookie {
                name: "MUSIC_U".to_string(),
                value: "new_token_456".to_string(),
            }],
        };
        write_private_json(&paths.session(), &session2).unwrap();

        // 4. Verify state.json was NOT overwritten or wiped
        let (loaded_state, corrupt) = load_state(&paths).unwrap();
        assert!(!corrupt);
        assert_eq!(
            loaded_state.playlist_id.as_deref(),
            Some("target-playlist-123")
        );
        assert_eq!(loaded_state.last_sync_at, Some(1700000000));

        let loaded_session = load_session(&paths).unwrap().unwrap();
        assert_eq!(loaded_session.cookies[0].value, "new_token_456");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn audit_is_read_only_and_classifies_every_status() {
        let mut remote = FakeRemote::free();
        remote.playlists = vec![PlaylistSummary {
            id: "p1".to_string(),
            name: PLAYLIST_NAME.to_string(),
            track_count: 4,
            owner_id: Some("u".to_string()),
        }];
        remote.tracks = HashSet::from([
            "1".to_string(),
            "2".to_string(),
            "3".to_string(),
            "4".to_string(),
        ]);
        let vip = song("2", "Vip", "Artist", 180_000, 1);
        let unavailable = Song {
            privilege: Some(json!({"fee": 0, "st": -200, "pl": 0})),
            ..song("3", "Unavailable", "Artist", 180_000, 0)
        };
        let unknown = Song {
            privilege: None,
            fee: None,
            ..song("4", "Unknown", "Artist", 180_000, 0)
        };
        remote.details.extend(HashMap::from([
            (vip.id.clone(), vip),
            (unavailable.id.clone(), unavailable),
            (unknown.id.clone(), unknown),
        ]));
        let state = StateFile {
            playlist_id: Some("p1".to_string()),
            last_sync_at: None,
            added_song_ids: Vec::new(),
        };
        let result = audit(&mut remote, "u", &state, false).unwrap();
        assert_eq!(result["needs_attention"], true);
        assert_eq!(result["summary"]["still_free"], 1);
        assert_eq!(result["summary"]["became_restricted"], 1);
        assert_eq!(result["summary"]["unavailable"], 1);
        assert_eq!(result["summary"]["unknown"], 1);
        assert_eq!(remote.create_calls, 0);
        assert_eq!(remote.add_calls, 0);
    }

    #[test]
    fn audit_without_playlist_is_healthy_and_read_only() {
        let mut remote = FakeRemote::free();
        remote.playlists = Vec::new();
        let result = audit(&mut remote, "u", &StateFile::default(), false).unwrap();
        assert_eq!(result["playlist_exists"], false);
        assert_eq!(result["needs_attention"], false);
        assert_eq!(result["checked_count"], 0);
        assert_eq!(remote.create_calls, 0);
        assert_eq!(remote.add_calls, 0);
    }

    #[test]
    fn audit_fails_closed_on_ambiguous_playlists() {
        let mut remote = FakeRemote::free();
        remote.playlists = vec![
            PlaylistSummary {
                id: "p1".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("u".to_string()),
            },
            PlaylistSummary {
                id: "p2".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("u".to_string()),
            },
        ];
        let error = audit(&mut remote, "u", &StateFile::default(), false).unwrap_err();
        assert!(matches!(error, AppError::AmbiguousPlaylist));
    }

    #[test]
    fn trusted_mapping_is_used_only_while_target_still_free() {
        let mut remote = FakeRemote::restricted_with_candidate();
        let mut trusted = TrustedStore::default();
        trusted.approve("1", "2");
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &trusted,
            "preview",
            false,
        )
        .unwrap();
        let decision = &report.decisions[0];
        assert!(matches!(decision.action, Action::TrustedMapping));
        assert!(decision.trusted_mapping);
        assert!(!decision.trusted_invalid);
        assert_eq!(decision.selected_id.as_deref(), Some("2"));
        assert!(report.would_add_ids.contains(&"2".to_string()));
        let serialized = serde_json::to_value(&decision.action).unwrap();
        assert_eq!(serialized, json!("trusted_mapping"));
    }

    #[test]
    fn stale_trusted_mapping_falls_back_and_reports_invalid() {
        let mut remote = FakeRemote::restricted_with_candidate();
        let mut trusted = TrustedStore::default();
        // Approved target "9" does not exist remotely.
        trusted.approve("1", "9");
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &trusted,
            "preview",
            false,
        )
        .unwrap();
        let decision = &report.decisions[0];
        assert!(decision.trusted_invalid);
        assert!(matches!(decision.action, Action::CandidateOnly));
        assert!(!report.would_add_ids.contains(&"2".to_string()));
    }

    #[test]
    fn trusted_target_becoming_restricted_stops_usage() {
        let mut remote = FakeRemote::restricted_with_candidate();
        let mut trusted = TrustedStore::default();
        trusted.approve("1", "2");
        remote.probes.get_mut("2").unwrap().fee = Some(1);
        let report = build_plan(
            &mut remote,
            "u",
            &StateFile::default(),
            &trusted,
            "preview",
            false,
        )
        .unwrap();
        let decision = &report.decisions[0];
        assert!(decision.trusted_invalid);
        assert!(matches!(decision.action, Action::Skip));
        assert!(report.would_add_ids.is_empty());
    }

    #[test]
    fn trusted_store_roundtrip_and_corrupt_recovery() {
        let root = env::temp_dir().join(format!("freefm-trusted-{}", now_seconds()));
        let paths = Paths { root: root.clone() };
        let mut store = TrustedStore::default();
        store.approve("1", "2");
        save_trusted(&paths, &store).unwrap();
        let (loaded, corrupt) = load_trusted(&paths).unwrap();
        assert!(!corrupt);
        assert_eq!(loaded.mappings.get("1").unwrap().target_id, "2");
        fs::write(paths.trusted(), "{not json").unwrap();
        let (loaded, corrupt) = load_trusted(&paths).unwrap();
        assert!(corrupt);
        assert!(loaded.mappings.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_persists_only_explicit_confirmation() {
        let root = env::temp_dir().join(format!("freefm-review-{}", now_seconds()));
        let paths = Paths { root: root.clone() };
        let mut remote = FakeRemote::restricted_with_candidate();
        let result = review(
            &paths,
            &mut remote,
            "u",
            &StateFile::default(),
            false,
            true,
            || true,
            |_| Vec::new(),
        )
        .unwrap();
        assert_eq!(result["approved_count"], 1);
        let (loaded, _) = load_trusted(&paths).unwrap();
        assert_eq!(loaded.mappings.get("1").unwrap().target_id, "2");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_without_confirmation_writes_nothing() {
        let root = env::temp_dir().join(format!("freefm-review-skip-{}", now_seconds()));
        let paths = Paths { root: root.clone() };
        let mut remote = FakeRemote::restricted_with_candidate();
        let result = review(
            &paths,
            &mut remote,
            "u",
            &StateFile::default(),
            false,
            true,
            || false,
            |_| Vec::new(),
        )
        .unwrap();
        assert_eq!(result["approved_count"], 0);
        assert_eq!(result["skipped_count"], 1);
        assert!(!paths.trusted().exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_remove_deletes_only_requested_mapping() {
        let root = env::temp_dir().join(format!("freefm-review-rm-{}", now_seconds()));
        let paths = Paths { root: root.clone() };
        let mut store = TrustedStore::default();
        store.approve("1", "2");
        store.approve("5", "6");
        save_trusted(&paths, &store).unwrap();
        let mut remote = FakeRemote::restricted_with_candidate();
        let result = review(
            &paths,
            &mut remote,
            "u",
            &StateFile::default(),
            false,
            true,
            || false,
            |_| vec!["1".to_string()],
        )
        .unwrap();
        assert_eq!(result["removed_count"], 1);
        let (loaded, _) = load_trusted(&paths).unwrap();
        assert!(!loaded.mappings.contains_key("1"));
        assert_eq!(loaded.mappings.get("5").unwrap().target_id, "6");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_replaces_stale_mapping_with_new_confirmation() {
        let root = env::temp_dir().join(format!("freefm-review-replace-{}", now_seconds()));
        let paths = Paths { root: root.clone() };
        let mut store = TrustedStore::default();
        store.approve("1", "9");
        save_trusted(&paths, &store).unwrap();
        let mut remote = FakeRemote::restricted_with_candidate();
        let result = review(
            &paths,
            &mut remote,
            "u",
            &StateFile::default(),
            false,
            true,
            || true,
            |_| Vec::new(),
        )
        .unwrap();
        assert_eq!(result["approved_count"], 1);
        let (loaded, _) = load_trusted(&paths).unwrap();
        assert_eq!(loaded.mappings.len(), 1);
        assert_eq!(loaded.mappings.get("1").unwrap().target_id, "2");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_records_added_song_ids_and_old_state_stays_compatible() {
        let root = env::temp_dir().join(format!("freefm-added-{}", now_seconds()));
        let paths = Paths { root: root.clone() };
        restrict_dir(&paths.root).unwrap();
        // Pre-existing state written by an older FreeFM has no added_song_ids.
        fs::write(
            paths.state(),
            r#"{"playlist_id":"p1","last_sync_at":1700000000}"#,
        )
        .unwrap();
        let (state, corrupt) = load_state(&paths).unwrap();
        assert!(!corrupt);
        assert!(state.added_song_ids.is_empty());

        let mut remote = FakeRemote::free();
        remote.playlists = vec![PlaylistSummary {
            id: "p1".to_string(),
            name: PLAYLIST_NAME.to_string(),
            track_count: 0,
            owner_id: Some("u".to_string()),
        }];
        let report = build_plan(
            &mut remote,
            "u",
            &state,
            &TrustedStore::default(),
            "sync",
            false,
        )
        .unwrap();
        let synced = sync(&paths, report, "u", &mut remote).unwrap();
        assert_eq!(synced.would_add_ids, vec!["1".to_string()]);
        let (state2, corrupt2) = load_state(&paths).unwrap();
        assert!(!corrupt2);
        assert_eq!(state2.added_song_ids, vec!["1".to_string()]);
        let _ = fs::remove_dir_all(root);
    }
}
