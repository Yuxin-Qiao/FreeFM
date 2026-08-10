#!/bin/sh
set -eu

# Human-in-the-loop experiment: does a read-only Private FM fetch advance the
# queue the official NetEase client shows? This script only runs `preview`
# (read-only). It never calls playback, skip, trash, scrobble, or sync.

umask 077

binary=${FREEFM_BINARY:-"$HOME/.local/bin/freefm"}
if [ ! -x "$binary" ]; then
  binary=$(command -v freefm 2>/dev/null || true)
fi
if [ -z "$binary" ] || [ ! -x "$binary" ]; then
  echo "freefm 未找到；请先安装或设置 FREEFM_BINARY" >&2
  exit 127
fi

data_dir=${FREEFM_DATA_DIR:-"$HOME/.freefm"}
evidence_dir=${FREEFM_VALIDATION_DIR:-"$HOME/.freefm-validation"}
evidence="$evidence_dir/fmqueue-experiment.jsonl"
salt_file="$evidence.salt"
launch_agent="$HOME/Library/LaunchAgents/com.freefm.validation.plist"

mkdir -p "$evidence_dir"
chmod 700 "$evidence_dir"
if [ ! -s "$salt_file" ]; then
  openssl rand -hex 32 >"$salt_file"
fi

confirm() {
  printf '%s [y/N] ' "$1"
  read -r answer
  [ "$answer" = "y" ] || [ "$answer" = "Y" ]
}

# Pause the read-only validation observer so no fetch happens mid-experiment.
if [ -f "$launch_agent" ] && launchctl list 2>/dev/null | grep -q com.freefm.validation; then
  if confirm "暂停 com.freefm.validation 观察（实验期间不自动采样）？"; then
    launchctl unload "$launch_agent"
    echo "已暂停观察。"
  else
    echo "警告：观察任务仍在运行，实验期间可能插入额外 fetch，结果可能受干扰。" >&2
  fi
fi

echo
echo "步骤 1/3：打开网易云官方客户端 -> 私人FM。"
echo "          记下当前第一首歌的标题（不要播放、不要跳过、不要切歌）。"
echo "          完成后按回车继续。"
read -r _unused

raw=$(mktemp "${TMPDIR:-/tmp}/freefm-fmqueue.XXXXXX")
hashes=$(mktemp "${TMPDIR:-/tmp}/freefm-fmqueue-hashes.XXXXXX")
trap 'rm -f "$raw" "$hashes"' EXIT HUP INT TERM

if ! "$binary" preview --data-dir "$data_dir" --json >"$raw"; then
  echo "preview 失败（请先完成 freefm auth）；实验中止，未记录任何结果。" >&2
  exit 1
fi

salt=$(tr -d '\n' <"$salt_file")
jq -r '.decisions[].original_id' "$raw" | LC_ALL=C sort -u | while IFS= read -r id; do
  printf '%s:%s' "$salt" "$id" | shasum -a 256 | awk '{print $1}'
done >"$hashes"
track_hashes=$(jq -R -s 'split("\n") | map(select(length > 0))' "$hashes")
batch_hash=$(printf '%s' "$track_hashes" | shasum -a 256 | awk '{print $1}')

echo
echo "步骤 2/3：再次打开网易云官方客户端 -> 私人FM。"
echo "          对比刚才记下的第一首歌：队列被这次 fetch 推进了吗？"
answer=""
while [ -z "$answer" ]; do
  printf '回答 (y=推进了/第一首变了, n=完全没变, u=不确定): '
  read -r answer
  case "$answer" in
    y | n | u) ;;
    *) answer="" ;;
  esac
done

note=""
printf '可选备注（不包含歌曲名/ID，例如“顺序没变但位置前移”）: '
read -r note || true

jq -cn \
  --arg observed_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg batch_hash "$batch_hash" \
  --argjson track_hashes "$track_hashes" \
  --argjson batch_size "$(jq '.decisions | length' "$raw")" \
  --argjson client_calls "$(jq '.client_calls' "$raw")" \
  --argjson http_requests "$(jq '.http_requests' "$raw")" \
  --arg answer "$answer" \
  --arg note "$note" \
  '{observed_at: $observed_at, batch_hash: $batch_hash, track_hashes: $track_hashes,
    batch_size: $batch_size, client_calls: $client_calls, http_requests: $http_requests,
    official_queue_advanced: $answer, note: $note}' \
  >>"$evidence"
chmod 600 "$evidence" "$salt_file"

echo
echo "已记录（仅盐化哈希 + 你的回答，无歌曲名/ID）：$evidence"
echo "本次记录：$(jq -c '{batch_size, http_requests, official_queue_advanced, note}' "$evidence" | tail -1)"

if [ -f "$launch_agent" ] && launchctl list 2>/dev/null | grep -q com.freefm.validation; then
  :
elif [ -f "$launch_agent" ]; then
  if confirm "恢复 com.freefm.validation 观察？"; then
    launchctl load "$launch_agent"
    echo "已恢复观察。"
  else
    echo "观察保持暂停。"
  fi
fi

echo "实验完成。"
