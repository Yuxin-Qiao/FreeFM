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

Three observations over 53 minutes returned three distinct three-track batches
and nine distinct salted track hashes. This demonstrates that passive reads
can change recommendations without playback behavior. It does **not** yet
establish the +24 hour or multi-day behavior, nor whether the server internally
consumes a batch when it is fetched.

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

OpenClaw is not installed on this host, so its deterministic-command cron has
documentation but no host-level execution proof here.

## Remaining external gates

- Complete the running +24 hour and 7-day passive FM observations before
  enabling unattended synchronization.
- Test session expiry/revocation and QR re-authentication separately; restart
  restoration is proven, server-side expiry is not.
- Validate the OpenClaw deterministic-command configuration on a host where
  OpenClaw is installed.
- Re-run the release gate on CI after the repository is published.
