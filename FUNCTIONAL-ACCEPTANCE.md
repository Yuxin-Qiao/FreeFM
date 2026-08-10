# FreeFM v0.1 product functional acceptance

Date: 2026-08-10 (Asia/Shanghai). This is the single acceptance record. It is
not a code review and not a test-count report: every row is a black-box check
of the product truth below.

## Product truth

An ordinary non-VIP NetEase user scans the official-client QR code, FreeFM
keeps sampling that account's Private FM, and strictly free-playable tracks
accumulate in the user's own `FreeFM · Auto` playlist. Restricted tracks are
never added; a plausible free twin may be shown as a candidate but can only be
used after an explicit human confirmation (trusted mapping), re-verified free
on every later use. Saved tracks that later become restricted/unavailable/
unknown must be detectable and reportable. Routine operation is a one-shot
native Rust CLI with no LLM, daemon, Node, Python, or Docker.

## Status legend

`PASS` / `FAIL` / `BLOCKED_USER_ACTION` / `BLOCKED_EXTERNAL` /
`NOT_APPLICABLE`. "Has code" is not PASS. A FAIL must be fixed and re-run.

## Acceptance matrix

| ID | Scenario | Type | Result | Evidence |
|---|---|---|---|---|
| A-01 | Fresh install: installer checksum fail-closed (missing tool / missing checksum / mismatch / correct) and clean `--version` | fixture/static | PASS | `scripts/release-smoke-test.sh` 4/4 cases |
| A-02 | QR auth -> session persisted -> process restart -> `status` authenticated with `vipType=0` | live | BLOCKED_USER_ACTION | steps in section L1 |
| A-03 | `preview` reads real Private FM and classifies free/restricted/unavailable/unknown/candidate | live | BLOCKED_USER_ACTION | steps in section L1 (fixture PASS: classification suite, 45 lib tests) |
| A-04 | Restricted/VIP/purchase/unavailable/trial/missing/contradictory fields never added | fixture | PASS | `availability_from_fields` suite; `restricted_song_candidate_is_preview_only`, `missing_or_string_entitlement_never_becomes_free`, trial tests |
| A-05 | `preview` performs zero remote writes (no create/add/remove/reorder; no trusted mapping created; no false state) | fixture | PASS | `preview_plans_free_song_without_remote_write`, `audit_is_read_only_and_classifies_every_status` (assert create/add == 0) |
| A-06 | `sync` appends to owned `FreeFM · Auto`, re-reads remotely, second run adds nothing | live | BLOCKED_USER_ACTION | steps in section L1 (fixture PASS: `sync_is_append_only_and_second_run_is_idempotent`) |
| A-07 | Concurrent sync: one process wins, other gets stable concurrent error | fixture | PASS | `lock_is_exclusive_and_removed_on_drop`, `flock_blocks_another_process_and_recovers_after_kill` |
| A-08 | Candidate is never auto-used before user confirmation | fixture | PASS | `restricted_song_candidate_is_preview_only`, `stale_trusted_mapping_falls_back_and_reports_invalid` |
| A-09 | Credentials/playback URL never printed, stored in JSON/state/log, or committed | static | PASS | JSON validity + secret scan of all command outputs; `gitleaks dir .` (748 MB, no leaks); URL is in-memory-only by construction (`Probe` holds booleans only) |
| A-10 | Playlist target: only owned `FreeFM · Auto`; ambiguous/non-owner/stale-ID fail closed | fixture | PASS | `multiple_owned_same_name_playlists_fail_closed`, `remote_named_playlist_is_authoritative_over_stale_state`, owner checks in `playlist_summary_by_id` |
| A-11 | Session expiry/revocation stops remote writes with stable actionable error | fixture | PASS | `login_expiry_stops_before_private_fm`, `timeout_and_login_errors_are_safe_categories`; exit matrix below |
| A-12 | Manual user tracks never deleted/reordered | live | BLOCKED_USER_ACTION | steps in section L2 (no delete path exists; fixture: append-only dedupe) |
| A-13 | Scheduler executes deterministic `sync --quiet` / `audit --quiet` with zero LLM | static + external | PASS (examples) / BLOCKED_EXTERNAL (24h real run) | helpers run the binary directly; OpenClaw/Hermes evidence in V01; 24h live scheduler observation is a time gate |
| A-14 | `audit` reads actual playlist, reuses the sync playability logic, reports the four buckets, zero remote writes | live | BLOCKED_USER_ACTION | steps in section L1 (fixture PASS: `audit_is_read_only_and_classifies_every_status`, `audit_without_playlist_is_healthy_and_read_only`) |
| A-15 | `audit --quiet`: silent on healthy, exit 3 + structured output on attention | static/live | PASS (code path) / BLOCKED_USER_ACTION (attention sample) | exit-code matrix below; attention branch needs a live sample or fixture (fixture covers buckets) |
| A-16 | Audit attention reaches unattended automation | static | PASS | new `freefm-audit.sh` helper pair, OpenClaw `--every 24h` + Hermes `0 3 * * *` examples, CI compares both copies, WorkBuddy ZIP includes it |
| A-17 | `review` reject: no mapping, no remote writes | fixture | PASS | `review_without_confirmation_writes_nothing` |
| A-18 | `review` approve: prompt content, explicit `y`, local persist, no immediate remote write | fixture | PASS | `review_persists_only_explicit_confirmation`; live run pending (L3) |
| A-19 | Trusted mapping survives process restart | fixture | PASS | `trusted_store_roundtrip_and_corrupt_recovery` (save/load); live restart pending (L3) |
| A-20 | Trusted target re-verified free on every use; stops when restricted/unavailable/unknown | fixture | PASS | `trusted_mapping_is_used_only_while_target_still_free`, `trusted_target_becoming_restricted_stops_usage` |
| A-21 | Trusted mapping revoke/replace: user can remove or overwrite; no silent duplicates | fixture | PASS | `review_remove_deletes_only_requested_mapping`, `review_replaces_stale_mapping_with_new_confirmation` (new in this batch) |
| A-22 | Re-auth keeps playlist binding, added_song_ids, trusted mappings | fixture | PASS | `reauth_preserves_state_json_and_playlist_binding`, `sync_records_added_song_ids_and_old_state_stays_compatible`; live pending |
| A-23 | JSON/quiet/exit codes stable and documented | static | PASS | exit-code matrix below; all five `--json` commands validated as parseable JSON with no secrets |
| A-24 | 500+ playlist tracks pagination | fixture | PASS | `playlist_track_parser_handles_more_than_500_ids` |
| A-25 | State/session/trusted corruption fail closed without data loss | fixture | PASS | `corrupted_state_recovers_to_empty_state`, `corrupted_session_is_rejected_without_recovery`, `trusted_store_roundtrip_and_corrupt_recovery` |
| A-26 | FM passive fetch advances official App queue? | live | PASS (n=2 evidence, not proof) | two human runs 2026-08-10, both `official_queue_advanced=n`; recorded in V01; 7-day observation continues |
| A-27 | 7-day passive observation | live | BLOCKED_EXTERNAL | until 2026-08-16 23:21 Asia/Shanghai; no fabricated results |
| A-28 | Resource profile unchanged by audit/review/trusted (size, cold start, RSS, state size, no daemon/db) | static | PASS | numbers in section below; binary grew 1.6% (1,854,240 -> 1,887,456 B), no new deps |
| A-29 | README claims match implementation (long-lived = detect+report, not auto-repair; candidate = review/approve, not auto-swap; audit in automation) | static | PASS | README/zh-CN updated this batch; audit job documented; no overclaim remains |
| A-30 | Homebrew/WorkBuddy import/other platforms | external | NOT_APPLICABLE for v0.1 gate (P2); installer smoke PASS | P2 list |

## Exit-code matrix (verified on the release binary, no session)

| Command | Condition | Exit | Output |
|---|---|---|---|
| preview/sync/audit | no session | 1 | JSON `login_required` (or human text) |
| status/doctor | no session | 0 | `authenticated:false` JSON |
| review | `--quiet` or non-tty | 2 | usage error |
| audit | healthy (all still_free) | 0 | `--quiet` silent |
| audit | needs attention | 3 | structured JSON/human report |
| any | parse/usage error | 2 | error text |
| auth | QR confirmed | 0 | success line |

## Resource numbers (this batch, release binary, arm64)

- binary: 1,887,456 B (1.80 MB); SHA-256 `acd7f9a859e2a151f39bfb48ccfd16b9f6483e48e32328659bd7dcf17ba33575`
- `--version` cold start: 0.11 s in sandbox; 0.01 s measured earlier on bare metal (V01), peak RSS 2,129,920 B (V01; `/usr/bin/time -l` blocked in this sandbox)
- `~/.freefm` state: 8.0 KB; no audio/cover/lyrics/cache/logs
- idle: zero processes/RAM (one-shot CLI, exits after each run); no daemon/db/Node/Python
- trusted store: single small JSON, 0600, atomic rename

## P0 / P1 / P2 status

- P0: no FAIL found in anything verifiable offline; live chain pending user (L1-L3).
- P1: review revoke/replace gap found and fixed this batch (A-21); audit automation gap found and fixed this batch (A-16); both covered by new regression tests and CI.
- P2 (not blocking v0.1): Windows, Homebrew tap, WorkBuddy signed import, TUI polish, auto-repair (intentionally absent in v0.1).

## BLOCKED_USER_ACTION steps

### L1. Fresh-dir real-account main chain (about 5 minutes)

```sh
cd /Users/yuxinqiao/Developer/free-music-agent
bash scripts/acceptance-live.sh
```

The script uses `~/.freefm-acceptance/` (does not touch `~/.freefm`), prompts
the official-client QR scan once, then runs: status after restart, preview,
sync (reuses an existing owned `FreeFM · Auto` if present, never creates a
second one), preview reread, second sync (idempotency), audit --json, and
audit --quiet. It records only aggregate counts and exit codes in
`~/.freefm-acceptance/acceptance.jsonl`. Confirm the printed summary:
`sync-1` exit 0 with a `would_add_count >= 1` or a valid owned playlist,
`sync-2` exit 0 with `would_add_count == 0`, `audit` exit 0.

### L2. Manual-track safety (1 minute)

In the official client, manually add any song to `FreeFM · Auto` (or to a
separate copy if you prefer not to touch the real playlist), then run
`bash scripts/acceptance-live.sh` again and verify in the official client that
the manual song is still present and in the same position.

### L3. Review loop: reject + approve + restart reuse (needs a candidate)

1. Run `freefm preview --json --data-dir ~/.freefm-acceptance` until a
   `candidate_only` decision appears (restricted originals are common; a few
   runs are usually enough).
2. Run `freefm review --data-dir ~/.freefm-acceptance`, answer `n` for the
   first candidate: confirm no mapping was written (`ls ~/.freefm-acceptance/trusted.json`).
3. Run it again, answer `y` for a candidate: the prompt shows original and
   candidate title/artist/album/duration delta/version markers plus the
   "no authoritative recording identity" disclaimer.
4. Re-run `freefm status --json` and then `freefm preview --json` from a new
   process: the same original must show action `trusted_mapping` with the
   approved target, and `sync` must be able to use it (would_add contains the
   target when not already present).
5. Optionally remove it again: `freefm review`, then enter the listed index to
   remove; verify `trusted.json` no longer contains it.

## BLOCKED_EXTERNAL

- 7-day passive-FM observation: until 2026-08-16 23:21 Asia/Shanghai.
- 24-hour real scheduler observation of the 6h sync + daily audit jobs (setup
  steps are documented; results are time-gated, not fabricated).

## Conclusion (2026-08-10)

All immediately verifiable P0/P1 items pass; two real gaps found during this
acceptance (review revoke/replace, audit in automation) were fixed and
regression-tested. The live user-action chain (L1-L3) and the time gates are
open. Status:

`FUNCTIONALLY_READY_WAITING_FOR_LONG_RUN_GATE` (with open BLOCKED_USER_ACTION
items L1-L3; any FAIL found there drops this to `FAIL_CONTINUE_FIXING` and work
continues until it passes).

`READY_FOR_V0.1.0` is not granted until L1-L3 pass and the 7-day gate closes.
