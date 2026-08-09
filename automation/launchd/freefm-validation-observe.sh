#!/bin/sh
set -eu

umask 077

binary=${FREEFM_BINARY:-"$HOME/.local/bin/freefm"}
data_dir=${FREEFM_DATA_DIR:-"$HOME/.freefm"}
evidence_dir=${FREEFM_VALIDATION_DIR:-"$HOME/.freefm-validation"}
passive_helper=${FREEFM_PASSIVE_HELPER:-"$HOME/.local/libexec/freefm-passive-fm-observe.sh"}

mkdir -p "$evidence_dir"
chmod 700 "$evidence_dir"

lock_dir="$evidence_dir/observe.lock"
if ! mkdir "$lock_dir" 2>/dev/null; then
  exit 0
fi
trap 'rm -rf "$lock_dir"; rm -f "${status_raw:-}" "${error_raw:-}"' EXIT HUP INT TERM

started_file="$evidence_dir/started-at"
if [ ! -s "$started_file" ]; then
  date -u +%Y-%m-%dT%H:%M:%SZ >"$started_file"
  chmod 600 "$started_file"
fi

status_raw=$(mktemp "${TMPDIR:-/tmp}/freefm-validation-status.XXXXXX")
error_raw=$(mktemp "${TMPDIR:-/tmp}/freefm-validation-error.XXXXXX")
observed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)

if "$binary" status --data-dir "$data_dir" --json >"$status_raw" 2>"$error_raw"; then
  jq -c --arg observed_at "$observed_at" \
    '{observed_at: $observed_at, ok, authenticated, account_vip_type,
      session_present, login_required: (.login_required // false),
      client_calls: (.client_calls // 0), http_requests: (.http_requests // 0)}' \
    "$status_raw" >>"$evidence_dir/session.jsonl"
else
  jq -cn --arg observed_at "$observed_at" \
    '{observed_at: $observed_at, ok: false, failure: "status_failed"}' \
    >>"$evidence_dir/session.jsonl"
fi

if "$passive_helper" "$binary" "$data_dir" "$evidence_dir/passive.jsonl" \
  >/dev/null 2>"$error_raw"; then
  :
else
  jq -cn --arg observed_at "$observed_at" \
    '{observed_at: $observed_at, ok: false, failure: "preview_failed"}' \
    >>"$evidence_dir/failures.jsonl"
fi

chmod 600 "$evidence_dir"/* 2>/dev/null || true
