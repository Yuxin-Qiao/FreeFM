# Security policy

## Supported status

FreeFM is an experimental alpha. Security fixes are accepted against the
latest default branch; no stable compatibility or security-support window is
promised yet.

## Reporting

Use GitHub's private security-advisory feature when it is available for this
repository. Do not open a public issue containing a session, cookie, QR key,
account identifier, playlist identifier, playback URL, or raw API response.

If a public issue is sufficient, include only the FreeFM version, operating
system, safe error category, reproduction steps using fake data, and redacted
output.

## Local credential model

FreeFM stores the minimum observed session credential under `~/.freefm/`, with
directory mode `700` and file mode `600` on supported Unix systems. It never
prints that credential intentionally. Anyone who obtains the session file may
be able to act as the account until the server invalidates it; protect backups
and revoke the session from the official client if exposure is suspected.

External playlist credentials (`FREEFM_SPOTIFY_TOKEN`, Apple Music tokens, and
YouTube credentials) are environment-only inputs. FreeFM never persists or
prints them. External APIs are used for playlist metadata and, only through
`sync --source --target`, append-only same-provider playlist writes after
target ownership verification. Cross-provider candidates require explicit
per-track review; incomplete mappings fail closed before any remote write.
The approved external mappings contain source/target provider, playlist,
storefront, and item ids plus approval timestamps, never access tokens or
playback URLs. This context prevents a mapping approved for one target
playlist or Apple storefront from being reused for another.

FreeFM intentionally does not implement provider OAuth login or refresh-token
storage. Operators must obtain provider credentials through the official
provider flow and pass them for one run; `doctor --target` reports only
redacted configuration booleans and required scopes.
