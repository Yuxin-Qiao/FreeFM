# FreeFM Phase 2 evidence

Date: 2026-08-09 (Asia/Shanghai)

This document records the live closed-loop proof without retaining account or
catalog identifiers. The current authoritative measurements are also collected
in `V01-VALIDATION.md`.

## Session and account

- QR login was completed with the official NetEase Cloud Music client.
- Credentials were never printed and are excluded from Git.
- A new process restored a session containing only `MUSIC_U` and reported an
  explicit ordinary-account `vipType=0`.
- The data directory was mode `700`; session and state files were mode `600`.

## Verified live protocol

- `/api/login/qrcode/unikey` (`eapi`): QR key.
- `/api/login/qrcode/client/login` (`eapi`): QR polling.
- `/api/w/nuser/account/get` (`weapi`): login/account status.
- `/api/v1/radio/get` (`weapi`): Private FM.
- `/api/v3/song/detail` (`weapi`): song metadata and top-level privileges.
- `/api/song/enhance/player/url/v1` (`eapi`): capability probe.
- `/api/cloudsearch/pc` (`eapi`): catalog search.
- `/api/user/playlist` (`weapi`): owned playlist discovery.
- `/weapi/v3/playlist/detail` through `linuxapi`: playlist detail/tracks.
- `/api/playlist/create` (`weapi`): playlist creation.
- `/api/playlist/manipulate/tracks` (`weapi`): append with `pid`, `trackIds`,
  and `op=add`.

The commonly cited `/api/playlist/track/add` shape returned 401 for a verified
owned playlist. The manipulate-tracks endpoint returned 200, and a subsequent
playlist read confirmed the append.

## Final live write

The frozen release binary performed one append-only sync against an owned
`FreeFM · Auto` playlist. It appended one strictly free original track, skipped
two restricted originals, added no searched replacement, and confirmed the
new track by re-reading the playlist. A second `sync --quiet` exited 0 with
empty stdout and stderr, confirming idempotent steady-state behavior.

Measured final run: 16 client calls, 19 HTTP requests, 4.02 seconds wall time,
15,269,888 bytes peak RSS, and a 1,802,256-byte release binary.

## Safety conclusions

- URL presence alone is not free-playability evidence; live restricted
  responses could still contain URLs.
- Free classification requires fee zero and positive, non-trial entitlement
  evidence from both privilege and capability responses.
- The live detail response's top-level privilege array must be joined by song
  ID; a regression test prevents entitlement loss during metadata enrichment.
- Search metadata exposes no authoritative same-recording identity in the
  tested responses, so free alternatives remain preview-only in v0.1.
- Short-term passive reads returned changing batches, but longer observation is
  still required before enabling an unattended scheduler.
