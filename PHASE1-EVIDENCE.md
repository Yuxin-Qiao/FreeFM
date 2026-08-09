# FreeFM Phase 1 Evidence Report

> Historical bootstrap record. Its `IN_PROGRESS` and `UNVERIFIED` conclusions
> were superseded by the completed live safety proof in `V01-VALIDATION.md`.
> It is retained only to document the original environment and dependency
> baseline; do not use it as current release status.

Date: 2026-08-09 (Asia/Shanghai)

## Result

Phase 1 status: **IN_PROGRESS** (network gate passed with approved network
execution)

Current gate: **PHASE_READY_FOR_RUNTIME_EXPERIMENTS**

Product status: **UNVERIFIED**. No product-level Go/No-Go conclusion can be
made yet.

This checkout was empty at the start of the run: it contained no Git metadata,
Cargo manifest, Rust source, Phase 0 notes, or `netease-music` source/cache.
Cargo and Rust were installed. The initial sandbox had network disabled, but a
user-approved network execution path later allowed the real Cargo dependency
gate to pass. The earlier sandbox failure is retained below as historical
environment evidence, not as a current product blocker:

```text
cargo 1.97.1 (c980f4866 2026-06-30)
rustc 1.97.1 (8bab26f4f 2026-07-14)
`curl -I https://index.crates.io/` -> HTTP 200 (one successful attempt)
`curl -fsSL https://index.crates.io/config.json` -> success (one successful attempt)
`config.json` advertised `https://static.crates.io/crates` as the download base.
Subsequent checks failed with `Could not resolve host` for
`index.crates.io`, `static.crates.io`, and `github.com`.
```

No authenticated NetEase request has been made yet. No runtime product result
is inferred from the build-only evidence.

## 1. Exact operations tested

Only environment and local-source checks were run:

1. `git status --short --branch` — failed because this directory is not a Git
   repository.
2. Workspace file listing — no project files were present.
3. `cargo --version`, `rustc --version` — both available.
4. `curl -I https://index.crates.io/` — HTTP 200 on one attempt.
5. `curl -fsSL https://index.crates.io/config.json` — readable on one attempt;
   it pointed downloads at `static.crates.io`.
6. Per-host Python DNS lookup for `index.crates.io`, `crates.io`, and
   `github.com` — all failed with `gaierror(8)` on the later attempt.
7. `git ls-remote https://github.com/ddy314/netease-music.git HEAD` — failed
   because `github.com` could not be resolved.
8. `curl` download of
   `https://static.crates.io/crates/netease-music/netease-music-0.1.1.crate` —
   failed because `static.crates.io` could not be resolved.
9. `cargo search netease-music --limit 3` — failed earlier on registry DNS;
   this API result is not treated as the registry verdict.
10. Local source inspection of `reqwest 0.12.28` — completed.
11. ncm-spike QR authentication — QR was rendered locally and the polling
    process eventually returned `QR code expired`; no session was persisted.

### Follow-up network recovery gate

Timestamp: `2026-08-09T13:00:47+08:00`

This was a fresh gate attempt, not a product or architecture attempt:

```text
curl -fsSL https://index.crates.io/config.json >/dev/null
-> FAILED: curl: (6) Could not resolve host: index.crates.io

curl -fsSL https://static.crates.io/crates/serde/serde-1.0.0.crate >/dev/null || true
-> FAILED: curl: (6) Could not resolve host: static.crates.io

git ls-remote https://github.com/ddy314/netease-music.git HEAD
-> FAILED: Could not resolve host: github.com
```

Repeated Python DNS checks (three attempts each) returned
`gaierror(8)` for `index.crates.io`, `static.crates.io`, and `github.com` on
every attempt. Because the prerequisite hosts were unresolved, no temporary
Cargo probe was created and no `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
cargo fetch` was attempted; there was no Cargo layer to test beyond DNS.

### Network permission recovery and Cargo gate

Timestamp: `2026-08-09T13:41:16+08:00`

The environment variable check returned `CODEX_SANDBOX_NETWORK_DISABLED=1`.
The unapproved sandbox probe failed at DNS, while the user-approved network
execution path successfully read the sparse registry configuration.

A temporary probe under `/private/tmp` then ran:

```text
cargo add serde@1.0.0
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse cargo fetch
```

The fetch exited 0 and downloaded actual registry dependencies. The real
`baseline-netease` lockfile fetch initially hit a transient
`static.crates.io` DNS failure, then succeeded on one bounded retry with
`cargo fetch --locked`; no mirror or persistent configuration was changed.

No NetEase API operation was executed. Therefore there is no observed current
wire endpoint, request encryption, cookie requirement, response status, or
response schema to report.

## 2. Sanitized request/response schemas

**NOT RUN.** No request or response was captured.

The historical `/weapi/v1/radio/get` path remains unconfirmed and is not used
as runtime evidence in this report.

## 3. Private FM progression

**NOT RUN.** There are no T0, +1m, +10m, or +1h observations, song-ID
sequences, recommendation reasons, batch sizes, timestamps, or session
identities.

The no-playback and legitimate-progression comparison was also not run.

## 4. Session persistence

**ATTEMPTED, NOT PASSED.** The ncm-spike generated a QR locally and waited for
the official-client confirmation. The attempt expired before the service
returned a successful login code, so no session file was written and process
restart restoration remains unverified.

## 5. Playability findings

**NOT RUN.** No real response fixtures were obtained and no audio URL or
restricted audio endpoint was called. No `Playability` classification has been
implemented or inferred.

The required fail-closed enum remains the experiment contract:

```text
PlayableWithoutVip
VipRequired
PurchaseRequired
Unavailable
Unknown
```

## 6. Playlist-write findings

**NOT RUN.** No user identity, playlist listing, `FreeFM · Dev Test` creation,
track append, or re-read verification occurred.

## 7. Binary/RSS/startup baselines

The throwaway projects were created after the Cargo gate passed:

- `baseline-std`
- `baseline-netease`
- `ncm-spike`

`baseline-std` and `baseline-netease` both built successfully in release mode.
`baseline-netease` only calls `NeteaseMusicClient::new()` and exits; it makes
no NetEase request.

Measured on the local arm64 macOS host with `/usr/bin/time -l`:

| Binary | Release bytes | Stripped bytes | 5-run startup display | Peak RSS |
|---|---:|---:|---:|---:|
| baseline-std | 430,864 | 341,544 | 0.00 s each | 1,867,776 bytes |
| baseline-netease | 4,091,872 | 3,160,312 | 0.00 s each | 3,129,344 bytes |

The displayed startup value is below `/usr/bin/time`'s two-decimal
resolution, not a claim of zero CPU or wall time. Stripped RSS was the same
for baseline-netease; baseline-std had four samples at 1,867,776 bytes and one
at 1,884,160 bytes.

Approximate total linked cost relative to baseline-std:

- release: `+3,661,008` bytes
- stripped: `+2,818,768` bytes
- observed peak RSS: `+1,261,568` bytes

These are whole-program baseline differences, not per-dependency attribution.

The verified `netease-music 0.1.1` direct dependency set includes AES/CBC/ECB,
base64, bytes, flate2, md5, num-bigint, rand, reqwest 0.12 with
`blocking + gzip + json + rustls-tls`, serde/serde_json, thiserror, url, and
zstd. The resolved build includes Tokio 1.53.1, rustls, hyper, and
async-compression among notable transitive components. This confirms the
blocking API is not Tokio-free; no optimization decision is made yet.

## 8. Source-level finding and blockers

The local `reqwest 0.12.28` source confirms the Phase 0 correction:

- `src/blocking/client.rs:1365-1367` creates a thread named
  `reqwest-internal-sync-runtime`.
- `src/blocking/client.rs:1368-1372` constructs a Tokio current-thread runtime.
- `src/blocking/client.rs:1400-1403` receives requests and calls
  `tokio::spawn`.
- `Cargo.toml` enables Tokio-related blocking features and declares Tokio for
  non-WASM targets.

Historical/environment conditions:

1. The initial sandbox had DNS/network access disabled; approved network
   execution is now available for build commands.
2. The checkout began empty; this was expected and the experiment structure is
   now present.
3. No real authenticated NetEase account/session was available to this run;
   the intended flow is QR login by the user, with credentials kept only in
   the local experiment directory. No cookie/session was requested or printed.

The current remaining blocker is user-interactive QR authentication, not Cargo
or Rust networking.

No unlocking, bypass, audio-source replacement, or restricted audio endpoint
was used.

## 9. Recommendation

**IN PROGRESS / PRODUCT_UNVERIFIED.** Network/build prerequisites and both
binary baselines are complete. Proceed next with QR login and process-restart
session restoration, then Private FM behavior. This report still makes no
product-level Go/No-Go conclusion.
