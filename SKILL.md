# FreeFM deterministic scheduling

FreeFM routine synchronization is a deterministic native command. Never start
an Agent or LLM for it:

```shell
$HOME/.local/bin/freefm sync --quiet
```

Normal success exits zero and writes nothing. Any non-zero exit or visible
output means login renewal, API incompatibility, ordinary-account verification,
lock contention, or manual review is needed. Do not invoke Cargo from a
scheduler, and do not schedule until `freefm preview --json` has been reviewed.

## Hermes

Hermes 0.17 expects scripts below `~/.hermes/scripts`. Install this repository's
`automation/hermes/freefm-sync.sh` there, then create a no-agent cron:

```shell
hermes cron create '0 * * * *' \
  --name freefm-hourly \
  --script freefm-sync.sh \
  --no-agent
```

The local verification job uses this exact mode. A manual trigger recorded
`no_agent (script)` and `silent (empty output)`, which means the run bypassed
the Agent/LLM and used zero LLM tokens. It remains paused until FreeFM's
long-duration passive-FM validation is complete.

## OpenClaw

Configure a deterministic command cron that executes the installed binary
directly:

```text
command: $HOME/.local/bin/freefm sync --quiet
schedule: 0 * * * *
agent: false
llm: false
```

OpenClaw was not installed on the validation host, so this is a host-neutral
configuration requirement rather than a completed OpenClaw execution proof.

FreeFM implements no scheduler itself and leaves no resident process between
runs. v0.1 never automatically substitutes a restricted song with a searched
candidate.
