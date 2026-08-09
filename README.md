<div align="center">

# 🎧 FreeFM

**Private FM in. A clean, free-playable playlist out.**

Native Rust · official-client QR login · strict free-playability proof · append-only

[![CI](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml/badge.svg)](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-native-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-222)](#install)
[![License](https://img.shields.io/badge/license-MIT-6E56CF)](LICENSE)
[![ClawHub](https://img.shields.io/badge/ClawHub-FreeFM-13B8A6)](https://clawhub.ai/yuxin-qiao/skills/freefm)

[简体中文](README.zh-CN.md) · [Install](#install) · [TUI](#tui) ·
[Agent platforms](#agent-platforms) · [Validation](V01-VALIDATION.md)

</div>

![FreeFM — Private FM in, a clean playlist out](assets/freefm-hero.svg)

> [!IMPORTANT]
> FreeFM is an experimental community project, not an official NetEase Cloud
> Music product. It never unlocks restricted audio, replaces playback URLs, or
> downloads music. Use it only with your own account.

## Why FreeFM

FreeFM reads NetEase Private FM and append-only maintains your owned
`FreeFM · Auto` playlist. A track is added only when an ordinary account has
consistent positive evidence of full, free playback. Missing, malformed, or
conflicting entitlement data is skipped. Similar free releases are preview-only
in v0.1—Live, Remix, covers, edits, and re-recordings are never silently swapped.

`preview` is read-only; only `sync` writes. Repeated and concurrent runs are
ID-based and idempotent. FreeFM exits after each run, so idle cost is zero.

## Install

macOS and Linux currently install from source:

```sh
cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked
freefm --version
```

The Homebrew tap will be enabled with the first stable tagged release. Until
then, the command above is the reproducible public-alpha path.

## Start

```sh
freefm tui          # guided terminal UI

# or explicit commands
freefm auth         # scan in the official NetEase client
freefm preview      # guaranteed read-only
freefm sync         # append-only remote write
freefm sync --quiet # scheduler path; silent on success
```

Never paste a cookie, `MUSIC_U`, session, or QR key into an AI chat. Credentials
remain under `~/.freefm/`, are not logged, and music/audio is never cached.

### Ask an AI to install it

```text
Install FreeFM from https://github.com/Yuxin-Qiao/FreeFM on this macOS or Linux
machine. Read AGENTS.md and README.zh-CN.md first. Never ask for, inspect, print,
or upload NetEase cookies, MUSIC_U, sessions, or QR keys. Install with
`cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked`. Let me run
`freefm auth` in a visible terminal and scan the QR myself. Run `preview` first;
ask before `sync` or scheduler changes. Scheduled sync must execute
`freefm sync --quiet` directly without an Agent/LLM turn.
```

## TUI

`freefm tui` is a native Rust menu for auth, preview, sync, status, and doctor.
Use arrows or `j`/`k`, `o` for human/JSON output, and `q` to exit. Sync requires
an explicit `y`; Enter cancels. Automation must use the non-interactive CLI.

## Commands

| Command | Remote write | Purpose |
|---|---:|---|
| `freefm auth` | No playlist write | Official-client QR login |
| `freefm preview` | No | Show additions, candidates, and skips |
| `freefm sync` | Append only | Add strictly verified free originals |
| `freefm status` | No | Check local session and account |
| `freefm doctor` | No | Check permissions, state, login, and API shape |
| `freefm tui` | Selected action | Guided terminal interface |

Use `--json` for stable machine output, `--quiet` for silent success, and
`--data-dir PATH` or `FREEFM_HOME` for an isolated state root.

## Agent platforms

The Rust binary is the product. Platform integrations only install and invoke
its deterministic command; routine sync needs zero LLM tokens.

| Platform | Install | Model-free scheduled path | Evidence |
|---|---|---|---|
| 🦞 **OpenClaw** | `openclaw skills install @yuxin-qiao/freefm` | Gateway `--command-argv` | Isolated live run: exit 0, empty output |
| 🪽 **Hermes** | `hermes skills install Yuxin-Qiao/FreeFM/skills/freefm` | `--no-agent` script | Live run `SAFE / ALLOWED` |
| 🤖 **Tencent WorkBuddy** | Upload `freefm-workbuddy.zip` | Local command capability | Signed client installed; import validation pending login |

Build the WorkBuddy package with `scripts/package-workbuddy.sh`. It contains
only `freefm/SKILL.md` and the deterministic helper; see Tencent's
[local skill upload guide](https://cloud.tencent.com/document/product/1831/134432).

OpenClaw uses this command payload:

```sh
openclaw automations add --every 1h --name freefm-hourly \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver --timeout-seconds 120
```

Do not enable unattended sync until the live-validation gate is complete.

## Safety and evidence

FreeFM requires explicit `vipType=0` and consistent privilege/player evidence:
numeric fee zero, usable playback capability, a non-empty in-memory URL, and no
trial marker. The URL is never printed, persisted, downloaded, or substituted.

The exact endpoints, real HTTP count, session restart, passive-FM experiment,
binary size/RSS, platform proofs, and remaining gates are recorded in
[V01-VALIDATION.md](V01-VALIDATION.md). CI uses fake transports and redacted
fixtures only; real-account requests are forbidden.

## Development

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Read [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[SECURITY.md](SECURITY.md) before changing protocol or credential code.

<div align="center">

Independent open source under the [MIT License](LICENSE).

</div>
