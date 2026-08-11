# Contributing to FreeFM

FreeFM is intentionally small and fail-closed. Contributions must preserve the
one-shot native CLI, append-only playlist behavior, ordinary-account semantics,
and the rule that uncertain entitlement or recording identity is skipped.

## Development gate

Run before submitting a change:

```shell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo audit
git diff --check
```

Tests must use the fake `RemoteApi` seam and minimized fixtures. CI must never
contact NetEase or use a real account.

The repository automation, workflow security boundaries, required checks, and
external GitHub setup are documented in
[docs/github-automation.md](docs/github-automation.md).

## Data and credential rules

Do not commit cookies, `MUSIC_U`, QR keys or images, account/profile IDs, real
playlist or song IDs, playback URLs, listening history, complete API payloads,
or raw request/response logs. New fixtures must follow
`FIXTURE-REDACTION.md`.

Changes that unlock restricted content, obtain restricted audio, replace
playback URLs, download audio, or silently substitute a different recording
are out of scope.

## Releasing

FreeFM releases are deliberately restrained and stability-first. All release
policy, versioning rules, observation windows, gates, and No-Go conditions
live in [RELEASING.md](RELEASING.md). Every release requires the full gate
suite and a human Go/No-Go; there are no hotfix exemptions.
