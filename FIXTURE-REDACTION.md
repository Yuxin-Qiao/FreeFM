# Fixture redaction rules

Fixtures are offline test inputs, not account backups. Keep only fields needed
to parse songs, classify ordinary-account playability, compare metadata, or
exercise playlist flow control.

Never commit cookies, `MUSIC_U`, QR keys, account/profile identifiers, playlist
identifiers from a real account, listening history, complete API bodies,
headers, audio URLs, download links, cover URLs, lyrics, or long-term logs.
Replace any retained URL with `REDACTED_NOT_STORED`. Real captures must be
reviewed manually, minimized, and dated in the evidence report before being
added to `fixtures/`.
