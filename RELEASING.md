# FreeFM Release Policy

FreeFM releases are deliberately restrained and stability-first. Every release
must fail closed; the cadence is decided by humans, never by automation or
deadlines.

## 1. Versioning

FreeFM follows Semantic Versioning and is currently in the `0.x.y` phase. A
valid release must have matching versions in all of:

- `Cargo.toml` `version`
- the git tag `v<version>`
- the Homebrew formula pinned to that tag with real checksums

Version bump rules:

- major (`X.0.0`): breaking CLI, protocol, or output contracts; changes to
  append-only playlist behavior, free-playability classification, credential
  handling, or MSRV; new platform obligations.
- minor (`0.X.0`): backward-compatible features or commands; new platform
  support.
- patch (`0.0.X`): bug fixes, documentation, dependency updates, or CI-only
  changes with no product behavior change.

A behavior change must never be hidden in a patch. Anything that changes
`sync`, free-playability classification, or credential handling is at least a
minor release; anything that breaks an existing contract is a major release.

## 2. Cadence and restraint

- No scheduled or automatic releases. Every release requires an explicit human
  Go/No-Go from the Release owner.
- At most one release tag per calendar day (Asia/Shanghai).
- Read-only observation windows (`status`/`preview` only):
  - patch: 24 hours
  - minor: 7 days
  - major: 7 days
- During observation, record session validity, ordinary-account confirmation
  (`vipType == 0`), salted uniqueness of passive-FM batches, and bounded
  evidence growth. Never run play, skip, trash, scrobble, or `sync` during
  observation.
- No hotfix exemption: any release, including urgent fixes, still passes the
  full gates and its observation window.

## 3. Release gates (every release, no exceptions)

- The version bump must be its own commit (`release: vX.Y.Z`) containing the
  version change and release notes, separate from feature commits.
- Local gates before tagging:
  - `cargo fmt --all -- --check`
  - `cargo test --all-targets --locked`
  - `cargo clippy --all-targets --locked -- -D warnings`
  - `cargo build --release --locked`
  - `git diff --check`
  - `cargo audit`
  - `gitleaks`
  - `scripts/release-smoke-test.sh`
- CI and repo state: `main` CI green; worktree clean; `HEAD == origin/main`.
- Supply chain: Actions references pinned to commit SHAs; the Release workflow
  produces artifact attestation and a CycloneDX SBOM; the release tarball
  contains only the binary, README, and LICENSE.
- Tag and publish: push `vX.Y.Z` only after all gates pass; wait for the
  Release workflow to complete green (build/attest/SBOM/publish); verify
  checksums and Homebrew `brew install` / `brew test` / `brew audit`.
- Evidence: record commands, exit codes, redacted summaries, and commit/tag
  SHAs in `V01-VALIDATION.md`; never write "verified" without the evidence.

## 4. No-Go conditions

Stop immediately and keep all scheduled `sync` paused when any of the following
occurs:

- login loss or authentication failure
- unexpected API structure changes
- playlist owner ambiguity
- contradictory or missing free-playability evidence
- leak-scan failure
- attempts to bypass environment limits (DNS, VPN, proxy, Cargo mirrors)
- any gate or workflow failure

## 5. Scope discipline

- Keep `preview` read-only and `sync` append-only. Never unlock restricted
  audio, replace playback URLs, download media, or auto-substitute recordings.
- Preserve the one-shot model: no daemon, scheduler, database service, web
  server, or model call as part of normal operation.
- Credentials stay local: never print or commit cookies, `MUSIC_U`, QR keys,
  playback URLs, full API responses, or account identifiers.
- `v0.1.0` is governed by `TEAM-COMPLETION-PLAN.zh-CN.md` and
  `scripts/release-closeout.sh`; this policy governs subsequent releases.
