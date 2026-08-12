---
name: freefm
description: Set up and operate the FreeFM Rust app for NetEase Private FM, including its TUI, QR login, read-only preview, append-only sync, diagnostics, and zero-LLM scheduling.
license: MIT
metadata:
  openclaw:
    emoji: "🎵"
    homepage: https://github.com/Yuxin-Qiao/FreeFM
    os: [darwin, linux]
  hermes:
    tags: [music, netease, private-fm, cli, automation, zero-llm]
    category: media
    requires_toolsets: [terminal]
---

# FreeFM

Operate FreeFM as a one-shot native CLI. Use an Agent only for interactive
setup or troubleshooting; scheduled synchronization must execute the binary
directly without a model turn.

## Guardrails

- Work only with the user's own NetEase Cloud Music account.
- Never ask the user to paste `MUSIC_U`, cookies, session files, or QR keys.
- Never print, inspect, summarize, upload, or commit credentials or playback
  URLs.
- Never unlock restricted content, replace audio URLs, download audio, or
  automatically substitute a searched recording.
- `freefm review` is the only way a free-version candidate becomes trusted: an
  explicit user `y` in the interactive prompt. Never infer a mapping yourself.
- Prefer `freefm audit --quiet` for routine checks: it is read-only and exits 3
  when a saved track needs attention. Never delete or repair saved tracks.
- Run `preview` before the first `sync`. Only `sync` may write remotely.
- Do not schedule until one manual preview and one manual sync have succeeded.

External playlist import is supported through `--source` for Spotify, Apple
Music, and YouTube Music URLs:

```sh
freefm preview --source 'https://open.spotify.com/playlist/<id>'
freefm review --source 'https://open.spotify.com/playlist/<id>'
freefm sync --source 'https://open.spotify.com/playlist/<id>'
freefm sync --source 'https://open.spotify.com/playlist/<source-id>' \
  --target 'https://open.spotify.com/playlist/<target-id>'
freefm review --source 'https://open.spotify.com/playlist/<source-id>' \
  --target 'https://music.youtube.com/playlist?list=<target-id>'
freefm tui --source 'https://open.spotify.com/playlist/<id>'
freefm doctor --json --source 'https://open.spotify.com/playlist/<id>'
# Read-only target credential/scope check; no provider request or write
freefm doctor --json --target 'https://music.youtube.com/playlist?list=<target-id>'
```

Set only the matching environment credential for the selected provider:
`FREEFM_SPOTIFY_TOKEN` (optionally `FREEFM_SPOTIFY_MARKET`),
`FREEFM_APPLE_MUSIC_DEVELOPER_TOKEN` (and
`FREEFM_APPLE_MUSIC_USER_TOKEN` for library playlists), or
`FREEFM_YOUTUBE_API_KEY` / `FREEFM_YOUTUBE_ACCESS_TOKEN`. These credentials are
read for one run and never persisted. External-to-NetEase tracks always require
explicit `review` before the ordinary source `sync`; source metadata never
unlocks, downloads, or replaces audio. Same-provider `--target` sync uses only
the stable source item id.

`sync --source --target` is the only external write path. It supports
same-provider stable-id copies to owned Spotify playlists, Apple Music library
playlists, and YouTube playlists. It verifies target ownership, rereads current
items, deduplicates, and appends only. Apple targets require a Music User Token;
YouTube targets require OAuth. Cross-provider transfers require explicit
per-track confirmation through `review --source --target`; search results are
never written directly. Until every source track has a valid mapping,
`sync` returns `target_mapping_required` and writes nothing.

FreeFM does not implement provider OAuth login or refresh-token storage. Obtain
provider tokens through the official provider flow, export them for one run,
and use `doctor --target` to inspect redacted credential booleans and required
scopes. Target writes reread ownership and items after every append batch; an
unconfirmed reread returns `target_write_uncertain` and never auto-retries.

## Locate or install the CLI

Prefer an existing `freefm` on `PATH`, then `$HOME/.local/bin/freefm`. If it is
missing, tell the user to install the public alpha from source:

```sh
cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked --root "$HOME/.local"
```

Do not run an installer without explicit user approval. FreeFM supports macOS
and Linux only.

## Interactive workflow

Run authentication in a terminal visible to the user so they can scan the QR
code with the official NetEase Cloud Music client:

```sh
freefm tui
freefm auth
freefm status --json
freefm preview --json
freefm audit --json
freefm review
freefm sync
freefm sync --quiet
```

Use `freefm tui` for guided interactive setup. It is only a front end for the
commands below. Never use the TUI in a scheduler.

Confirm `authenticated=true` and `account_vip_type=0` before sync. Treat
`login_required`, `ordinary_account_required`, `api_incompatible`, ambiguous
playlist ownership, unknown availability, and any non-zero exit as a manual
review condition. Never infer success from an empty failed run.

## OpenClaw deterministic automation

Resolve the absolute binary path first. Create an operator-admin command job
with exact argv, no delivery, and no Agent/model payload:

```sh
openclaw automations add --every 6h \
  --name freefm-sync \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver \
  --timeout-seconds 120
```

Use `openclaw automations list` to obtain the job ID, then verify one run with
`openclaw automations run <job-id> --wait`. Command payloads execute inside the
Gateway scheduler without starting a model-backed turn. Do not replace
`--command-argv` with an agent message.

Long-term health needs a second, low-frequency job that runs the read-only
audit so nobody has to remember it manually:

```sh
openclaw automations add --every 24h \
  --name freefm-audit \
  --command-argv '["/absolute/path/to/freefm","audit","--quiet"]' \
  --no-deliver \
  --timeout-seconds 120
```

`freefm audit --quiet` exits 0 silently when every saved track is still free
and exits 3 with a structured report when attention is needed; the scheduler
surfaces the non-zero exit without starting a model turn.

## Hermes no-agent automation

Install the bundled fixed-command helper, then create a script-only cron:

```sh
install -d -m 700 "$HOME/.hermes/scripts"
install -m 755 "{baseDir}/scripts/freefm-sync.sh" "$HOME/.hermes/scripts/freefm-sync.sh"
```

Hermes 0.17 may install only `SKILL.md` from a community GitHub/skills.sh
source. If `{baseDir}/scripts/freefm-sync.sh` is absent, ask for approval and
fetch the helper from the immutable source commit, then verify it before use:

```sh
helper=$(mktemp)
curl -fsSL \
  https://raw.githubusercontent.com/Yuxin-Qiao/FreeFM/c7bcf10dce142fd85c84f82173a307e91ea99adc/skills/freefm/scripts/freefm-sync.sh \
  -o "$helper"
test "$(shasum -a 256 "$helper" | awk '{print $1}')" = \
  "b9dd3bd85e32c8ce57ba11ef474149839ad898090495daf7336d396d37830fd1"
install -d -m 700 "$HOME/.hermes/scripts"
install -m 755 "$helper" "$HOME/.hermes/scripts/freefm-sync.sh"
rm -f "$helper"
```

Then create the job:

```sh
hermes cron create "0 */6 * * *" \
  --name freefm-sync \
  --script freefm-sync.sh \
  --no-agent
```

Use `hermes cron list` to obtain the job ID and `hermes cron run <job-id>` for
one manual verification. `--no-agent` makes the script the job; empty stdout on
success is silent and consumes no LLM tokens.

Pair a daily read-only audit job so health regressions surface without manual
effort:

```sh
install -m 755 "{baseDir}/scripts/freefm-audit.sh" "$HOME/.hermes/scripts/freefm-audit.sh"
hermes cron create "0 3 * * *" \
  --name freefm-audit \
  --script freefm-audit.sh \
  --no-agent
```

## Tencent WorkBuddy local skill

When this directory is imported as a local WorkBuddy skill, use the same
interactive workflow and guardrails above. Confirm the local terminal command
capability can resolve `freefm`, then run `status --json` and `preview --json`.
Do not run `auth` in a hidden terminal: the user must see and scan the QR code.
Do not create an unattended task until one user-confirmed `sync` succeeds, and
keep its payload fixed to `freefm sync --quiet` without an Agent turn.

## Codex skill usage

This folder is directly usable as a Codex skill: it installs as
`~/.codex/skills/freefm` and is discovered through the `name`/`description`
frontmatter above. One-time install:

```sh
install -d -m 700 "$HOME/.codex/skills"
cp -R "{baseDir}/skills/freefm" "$HOME/.codex/skills/freefm"
```

Alternatively, ask the Codex skill installer to fetch the `skills/freefm` path
from `Yuxin-Qiao/FreeFM`. Restart Codex after installing.

Scheduling honesty: `codex exec` and desktop recurring automations start an
Agent turn and consume LLM tokens; use them for interactive setup and
troubleshooting only, never for routine sync. Zero-token cycles must execute
the binary directly: the OS scheduler (cron/launchd) calling
`freefm sync --quiet`, or, where the deterministic sandbox runner is available,
a fixed command such as `codex sandbox -- freefm sync --quiet`. The sandbox
path needs a permissions profile that allows network access and writes to
`~/.freefm`; verify one `status --json` through the same profile before
enabling it.

## Verification

- `freefm status --json` reports an authenticated ordinary account.
- `freefm preview --json` reports decisions without creating or appending.
- `freefm audit --quiet` exits 0 with no output when every saved track is still
  free, and exits 3 with a structured report when attention is needed.
- A repeated `freefm sync --quiet` exits zero with empty stdout and stderr.
- The scheduler history reports success and no Agent/model invocation.
- FreeFM is absent from the process list between scheduled runs.

For user-facing installation, recovery, removal, and troubleshooting steps,
refer to https://github.com/Yuxin-Qiao/FreeFM/blob/main/README.zh-CN.md.
