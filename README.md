<div align="center">

# 🎧 FreeFM

**Private FM in. A clean, free-playable playlist out.**

Native Rust CLI/TUI · Safely sync free-playable NetEase Private FM tracks into an append-only playlist

[![CI](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml/badge.svg)](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-native-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-1E6FFF)](#install)
[![License](https://img.shields.io/badge/license-MIT-6E56CF)](LICENSE)
[![ClawHub](https://img.shields.io/badge/ClawHub-FreeFM-13B8A6)](https://clawhub.ai/yuxin-qiao/skills/freefm)
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
  skipped. Similar free releases are preview-only in v0.1; never auto-swapped.
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
freefm sync          # Append-only remote write to "FreeFM · Auto" playlist
freefm sync --quiet  # Scheduler path; silent on success

# Optional guided terminal UI:
freefm tui
```

Credentials stay local under `~/.freefm/` — never logged, never uploaded. Never paste a
cookie, `MUSIC_U`, session, or QR key into an AI chat.

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

`freefm tui` is a native Rust menu for auth, preview, sync, status, doctor, and
**settings**. Use arrows or `j`/`k`, `o` toggles JSON output, `q` exits; the
settings page toggles quiet mode (`u`) for scheduler-friendly output. Sync
requires an explicit `y`; Enter cancels. Automation must use the non-interactive
CLI, never the TUI.

## Commands

| Command | Remote write | Purpose |
|---|---:|---|
| `freefm auth` | No | Official-client QR login |
| `freefm preview` | No | Show additions, candidates, skips |
| `freefm sync` | Append only | Add strictly verified free originals |
| `freefm status` | No | Check local session and account |
| `freefm doctor` | No | Check permissions, state, and API shape |
| `freefm tui` | Selected action | Guided terminal interface |

`--json` for stable machine output, `--quiet` for silent success; `--data-dir
PATH` or `FREEFM_HOME` isolates the state root.

## Agent platforms

The Rust binary is the product; platform integrations only install and invoke
its deterministic command. Routine scheduled sync uses **zero LLM tokens**.

| Platform | Installation | Scheduled Path (0 LLM) | Validation Evidence |
|:---|:---|:---|:---|
| <img src="assets/platforms/openclaw.svg" width="18" height="18" align="center"> **OpenClaw** | `openclaw skills install` `@yuxin-qiao/freefm` | Gateway `--command-argv` | Isolated Gateway run (exit 0) |
| <img src="assets/platforms/hermes.png" width="18" height="18" align="center"> **Hermes** | `hermes skills install` `Yuxin-Qiao/FreeFM/skills/freefm` | Script `--no-agent` | Hermes scan `SAFE / ALLOWED` |
| <img src="assets/platforms/workbuddy.png" width="18" height="18" align="center"> **WorkBuddy** | Upload `freefm-workbuddy.zip` | Local command capability | Signed client & package verified |
| <img src="assets/platforms/codex.svg" width="18" height="18" align="center"> **Codex** | Copy `skills/freefm` to `~/.codex/skills/freefm` | OS cron / `codex sandbox` | Skill structure verified |

OpenClaw example:

```sh
openclaw automations add --every 1h --name freefm-hourly \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver --timeout-seconds 120
```

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
`concurrent_sync` → another run is in progress.

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
