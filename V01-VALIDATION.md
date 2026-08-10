# FreeFM v0.1 validation record

Date: 2026-08-10 (Asia/Shanghai)

## Release gate

- `cargo fmt --all -- --check`: passed.
- `cargo test --all-targets`: 38 passed (33 lib + 1 ignored child-process lock helper + 4 TUI).
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release`: passed.
- `cargo-audit audit`: passed; 1,198
  advisories loaded and 174 locked dependencies scanned with no finding.
- release binary: 1,854,240 bytes, stripped arm64 Mach-O.
- SHA-256:
  `22d15ee721a9555eae0896c39806ff85d54f984c97a4b7733320536f7d2403ba`.
- cold `--version` execution: 0.01 seconds real time, 2,129,920 bytes maximum resident set size.

All automated tests use the in-process fake protocol seam and redacted
fixtures. They never contact NetEase. Coverage includes strict entitlement
parsing, malformed and contradictory fields, trial/restricted/unavailable
songs, candidate-only replacement, preview zero-write, second-run idempotency,
owner collisions, more than 500 playlist tracks, timeout/5xx/incompatible
responses, post-create and post-append recovery, state-save failure recovery,
cross-process lock exclusion, JSON/quiet behavior, session expiry/revocation,
re-authentication state preservation, and credential redaction.

### Release build and zero-Rust-toolchain installation

On 2026-08-10, GitHub Release precompiled artifact build matrix (`.github/workflows/release.yml`) was established for `darwin-arm64`, `linux-x86_64`, and `linux-arm64`. Each build runs `cargo fmt`, `cargo test`, `cargo clippy`, `cargo build --release`, and generates SHA-256 checksums (`.tar.gz.sha256`).

A zero-dependency POSIX installer script (`scripts/install.sh`) performs OS/arch detection, release downloading, SHA-256 checksum validation, installation to `$HOME/.local/bin/freefm`, and `--version` verification. It does not install Rust/Node/Python/Docker, does not use `sudo`, and does not alter shell profiles. A Homebrew tap formula template (`scripts/formula/freefm.rb`) is provided for `Yuxin-Qiao/homebrew-tap`.

The WorkBuddy packaging script produced a ZIP containing only
`freefm/SKILL.md` and `freefm/scripts/freefm-sync.sh`; CI validates both paths
and shell syntax. Tencent WorkBuddy 5.1.2 for Apple silicon was downloaded from
the official archive, installed, and accepted by macOS Gatekeeper with a
stapled notarization ticket and the Tencent Technology (Shanghai) Company
Limited Developer ID signature.

### Codex platform support

On 2026-08-10 the same `skills/freefm` folder was validated as a Codex skill:
its `name`/`description` frontmatter matches the format Codex discovers under
`~/.codex/skills/<name>/SKILL.md`, and an `automation/codex/README.md` template
was added. `codex exec` and desktop recurring automations start an Agent turn (LLM tokens),
so the documented zero-token path stays with the OS scheduler or the deterministic
`codex sandbox -- freefm sync --quiet` runner.

## Live session and write proof

The final proof used the exact release binary identified above and a fresh data
directory whose persisted session contained only `MUSIC_U`:

- a restarted process returned authenticated with explicit `vipType=0`;
- the first `sync --json` selected an already-owned playlist, classified one
  source track as strictly free, appended it, and confirmed it by re-reading;
- the same run skipped two restricted tracks and added no searched replacement;
- measured work: 16 public client calls, 19 actual HTTP requests, 4.02 seconds
  wall time, and 15,269,888 bytes peak RSS;
- a second execution of the same binary with `sync --quiet` exited 0 with zero
  stdout and zero stderr, proving the steady-state scheduler contract;
- persisted sizes after the run: 1,071-byte session, 64-byte state, empty lock
  file; no music, cover, lyric, URL, or complete API response was cached.

## Session lifecycle and re-authentication

Server-side session expiry/revocation handling was explicitly hardened and tested:
- Responses containing status code 301, 401, 403, or null account objects are classified as `AppError::LoginRequired`.
- Encountering expired authentication during `sync --quiet` triggers immediate fail-closed exit: no remote playlist creation or track appends occur, state file remains unmutated, and credentials/cookies are never printed or logged. Output returns `kind: "login_required"` with stable exit code 1 and prompt `"登录已失效或尚未登录；请运行 freefm auth"`.
- Re-authenticating via `freefm auth` writes an updated `session.json` while preserving existing `state.json` (including `playlist_id` binding and sync timestamps). Subsequent `sync` runs automatically resume appending to the existing `FreeFM · Auto` playlist without creating duplicate playlists.

## HTTP accounting

`http_requests` is the real request count for the pinned `netease-music =
0.1.1` implementation. Every client call counts one request, while
`playlist_track_all` additionally counts the song-detail request emitted per
500 track IDs. Offline tests cover 0, 1, 500, 501, and larger track sets.

## Passive Private FM observation

The repository script `experiments/passive-fm-observe.sh` performs read-only
sampling without playback, skip, trash, or scrobble calls. It deletes raw
responses and stores only salted per-run hashes and aggregate counts.

As of 2026-08-11 06:09 Asia/Shanghai (2026-08-10 22:09 UTC), 33 observations
over 30 hours 48 minutes produced 33 distinct 3-track batches and 94 unique
salted track hashes, with zero failure records and zero duplicate batch
hashes. All 31 hourly session checks remained authenticated with explicit
`vipType=0` (`client_calls=1`, `http_requests=1`, `login_required=false`).

The +24h gate (2026-08-10 23:21 Asia/Shanghai) is reached: passive Private FM
sampling continues to produce fresh recommendation batches without playback,
skip, trash, or scrobble calls, and the QR session remains valid across
process restarts. The FM-queue experiment (two human runs, both
`official_queue_advanced=n`) and the live acceptance chain
(FUNCTIONAL-ACCEPTANCE.md, real account, 2026-08-11 00:09-00:51 CST) are
recorded separately; the 7-day observation gate remains
(2026-08-16 23:21:34 Asia/Shanghai).

This confirms that passive polling causes NetEase to return fresh recommendation batches without requiring playback, skip, or scrobble actions. The LaunchAgent `com.freefm.validation` continues running toward the 7-day observation gate (2026-08-16 23:21:34 Asia/Shanghai).

## Same-recording identity probe

The live song-detail response and `/api/song/play/about/block/page` (the current
song-wiki summary endpoint) were recursively inspected by field name only. No
ISRC, immutable recording ID, or equivalent authoritative identity signal was
present. Consequently v0.1 keeps searched free versions as preview-only
`candidate_only` results; title, artists, duration, album, and version markers
are insufficient proof for automatic substitution.

## Scheduler proof

Hermes 0.17.0 has an hourly job named `freefm-hourly` using
`freefm-sync.sh` with `no_agent=true`. A manual scheduler trigger completed
successfully. Its persisted run record states `Mode: no_agent (script)` and
`Status: silent (empty output)`, so that execution used no Agent/LLM and
produced no routine output.

OpenClaw 2026.8.1 installed the skill from both a local package and
`git:Yuxin-Qiao/FreeFM@main` in isolated state/workspace directories. On 2026-08-10 an
isolated token-authenticated Gateway executed a command job whose exact
argv was a proof executable followed by `sync --quiet`. The run completed in
555 ms with exit code 0, no stdout or stderr, no delivery request, and no agent
message payload.

ClawHub published `@yuxin-qiao/freefm` version `0.1.0-alpha.3` from the same
GitHub commit and moved both `alpha` and `latest` to that version.

## Remaining external gates

- Complete the running 7-day passive FM observation (gate: 2026-08-16 23:21 Asia/Shanghai) before enabling default unattended background synchronization.
- Complete the signed Tencent WorkBuddy client import after interactive login.

## Functional additions: audit, review, trusted mapping (2026-08-10)

Implemented after the release gate above, all covered by the same fake-transport
fixture suite (43 lib + 4 TUI tests passing):

- `freefm audit` re-checks every `FreeFM · Auto` track with the same strict
  playability logic as `sync`, reporting `still_free`, `became_restricted`,
  `unavailable`, and `unknown` plus a stable JSON schema, `needs_attention`,
  and exit code 3. It performs no remote write: tests assert zero create/add
  calls and no delete/reorder path exists.
- `freefm review` interactively shows high-similarity free candidates
  (title, artists, duration delta, album, version markers, why it matched)
  and only persists a local `trusted.json` mapping after an explicit user `y`.
  The approved target is re-verified free at review time and again on every
  later use; if it becomes restricted/unavailable, the mapping stops being used
  (`trusted_invalid`) and the decision falls back to candidate-only.
- `sync` now records the song IDs FreeFM itself appended in
  `state.json` (`added_song_ids`, `#[serde(default)]`, backward compatible) to
  scope any future safe repair. v0.1 still never deletes or repairs.
- No ISRC/recording identity exists in live responses (see above), so trusted
  mappings are human-confirmed only; there is no automatic fuzzy replacement,
  no confidence-threshold auto-upgrade, and no audio fingerprinting.
- Default scheduled-sync examples were relaxed from hourly to every 6 hours so
  `FreeFM · Auto` grows slowly; the binary still supports any user-chosen
  frequency and FreeFM itself still contains no scheduler.

Live-account behavior of `audit` and `review` (including the FM-queue
consumption question) is pending the human-in-the-loop experiment in
`scripts/fm-queue-experiment.sh`; no result is claimed here until it runs.

## FM-queue consumption experiment (2026-08-10, run by the account owner)

Procedure: pause the read-only observation LaunchAgent; the owner opens the
official NetEase client Private FM and notes the first track without playing,
skipping, or scrubbing; FreeFM performs one pure `preview` fetch (no
playback/skip/trash/scrobble); the owner re-opens the client and compares.
Only salted track hashes, aggregate counts, and the owner's `y/n/u` answer are
persisted in `~/.freefm-validation/fmqueue-experiment.jsonl`.

Result: two independent runs at 2026-08-10 15:20:07Z and 15:20:25Z, both with
`official_queue_advanced = "n"` (the owner saw no advancement or change in the
client-side Private FM queue after a 3-track, 15-HTTP-request read-only fetch).
This is a subjective n=2 observation, not a proof: the client queue is
client-local state and the server continues to return fresh recommendation
batches on each fetch. Current stance: treat passive fetch as "auto-sample new
recommendations" with no observed client-queue consumption; keep the
conservative 6-hour default recommendation and do not increase frequency based
on this result. If a later sample shows queue advancement, reduce frequency or
add an explicit user opt-in.
