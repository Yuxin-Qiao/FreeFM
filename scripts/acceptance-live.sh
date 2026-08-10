#!/bin/sh
set -eu

# Live acceptance driver: runs the real-account main chain against a fresh,
# isolated data directory. Only aggregate counts and exit codes are recorded;
# no song IDs, titles, cookies, or responses are persisted.

umask 077

binary=${FREEFM_BINARY:-"$PWD/target/release/freefm"}
data_dir=${FREEFM_ACCEPTANCE_DIR:-"$HOME/.freefm-acceptance"}
evidence="$data_dir/acceptance.jsonl"

if [ ! -x "$binary" ]; then
  echo "release binary 不存在：$binary（先 cargo build --release --locked）" >&2
  exit 1
fi

mkdir -p "$data_dir"
chmod 700 "$data_dir"

record() {
  step=$1
  code=$2
  output=$3
  payload=$(printf '%s' "$output" | jq -c 'if type == "object" then
    {private_fm_count,
     would_add_count: (.would_add_ids | length? // 0),
     existing_track_count,
     playlist_exists,
     summary,
     needs_attention,
     authenticated,
     account_vip_type,
     client_calls,
     http_requests} else {} end' 2>/dev/null || printf '{}')
  if [ -z "$payload" ]; then
    payload='{}'
  fi
  jq -cn --arg step "$step" --argjson code "$code" --argjson payload "$payload" \
    '{accepted_at: (now | todate), step: $step, exit: $code, payload: $payload}' \
    >>"$evidence"
  chmod 600 "$evidence" 2>/dev/null || true
}

run() {
  label=$1
  shift
  interactive=0
  if [ "${1:-}" = "--interactive" ]; then
    interactive=1
    shift
  fi
  if [ "$interactive" = "1" ]; then
    set +e
    "$binary" "$@"
    code=$?
    set -e
    record "$label" "$code" ""
    echo "== $label (exit $code)"
    return
  fi
  set +e
  output=$("$binary" "$@" 2>&1)
  code=$?
  set -e
  echo "== $label (exit $code)"
  printf '%s\n' "$output" | head -8
  record "$label" "$code" "$output"
}

echo "=== FreeFM live acceptance (fresh data dir: $data_dir) ==="
echo "如果当前账号已有 FreeFM · Auto，脚本会验证 owner 后复用，不会新建第二张。"

if [ -s "$data_dir/session.json" ]; then
  echo "已存在会话，跳过 auth（如需重新登录请删除 $data_dir/session.json）。"
else
  run "auth" --interactive auth --data-dir "$data_dir"
fi

run "status-after-restart" status --json --data-dir "$data_dir"

run "preview-1" preview --json --data-dir "$data_dir"

run "sync-1" sync --json --data-dir "$data_dir"

run "preview-after-sync" preview --json --data-dir "$data_dir"

run "sync-2-idempotent" sync --json --data-dir "$data_dir"

run "audit" audit --json --data-dir "$data_dir"

run "audit-quiet" audit --quiet --data-dir "$data_dir"

echo
echo "完成。脱敏汇总记录在：$evidence"
echo "对照 checklist（FUNCTIONAL-ACCEPTANCE.md 的 BLOCKED_USER_ACTION 章节）逐项确认："
jq -r '"  " + .accepted_at + "  step=" + .step + "  exit=" + (.exit|tostring) + "  payload=" + (.payload|tostring)' "$evidence"
