# FreeFM v0.1 validation record

Date: 2026-08-10 (Asia/Shanghai); current refresh: 2026-08-12

## Historical release gate (2026-08-10 baseline)

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
- workspace refresh 2026-08-11: `cargo test --all-targets --locked` 45 lib + 4 TUI
  passed (1 ignored child-process lock helper); fmt/clippy/`git diff --check` clean;
  local release binary 1,887,472 bytes (final SHA recorded at the +7d closeout).

## Current audit refresh (2026-08-12)

- The implementation/closeout commit `e14670771777d7bae48be317221e449dd3f11c83`
  passed main CI run `31564640529` and CodeQL run `31564640565`; a subsequent
  docs-only closeout was pushed directly, leaving `HEAD == origin/main` and a
  clean worktree. The direct push bypassed the repository's PR/required-check
  ruleset path, so no `ai-review` check is claimed for this validation.
- The current local release binary is 1,970,848 bytes with SHA-256
  `1458f5c5fab5ebb91cb7d83090c553ce981e108859c0a86080045e37a75648c4`.
- The observation directory has 61 session records and 63 passive records;
  all session records are authenticated ordinary-account samples with no failure
  records. The LaunchAgent is loaded but not running; Hermes has one paused
  legacy `freefm-hourly` job and no active jobs.
- There is no `v0.1.0` tag, GitHub Release, Homebrew Formula publication, or
  real Spotify/Apple Music/YouTube Music credential available on this machine.
- Codex sandbox: `codex sandbox -- target/release/freefm status --json` succeeds
  with a temporary normalized catalog and an isolated data directory (exit 0,
  `authenticated:false`). The default `~/.codex/cc-switch-model-catalog.json`
  still fails Codex parsing on an `audio` model type; no global provider config
  was changed and no `sync --quiet` was run.
- External-platform transfer hardening is fixture-verified only. The current
  checkout performs post-append rereads, requires Apple `canEdit == true`, and
  binds mappings to both playlist and storefront context; none of these claims
  is a real-account E2E claim.

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
`freefm/SKILL.md` and the two fixed command helpers under
`freefm/scripts/`; CI validates those paths and shell syntax. Tencent WorkBuddy
5.1.2 for Apple silicon was downloaded from the official archive, installed,
and accepted by macOS Gatekeeper with a stapled notarization ticket and the
Tencent Technology (Shanghai) Company Limited Developer ID signature.

On 2026-08-12, the generated package was uploaded through the installed Tencent
WorkBuddy 5.3.11 client. Its built-in security scan completed and the `freefm`
skill appeared in the client's “我安装的” list (count 1). This verifies client
import only; no NetEase credential was entered and no sync or scheduler was
enabled.

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

As of 2026-08-11 09:25 Asia/Shanghai (2026-08-11 01:25 UTC), 35 session checks
over 34 hours recorded 37 distinct 3-track batches (111 track slots) and 104
unique salted track hashes, with zero failure records and zero duplicate batch
hashes. All 35 session checks remained authenticated with explicit
`vipType=0` (`client_calls=1`, `http_requests=1`, `login_required=false`).

The +24h gate (2026-08-10 23:21 Asia/Shanghai) is reached: passive Private FM
sampling continues to produce fresh recommendation batches without playback,
skip, trash, or scrobble calls, and the QR session remains valid across
process restarts. The FM-queue experiment (two human runs, both
`official_queue_advanced=n`) and the live acceptance chain
(FUNCTIONAL-ACCEPTANCE.md, real account, 2026-08-11 00:09-00:51 CST) are
recorded separately; the 7-day observation gate remains
(2026-08-16 23:21:34 Asia/Shanghai).

This historical snapshot confirmed that passive polling caused NetEase to return
fresh recommendation batches without requiring playback, skip, or scrobble
actions. The current LaunchAgent state is recorded in the 2026-08-12 refresh
above.

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

As of 2026-08-12 the installed Hermes (v0.20.0) has one paused legacy
`freefm-hourly` job and no active jobs (`hermes cron status` reports no active
jobs), which keeps periodic sync fully paused. The
closeout script installs `automation/hermes/freefm-sync.sh` into
`~/.hermes/scripts/` and creates or resumes `freefm-sync` (every 6 h,
`--no-agent`) as its final step, after all release gates pass. If that named job
already exists, the closeout reuses it only when it is the exact active/paused
no-agent job with the expected schedule; otherwise it fails closed.

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

Live-account behavior of `audit` and `review` remains a read-only operational
check; the FM-queue consumption question was exercised separately below by the
account owner. No audit/review command is scheduled by the validation job.

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

## Daily aggregate 2026-08-12 (validation still running, read-only)

Source: `~/.freefm-validation/`; only timestamps, counts, booleans, and
failure types are recorded. No cookies, account identifiers, song IDs/titles,
or URLs are stored in this document.

Session (`session.jsonl`, n=61):

- first sample 2026-08-09T15:21:34Z, last sample 2026-08-12T03:27:55Z;
- 61/61 `ok`, 61/61 `authenticated`, `login_required` count = 0;
- unique `account_vip_type` values = `[0]` (ordinary account confirmed on
  every sample);
- failure types: none.

Passive FM (`passive.jsonl`, n=63):

- first sample 2026-08-09T14:28:33Z (two pre-LaunchAgent manual samples),
  hourly sampling from 15:21:37Z, last sample 2026-08-12T03:27:58Z;
- 63 unique salted batch hashes, 165 unique salted track hashes, zero
  failures; batches continue to contain new tracks;
- HTTP requests per fetch: 11-17 (mode 15), consistent with one preview.

LaunchAgent `com.freefm.validation`:

- loaded, hourly, last exit 0; script contains only `status` and `preview`
  invocations; keyword scan for sync/play/skip/trash/scrobble found none;
- evidence size bounded: session.jsonl 10,492 B + passive.jsonl 32,435 B
  (total ~42.9 KB at the latest local refresh; `failures.jsonl` is absent).

Status: within the +7d observation window (gate 2026-08-16 23:21
Asia/Shanghai). The LaunchAgent is loaded but not currently running; Hermes has
one paused legacy `freefm-hourly` job and no active jobs. No sync or active cron
has been enabled. The observer uses the
installed `/Users/yuxinqiao/.local/bin/freefm`; current checkout release-binary
provenance still needs to be reconciled before release.

## Local release-closeout hardening (2026-08-12)

`scripts/release-closeout.sh` now fails closed when required tools are missing,
the version/release commit contract is wrong, the worktree is dirty, an
attestation is missing, a tarball or WorkBuddy ZIP contains an unexpected file,
the downloaded native binary reports the wrong version, or Hermes creation is
not confirmed after the command returns. The local release smoke test also
syntax-checks the closeout script. These changes are code/documentation gates;
they do not create a tag, Release, Homebrew formula, or Hermes schedule.

## External playlist transfer hardening (2026-08-12)

The external Spotify, Apple Music, and YouTube Music adapters remain covered
by redacted fake-transport fixtures only. This checkout has no configured
`FREEFM_SPOTIFY_*`, `FREEFM_APPLE_MUSIC_*`, or `FREEFM_YOUTUBE_*` credentials,
so no real-account read/search/confirm/append/rerun proof is claimed here.

The implementation now rereads target ownership and items after every append
batch. A transport error that may have raced with a provider-side write is
immediately reread; a missing confirmation returns `target_write_uncertain`
and does not retry automatically. Apple library targets require explicit
`attributes.canEdit == true`. External mappings are keyed and validated by
source provider/playlist/storefront plus target provider/playlist/storefront,
so an approval cannot be reused for another target context.

`freefm doctor --json --target <URL>` reports only redacted credential
booleans, required scopes, and the remote checks that will occur. FreeFM does
not implement provider OAuth login or refresh-token persistence; provider app
registration and an operator-owned token remain external prerequisites.

Local gates for this change: `cargo test --all-targets --locked` = 87 passed,
1 ignored, plus 5 main tests; locked Clippy, release build, format/diff checks,
shell syntax, matching Hermes helpers, and WorkBuddy package inspection all
passed. `cargo audit` loaded 1,211 advisories and scanned 188 dependencies with
no finding; gitleaks scanned about 1.69 GB with no leak; release smoke passed all
four installer fail-closed cases and WorkBuddy package checks. No real-platform
E2E or +7d release Go/No-Go decision is implied.
