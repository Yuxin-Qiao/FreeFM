# FreeFM

FreeFM is a small native Rust CLI for NetEase Cloud Music Private FM. It logs
in through an official-client QR code, samples Private FM, and append-only
maintains a playlist named `FreeFM · Auto`.

v0.1 targets macOS and Linux. It is a one-shot CLI, not a daemon: between runs
it has zero resident processes, RAM, or CPU use.

It never unlocks restricted audio, replaces audio URLs, downloads music, or
uses an external service. `preview` is read-only; only `sync` writes the
playlist. Repeated syncs are ID-based and do not delete, reorder, or modify
existing user tracks.
In v0.1, restricted-song free-version candidates are preview-only and are
never automatically substituted.

## Commands

```text
freefm auth
freefm preview [--json] [--quiet]
freefm sync [--json] [--quiet]
freefm status [--json] [--quiet]
freefm doctor [--json] [--quiet]
freefm --version
```

Normal scheduled success with `sync --quiet` produces no FreeFM output. Use
`--data-dir PATH` for an isolated local test; the default directory is
`~/.freefm/`. `FREEFM_HOME` is also supported. Session files are written with
mode `600`, the directory with mode `700`, and neither is tracked by Git.
Only the `MUSIC_U` cookie is persisted after authentication; QR images are
rendered in the terminal and are not written to disk.
After the first successful sync, the cached playlist ID is checked against the
remote playlist name and owner before use; a stale or renamed ID is never used
as a write target.
Every preview/sync run takes a local advisory lock, multiple owned playlists
with the same name fail closed, and sync requires an explicit `vipType=0`
ordinary account.

## Current verified protocol

The current live QR session was used to verify these endpoints and modes:

- QR key: `/api/login/qrcode/unikey` (`eapi`)
- QR polling: `/api/login/qrcode/client/login` (`eapi`)
- login status: `/api/w/nuser/account/get` (`weapi`)
- Private FM: `/api/v1/radio/get` (`weapi`)
- song details: `/api/v3/song/detail` (`weapi`)
- playback capability probe: `/api/song/enhance/player/url/v1` (`eapi`)
- search: `/api/cloudsearch/pc` (`eapi`)
- user playlists: `/api/user/playlist` (`weapi`)
- playlist detail/tracks: `/weapi/v3/playlist/detail` via `linuxapi`
- playlist create: `/api/playlist/create` (`weapi`)
- playlist append: `/api/playlist/manipulate/tracks` (`weapi`), with
  `pid`, `trackIds` (JSON array string), and `op=add`

The seemingly natural `/api/playlist/track/add` route was tested against the
live service and returned `401` / `无权限操作歌单` for the owned playlist. The
`manipulate/tracks` shape returned `200` and was verified by re-reading the
playlist, so FreeFM uses the latter.

## Free-playability policy

The live account used for verification reported `vipType=0`. FreeFM requires
an explicitly confirmed `vipType=0` account and all of the following before
automatically adding a source song:

- privilege fee is explicitly numeric zero;
- a negative privilege state is absent; when present, a positive `pl` is
  required;
- the official player-capability response explicitly reports numeric fee zero,
  has a URL, and has no free-trial info;
- active free-trial privileges are rejected.

Missing, malformed, or contradictory entitlement fields are classified as
unknown and skipped.

The URL is observed only as an in-memory capability signal. It is never
printed, persisted, used as a replacement source, or downloaded. Live
restricted responses showed non-zero privilege fees while still returning a
URL, proving that URL presence alone is not a free-playability test. A live
fee-zero original passed both entitlement/player probes and was appended and
re-read successfully; its catalog identifiers are intentionally not retained.

Restricted songs are searched by title so `preview` can display possible free
candidates. Even when title, artist list, duration, version markers, and free
capability match, v0.1 reports the candidate only and skips automatic
replacement; these metadata fields are not treated as proof of the same
recording. A live field-name probe of song detail and song-wiki responses found
no ISRC or equivalent authoritative recording identity.

## Private FM and session findings

Without playing a song and without calling any skip/trash endpoint, two live
calls 6 minutes 14 seconds apart returned distinct three-song batches. This
proves that short-term passive reads can change recommendations. It does not
yet prove +1 hour, +24 hour, or multi-day behavior, or whether the service
internally marks a fetched recommendation as consumed; FreeFM never simulates
playback or skip behavior.

The QR-created session was saved, the process exited, and a new process later
returned `authenticated=true` and `vipType=0`. Session restoration is therefore
working for this live session; expiry or server-side revocation still requires
running `auth` again.

## Development

```shell
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

For automation, invoke the installed binary directly (`freefm sync --quiet`)
so Cargo itself does not add build output to scheduler stdout. Install a local
copy with `cargo install --path .` when appropriate.

Fixtures under `fixtures/` contain only classifier fields and redacted URL
placeholders; no account data or audio response is persisted. No database,
daemon, scheduler, Node.js, Python, Docker, web server, or LLM is required.

The final local release proof produced a 1,802,256-byte arm64 binary. A real
append-and-confirm sync used 19 HTTP requests, took 4.02 seconds, and peaked at
15,269,888 bytes RSS. The immediately repeated `sync --quiet` produced no
stdout or stderr. See `V01-VALIDATION.md` for exact scope and remaining gates.
