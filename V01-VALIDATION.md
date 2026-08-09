# FreeFM v0.1 validation record

Date: 2026-08-09 (Asia/Shanghai)

## Release gate

- `cargo fmt --all -- --check`: passed.
- `cargo test --all-targets`: 31 passed, 1 ignored child-process lock helper.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release`: passed.
- `cargo-audit audit`: passed; 1,198
  advisories loaded and 174 locked dependencies scanned with no finding.
- release binary: 1,802,256 bytes, stripped arm64 Mach-O.
- SHA-256:
  `69163eca36f9321cdaf6c61e1acd9ddb15f4b8c93c9fd1b86076fca0818ac42f`.

All automated tests use the in-process fake protocol seam and redacted
fixtures. They never contact NetEase. Coverage includes strict entitlement
parsing, malformed and contradictory fields, trial/restricted/unavailable
songs, candidate-only replacement, preview zero-write, second-run idempotency,
owner collisions, more than 500 playlist tracks, timeout/5xx/incompatible
responses, post-create and post-append recovery, state-save failure recovery,
cross-process lock exclusion, JSON/quiet behavior, and credential redaction.

### Current product-experience build

The original live proof below remains tied to its exact frozen binary. After
adding the optional `freefm tui` front end on 2026-08-10, the current release
build passed 34 tests with 1 ignored child-process helper. The stripped arm64
binary is 1,854,192 bytes with SHA-256
`306a73cb3528a58a45712a1e7a31b7cb56953462e7056b99e8e3c3694611e065`.
That is a 51,936-byte (2.9%) increase over the live-proof binary. A cold
`--version` execution measured below 0.01 seconds and 2,097,152 bytes maximum
resident set size. The TUI is not loaded by normal `sync --quiet` scheduling.

The WorkBuddy packaging script produced a ZIP containing only
`freefm/SKILL.md` and `freefm/scripts/freefm-sync.sh`; CI validates both paths
and shell syntax. Tencent WorkBuddy 5.1.2 for Apple silicon was downloaded from
the official archive, installed, and accepted by macOS Gatekeeper with a
stapled notarization ticket and the Tencent Technology (Shanghai) Company
Limited Developer ID signature. This establishes client provenance and package
reproducibility, not a completed import: interactive user login is still
required before the local ZIP can be uploaded.

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

The validation artifacts record only counts and hashes. Song IDs, titles,
playlist IDs, cookies, playback URLs, and complete responses are deliberately
excluded from this repository.

## Correctness issue found during live validation

The crate's song-detail response returns privileges in a top-level array rather
than inside each song object. Replacing Private FM metadata with song-detail
objects therefore discarded the entitlement evidence and made every track
`unknown`. FreeFM now joins top-level privileges by song ID and merges detail
metadata without discarding the FM privilege, fee, duration, artist, or album.
A regression test covers this exact response shape.

## HTTP accounting

`http_requests` is the real request count for the pinned `netease-music =
0.1.1` implementation. Every client call counts one request, while
`playlist_track_all` additionally counts the song-detail request emitted per
500 track IDs. Offline tests cover 0, 1, 500, 501, and larger track sets.
`client_calls` remains available to show the public wrapper operations.

## Passive Private FM observation

The repository script `experiments/passive-fm-observe.sh` performs read-only
sampling without playback, skip, trash, or scrobble calls. It deletes raw
responses and stores only salted per-run hashes and aggregate counts.

As of 2026-08-10 02:21 Asia/Shanghai, six observations over 3 hours 53 minutes
returned six distinct three-track batches and 18 distinct salted track hashes.
Four hourly session checks all remained authenticated with explicit
`vipType=0`. This demonstrates that passive reads can change recommendations
without playback behavior. It does **not** yet establish the +24 hour or
multi-day behavior, nor whether the server internally consumes a batch when it
is fetched.

A user LaunchAgent, `com.freefm.validation`, now runs the repository's
`automation/launchd/freefm-validation-observe.sh` once per hour. It invokes only
`status` and `preview`, exits after each sample, suppresses routine output, and
stores only mode-600 sanitized summaries under `~/.freefm-validation/`. A
Codex heartbeat checks the evidence every six hours and will remove the
LaunchAgent after the seven-day gate. This validation-only heartbeat is not
part of normal FreeFM synchronization.

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
produced no routine output. The job is paused and the Hermes gateway is not
started until the longer passive-FM observation gate is complete.

The public Hermes source was updated to commit
`8d16bda5c2ad0cd27d8d176c2d086b3b1c618471` and passed a fresh Hermes
community scan as `SAFE / ALLOWED`. Hermes 0.17 fetched only `SKILL.md`; the
published instructions therefore include an immutable commit URL and
SHA-256-verified fallback for the no-agent helper.

OpenClaw 2026.8.1 installed the skill from both a local package and
`git:Yuxin-Qiao/FreeFM@main` in isolated state/workspace directories. In both
cases `openclaw skills list` reported `freefm` as ready. On 2026-08-10 an
isolated token-authenticated Gateway then executed a command job whose exact
argv was a proof executable followed by `sync --quiet`. The run completed in
555 ms with exit code 0, no stdout or stderr, no delivery request, and no agent
message payload. The proof job was removed and the Gateway stopped immediately
afterward. This validates the deterministic Gateway command path without
contacting NetEase or using an LLM; it does not enable unattended production
sync before the longer passive-FM gate.

ClawHub published `@yuxin-qiao/freefm` version `0.1.0-alpha.3` from the same
GitHub commit and moved both `alpha` and `latest` to that version. Its security
result was clean/benign with high confidence and no warning. An isolated
OpenClaw installation downloaded that exact public release, contained both
`SKILL.md` and `scripts/freefm-sync.sh`, exposed the TUI instructions, and
reported the skill as ready. The helper SHA-256 remained
`b9dd3bd85e32c8ce57ba11ef474149839ad898090495daf7336d396d37830fd1`.

## Remaining external gates

- Complete the running +24 hour and 7-day passive FM observations before
  enabling unattended synchronization.
- Test session expiry/revocation and QR re-authentication separately; restart
  restoration is proven, server-side expiry is not.
- Complete the signed Tencent WorkBuddy client import after the user completes
  its interactive login; package structure is already proven offline.
