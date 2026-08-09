# FreeFM contributor guide for AI agents

FreeFM is a native Rust product. The platform skills under `skills/` are thin
installation and scheduling adapters; they are not the product runtime.

## Product invariants

- Keep `preview` read-only.
- Keep `sync` append-only: never delete, reorder, or alter existing tracks.
- Never unlock restricted audio, replace playback URLs, download media, or
  automatically substitute a searched recording.
- Treat missing, malformed, or contradictory playability evidence as unknown
  and skip it.
- Require an explicitly confirmed ordinary account (`vipType == 0`) before any
  playlist write.
- Verify the target playlist name and owner. Fail closed on ambiguity.
- Keep credentials local. Never print or commit cookies, `MUSIC_U`, QR keys,
  playback URLs, complete API responses, or account identifiers.
- Preserve the one-shot model: no daemon, scheduler, database service, web
  server, or model call is part of normal synchronization.

## Command boundaries

- `auth` is interactive and may only persist the minimal local session.
- `preview`, `status`, and `doctor` must not mutate remote playlists.
- `sync` is the only command allowed to append playlist tracks.
- `tui` is only a front end for existing commands. It must not create a second
  implementation of authentication, classification, or synchronization.
- Scheduled operation must remain `freefm sync --quiet`, not `freefm tui` and
  not an Agent prompt.

## Development workflow

Use fake transports and redacted fixtures for automated tests. Do not call the
live NetEase service in CI. Real-account experiments require an explicit human
QR login and must not expose or persist evidence beyond the documented minimal
state.

Before proposing a change, run:

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
git diff --check
```

If a platform skill changes, also run the Skill validator, compare the two
Hermes helper copies, validate shell syntax, build the WorkBuddy package, and
inspect its file list. Keep required technical identifiers lowercase:
`freefm` for the binary, crate, directories, and skill slug; use `FreeFM` for
human-facing product text.

## Scope discipline

Prefer measured changes over speculative protocol rewrites. Record live claims
with timestamps and sanitized evidence in `V01-VALIDATION.md`. Preserve user
work and unrelated changes. Never modify DNS, VPN, proxy, Cargo mirrors, or
host credentials to make a test pass.
