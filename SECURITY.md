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
