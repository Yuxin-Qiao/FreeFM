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

## Data and credential rules

Do not commit cookies, `MUSIC_U`, QR keys or images, account/profile IDs, real
playlist or song IDs, playback URLs, listening history, complete API payloads,
or raw request/response logs. New fixtures must follow
`FIXTURE-REDACTION.md`.

Changes that unlock restricted content, obtain restricted audio, replace
playback URLs, download audio, or silently substitute a different recording
are out of scope.
