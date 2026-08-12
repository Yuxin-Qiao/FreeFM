<div align="center">

# 🎧 FreeFM

**Private FM in. A clean, free-playable playlist out.**

Native Rust CLI/TUI · Safely sync free-playable NetEase Private FM tracks into an append-only playlist

[![CI](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml/badge.svg)](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-native-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-1E6FFF)](#install)
[![License](https://img.shields.io/badge/license-MIT-6E56CF)](LICENSE)
[![ClawHub](https://img.shields.io/badge/ClawHub-FreeFM-13B8A6)](https://clawhub.ai/yuxin-qiao/skills/freefm)
[![skills.sh](https://skills.sh/b/yuxin-qiao/freefm)](https://www.skills.sh/yuxin-qiao/freefm/freefm)
[![No LLM](https://img.shields.io/badge/scheduled%20sync-0%20LLM%20tokens-22C55E)](#agent-platforms)

[简体中文](README.zh-CN.md) · [Quick start](#quick-start) · [Ask an AI](#ask-an-ai-to-install-it) ·
[TUI](#tui) · [Agent platforms](#agent-platforms) · [Validation](V01-VALIDATION.md)

</div>

![FreeFM — Private FM in, a clean playlist out](assets/freefm-hero.svg)

> [!IMPORTANT]
> Experimental community project, not an official NetEase Cloud Music product.
> Never unlocks restricted audio, replaces playback URLs, or downloads music.
> Use it only with your own account; the service API may change without notice.

## What it does

FreeFM reads NetEase Private FM and append-only maintains your owned
`FreeFM · Auto` playlist — only original tracks with **consistent positive
evidence** that an ordinary account can play them free, in full:

- **Strict proof**: missing, malformed, or contradictory entitlement data is
  skipped. Similar free releases are never auto-swapped: `preview` shows them
  as candidates, and `freefm review` lets you approve a trusted mapping once.
- **Long-lived**: `freefm audit` re-checks every saved track with the same
  strict logic (`still_free` / `became_restricted` / `unavailable` / `unknown`)
  and never modifies the playlist. v0.1 never auto-repairs.
- **Append-only**: `preview` is read-only; only `sync` writes. Repeated and
  concurrent runs dedupe by remote ID — no deletes, no reorders, no touching
  tracks you added by hand.
- **Zero resident cost**: one-shot CLI, exits after each run. No daemon, no
  database, no web server, no LLM.

## Three Core Principles

1. **No VIP Unlocking**: Never unlocks VIP/restricted audio, replaces playback URLs, bypasses DRM, or downloads media.
2. **No Resident Daemon**: One-shot CLI that exits after each run. Zero background processes, zero RAM when idle.
3. **No LLM During Scheduled Sync**: Scheduled automation executes `freefm sync --quiet` deterministically with **zero LLM tokens**.

## Quick start

### 1. Install precompiled binary (macOS / Linux)

```sh
curl -fsSL https://raw.githubusercontent.com/Yuxin-Qiao/FreeFM/main/scripts/install.sh | sh
```

*(Alternatively for Rust developers: `cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked`)*

### 2. Run FreeFM

```sh
freefm auth          # Scan QR code with official NetEase client
freefm preview       # Read-only preview: show additions, candidates, skips
freefm preview --max-additions 10 # Optional per-run preview/write budget
freefm audit         # Read-only: re-check saved tracks still play free (exit 3 = attention)
freefm review        # Interactive: approve a free-version candidate once (local only)
freefm sync          # Append-only remote write to "FreeFM · Auto" playlist
freefm sync --max-additions 10 # Optional hard cap; default remains unlimited
freefm sync --quiet  # Scheduler path; silent on success

# Optional guided terminal UI:
freefm tui
```

Credentials stay local under `~/.freefm/` — never logged, never uploaded. Never paste a
cookie, `MUSIC_U`, session, or QR key into an AI chat.

### Import Spotify, Apple Music, or YouTube Music playlists

`--source` reads playlist metadata through the official service APIs and keeps
the existing safety boundary: `preview` is read-only, `review` is the only way
to approve a cross-service mapping for the NetEase destination, and ordinary
source-to-NetEase `sync` only appends targets that were already approved and
are currently proven free on NetEase. Same-provider `--target` sync uses the
source service's stable item ids instead of searching for a recording.

```sh
# Spotify Web API OAuth access token
export FREEFM_SPOTIFY_TOKEN='...'
# Optional ISO-3166-1 alpha-2 market for tokens without a user country
export FREEFM_SPOTIFY_MARKET='US'
freefm preview --source 'https://open.spotify.com/playlist/<id>'
freefm review  --source 'https://open.spotify.com/playlist/<id>'
freefm sync    --source 'https://open.spotify.com/playlist/<id>'

# Same-provider append-only copy; target ownership is verified first.
freefm sync --source 'https://open.spotify.com/playlist/<source-id>' \
  --target 'https://open.spotify.com/playlist/<target-id>'

# Apple Music catalog playlist: developer token
export FREEFM_APPLE_MUSIC_DEVELOPER_TOKEN='...'
freefm preview --source 'https://music.apple.com/us/playlist/<name>/pl.<id>'

# Apple Music library playlist: developer token + Music User Token
export FREEFM_APPLE_MUSIC_USER_TOKEN='...'
freefm preview --source 'https://music.apple.com/us/library/playlist/<id>'

# Public YouTube / YouTube Music playlist: Data API key
export FREEFM_YOUTUBE_API_KEY='...'
freefm preview --source 'https://music.youtube.com/playlist?list=<id>'
# Private playlists may use an OAuth access token instead of the API key:
export FREEFM_YOUTUBE_ACCESS_TOKEN='...'

# Apple targets must be library playlists; YouTube targets require OAuth.
freefm sync --source 'https://music.apple.com/us/playlist/<name>/pl.<source-id>' \
  --target 'https://music.apple.com/us/library/playlist/<target-id>'
freefm sync --source 'https://music.youtube.com/playlist?list=<source-id>' \
  --target 'https://music.youtube.com/playlist?list=<target-id>'

# Cross-provider transfer: search candidates, choose and confirm each mapping,
# then run the same sync command. review never writes a remote playlist.
freefm review --source 'https://open.spotify.com/playlist/<source-id>' \
  --target 'https://music.youtube.com/playlist?list=<target-id>'

# The same source can be passed through the TUI; the selected action keeps it.
freefm tui --source 'https://open.spotify.com/playlist/<id>'

# Read-only check of URL parsing and required environment variables
freefm doctor --json --source 'https://open.spotify.com/playlist/<id>'
# Read-only target check: credentials/scopes only; no provider request or write
freefm doctor --json --target 'https://music.youtube.com/playlist?list=<target-id>'
```

External service credentials are read from the environment for one run and
are never stored in `~/.freefm/`. Local files contain only explicit trusted
mappings and NetEase sync state. Unsupported items, unavailable videos, and incomplete
metadata are counted and skipped; FreeFM never downloads or substitutes media.
`sync --source --target` copies only stable item ids within the same provider,
reads the target first, deduplicates by remote id, and appends in bounded
batches. Cross-provider transfers require `review --source --target`: search
results are candidates only, and each mapping is persisted only after explicit
confirmation. `sync` fails closed with `target_mapping_required` until every
source track has a confirmed mapping; it never silently substitutes a recording.

FreeFM does not implement provider OAuth login or refresh-token storage. This
is deliberate: provider app registration, redirect handling, and refresh
credentials are provider-specific and would enlarge the local secret surface.
Obtain a short-lived token through the provider's own OAuth flow, export it
for the one command, and use `doctor --target` to inspect the required
permission contract. A missing or expired token fails closed before a write.
The target write path rereads ownership and items after every append batch; if
the reread cannot confirm the append, it returns `target_write_uncertain` and
does not automatically retry.

### Ask an AI to install it

```text
Install FreeFM from https://github.com/Yuxin-Qiao/FreeFM on this macOS or Linux
machine. Read AGENTS.md and README.zh-CN.md first. Never ask for, inspect, print,
or upload NetEase cookies, MUSIC_U, sessions, or QR keys. Install using
`curl -fsSL https://raw.githubusercontent.com/Yuxin-Qiao/FreeFM/main/scripts/install.sh | sh`.
Let me run `freefm auth` in a visible terminal and scan the QR myself. Run `preview` first;
ask before `sync` or scheduler changes. Scheduled sync must execute
`freefm sync --quiet` directly without an Agent/LLM turn. On permission, login,
or API errors, give redacted guidance only; never change DNS, VPN, or proxies.
```

## TUI

`freefm tui` is a native Rust menu for auth, preview, review, audit, sync, status,
doctor, and **settings**. Review and audit leave the menu and invoke the normal
CLI implementations; both are read-only with respect to the remote playlist.
Use arrows or `j`/`k`, `o` toggles JSON output, `q` exits; the settings page toggles quiet mode (`u`) for scheduler-friendly output. Sync
requires an explicit `y`; Enter cancels. Automation must use the non-interactive
CLI, never the TUI.

## Commands

| Command | Remote write | Purpose |
|---|---:|---|
| `freefm auth` | No | Official-client QR login |
| `freefm preview` | No | Show additions, candidates, skips |
| `freefm audit` | No | Re-check saved tracks: still_free / became_restricted / unavailable / unknown |
| `freefm review` | Local only | Choose from up to three strict candidates and approve one; never writes remotely |
| `freefm sync` | Append only | Add strictly verified free originals |
| `freefm review --source ... --target ...` | Local only | Search candidates and explicitly confirm cross-provider mappings |
| `freefm sync --source ... --target ...` | Append only | Copy stable ids or previously reviewed mappings after target ownership verification |
| `freefm status` | No | Check session/account plus local sync metadata |
| `freefm doctor` | No | Check permissions, state, and API shape |
| `freefm tui` | Selected action | Guided terminal interface |

`--json` for stable machine output, `--quiet` for silent success; `--data-dir
PATH` or `FREEFM_HOME` isolates the state root.

### JSON Contract v1

Machine-readable success responses include `schema_version: 1`, `ok: true`, and
`command`. Errors include the same schema version plus a stable `error.kind`.
Existing fields are preserved; additive fields may appear, and clients should
ignore unknown fields. Sync keeps `would_add_ids` for compatibility and adds
`added_ids` / `added_count` only after remote reread verification.

## Agent platforms

The Rust binary is the product; platform integrations only install and invoke
its deterministic command. Routine scheduled sync uses **zero LLM tokens**.

Public skill listings: [ClawHub](https://clawhub.ai/yuxin-qiao/skills/freefm) ·
[skills.sh](https://www.skills.sh/yuxin-qiao/freefm/freefm)

| Logo | Platform | Installation | Scheduled Path (0 LLM) |
| :---: | :--- | :--- | :--- |
| <img src="assets/platforms/openclaw.svg" width="32" height="32" alt="OpenClaw"> | **OpenClaw** | `openclaw skills install @yuxin-qiao/freefm` | Gateway `--command-argv` |
| <img src="assets/platforms/hermes.png" width="32" height="32" alt="Hermes"> | **Hermes** | `hermes skills install Yuxin-Qiao/FreeFM/skills/freefm` | Script `--no-agent` |
| <img src="assets/platforms/workbuddy.png" width="32" height="32" alt="WorkBuddy"> | **WorkBuddy** | Upload `freefm-workbuddy.zip` | Local command capability |
| <img src="assets/platforms/codex.svg" width="32" height="32" alt="Codex"> | **Codex** | Copy `skills/freefm` to `~/.codex/skills/freefm` | OS cron / `codex sandbox` |

OpenClaw example:

```sh
openclaw automations add --every 6h --name freefm-sync \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver --timeout-seconds 120
```

The conservative default is every 6 hours so the playlist grows slowly;
FreeFM itself has no built-in scheduler, so you may run it more or less often
as you prefer.

Pair the sync job with a low-frequency read-only audit so nobody has to
remember to run it manually:

```sh
openclaw automations add --every 24h --name freefm-audit \
  --command-argv '["/absolute/path/to/freefm","audit","--quiet"]' \
  --no-deliver --timeout-seconds 120
```

`audit --quiet` is silent on success and exits 3 with a structured report when
any saved track needs attention (became restricted, unavailable, or unknown).

Build the WorkBuddy package with `scripts/package-workbuddy.sh`; import the ZIP
under the Tencent WorkBuddy skill uploader. Do not enable unattended sync before
the live-validation gate completes.

Codex: the `skills/freefm` folder is itself a Codex skill (`name`/`description`
frontmatter); install it and restart Codex. `codex exec` and desktop recurring
automations start an Agent turn and consume tokens — use them for setup and
troubleshooting only. Zero-token cycles keep running the binary directly via
the OS scheduler (`freefm sync --quiet`) or the deterministic sandbox runner
`codex sandbox -- freefm sync --quiet` (permissions profile must allow network
and `~/.freefm` writes). See
[`automation/codex/README.md`](automation/codex/README.md).

Platform logos are shown nominatively to identify the corresponding products;
they remain trademarks of their respective owners and imply no affiliation or
endorsement.

## Safety

FreeFM requires explicit `vipType == 0` and consistent privilege/player evidence:
numeric fee zero, usable playback capability, a non-empty in-memory URL, and no
trial marker. The URL is never printed, persisted, downloaded, or substituted.
Ambiguous playlist targets fail closed; a local lock prevents concurrent
creation and appends. Error codes: `login_required` → re-run `auth`;
`ordinary_account_required` → account does not qualify;
`api_incompatible` → API changed, pause scheduling and file a redacted issue;
`concurrent_sync` → another run is in progress. `freefm audit` exits 0 when
every saved track is still free and 3 when any track needs attention; it never
deletes, replaces, or reorders anything.

## Validation and docs

Live endpoints, the passive-FM experiment, session restart, HTTP counts, binary
size / peak RSS, platform proofs, and remaining gates are in
[V01-VALIDATION.md](V01-VALIDATION.md). Contributor and security guidance:
[AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md),
[SECURITY.md](SECURITY.md).

<div align="center">

**Do less, but make every step explainable, reproducible, and safe to exit.**
· [MIT License](LICENSE)

</div>
