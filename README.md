<div align="center">

# 🎧 FreeFM

**Private FM in. A clean, free-playable playlist out.**

A small native Rust app for NetEase Cloud Music — QR login, strict
free-playability checks, append-only playlist sync, and zero background cost.

[![CI](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml/badge.svg)](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-native-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-222)](#install)
[![License](https://img.shields.io/badge/license-MIT-6E56CF)](LICENSE)
[![ClawHub](https://img.shields.io/badge/ClawHub-FreeFM-13B8A6)](https://clawhub.ai/yuxin-qiao/skills/freefm)

[简体中文](README.zh-CN.md) · [Install](#install) · [TUI](#terminal-interface) ·
[Automation](#agent-platforms) · [Security](SECURITY.md)

</div>

![FreeFM — Private FM in, a clean playlist out](assets/freefm-hero.svg)

> [!IMPORTANT]
> FreeFM is an experimental community project, not an official NetEase Cloud
> Music product. It never unlocks restricted audio, replaces playback URLs, or
> downloads music. Use it only with your own account.

## What it does

```text
Official QR login
       ↓
NetEase Private FM
       ↓
strict ordinary-account playability proof
       ↓
FreeFM · Auto  (append-only, owned playlist)
```

| Promise | Behaviour |
|---|---|
| Free means proven free | Missing or conflicting entitlement data is skipped. |
| No secret substitutions | Search candidates are preview-only in v0.1. |
| Safe writes | Only `sync` writes; it never deletes or reorders tracks. |
| Idempotent | Repeated and concurrent syncs do not duplicate tracks or playlists. |
| Zero idle cost | FreeFM is a one-shot binary, not a daemon. |
| Local credentials | The minimal session stays under `~/.freefm/` and is never logged. |

## Install

FreeFM currently supports macOS and Linux. Install the public alpha from
source:

```sh
cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked
freefm --version
```

Homebrew is intentionally not offered during the experimental alpha. A proper
[tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap) needs a stable
tagged release, checksums, and maintained bottles; a copied formula would
provide less safety than the command above. Tag builds already produce native
archives and SHA-256 files so a maintained tap can be added after the first
stable release.

## Three-minute start

```sh
# Optional guided menu
freefm tui

# Or use explicit commands
freefm auth
freefm preview
freefm sync
freefm sync --quiet
```

`auth` prints a QR code for the official NetEase client. Never paste a cookie,
`MUSIC_U`, QR key, or session into an AI chat.

## Let an AI help install it

Paste this prompt into a coding agent that can use your terminal:

```text
Install FreeFM from https://github.com/Yuxin-Qiao/FreeFM on this macOS or Linux
machine. Read AGENTS.md and README.zh-CN.md first. Do not ask me to paste any
NetEase cookie, MUSIC_U, session, or QR key. Do not inspect or print credentials.
Use cargo install --git ... --locked, then let me run `freefm auth` in a visible
terminal and scan the QR code myself. Run `freefm preview` before any sync. Do
not run `sync`, create a playlist, or install a scheduler without asking me.
For scheduled use, invoke `freefm sync --quiet` directly with no Agent/LLM turn.
```

`AGENTS.md` constrains AI contributors working inside this repository. The
prompt above is the end-user installation handoff; the two files serve
different purposes.

## Terminal interface

`freefm tui` is a lightweight native terminal menu built in Rust:

```text
FreeFM  Private FM → free playlist

› QR login
  Preview recommendations       read-only
  Sync to FreeFM · Auto         explicit y required
  Check status
  Run doctor
```

Arrow keys or `j`/`k` navigate, `o` toggles human/JSON output, and `q` exits.
Remote sync requires an explicit `y`; Enter cancels at the confirmation step.
The TUI delegates to the same command implementation; it does not weaken any
safety boundary. Automation must continue to use `freefm sync --quiet`.

## Commands

| Command | Remote write | Purpose |
|---|---:|---|
| `freefm auth` | No playlist write | Interactive QR login |
| `freefm preview` | No | Show original additions, candidates, and skips |
| `freefm sync` | Append only | Add strictly verified free originals |
| `freefm status` | No | Check local session and account status |
| `freefm doctor` | No | Check permissions, state, login, and API shape |
| `freefm tui` | Depends on selection | Guided front end for the commands above |

Add `--json` for stable machine output or `--quiet` for silent successful
automation. `--data-dir PATH` and `FREEFM_HOME` provide isolated state roots.

## Agent platforms

The Rust binary is the product. Platform skills only help install and schedule
that binary; routine sync never needs an LLM.

| Platform | Installation | Scheduled path | Status |
|---|---|---|---|
| OpenClaw | `openclaw skills install @yuxin-qiao/freefm` | deterministic `--command-argv` | Verified ready |
| Hermes | `hermes skills install Yuxin-Qiao/FreeFM/skills/freefm` | `--no-agent` script | Verified `SAFE / ALLOWED` |
| Tencent WorkBuddy | Upload generated `freefm-workbuddy.zip` | Use its local command capability | Experimental package compatibility |

Build the WorkBuddy import package locally:

```sh
scripts/package-workbuddy.sh
# target/freefm-workbuddy.zip
```

The ZIP contains only the standard `SKILL.md` and deterministic helper. Its
layout is tested in CI and follows WorkBuddy's documented
[local package upload](https://cloud.tencent.com/document/product/1831/134432),
but a Tencent WorkBuddy client import has not yet been recorded; do not
describe it as marketplace-published.

## Safety model

FreeFM requires an explicitly confirmed `vipType=0` account. A source track is
added only when its privilege fields and official player capability provide
consistent positive evidence: numeric fee zero, usable playback capability,
non-empty in-memory URL, and no free-trial marker. The URL is never printed,
persisted, downloaded, or used as a replacement source.

Search may surface a similar free release in `preview`, but title, artist,
duration, album, and version markers are not authoritative recording identity.
FreeFM therefore never auto-substitutes candidates in v0.1.

For the verified endpoints, passive-FM experiment, session restart result,
request count, binary size, and remaining external gates, read
[V01-VALIDATION.md](V01-VALIDATION.md).

## Development

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

CI uses fake transports and redacted fixtures only. Real-account requests are
forbidden in CI. See [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md),
and [SECURITY.md](SECURITY.md) before changing protocol or credential code.

---

<div align="center">

FreeFM is independent, open source, and released under the [MIT License](LICENSE).

</div>
