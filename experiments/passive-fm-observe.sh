#!/bin/sh
set -eu

binary=${1:-target/release/freefm}
data_dir=${2:-.freefm}
output=${3:-/private/tmp/freefm-passive-fm.jsonl}
salt_file="${output}.salt"

umask 077
mkdir -p "$(dirname "$output")"
if [ ! -s "$salt_file" ]; then
  openssl rand -hex 32 >"$salt_file"
fi

raw=$(mktemp "${TMPDIR:-/tmp}/freefm-passive.XXXXXX")
hashes=$(mktemp "${TMPDIR:-/tmp}/freefm-hashes.XXXXXX")
trap 'rm -f "$raw" "$hashes"' EXIT HUP INT TERM

"$binary" preview --data-dir "$data_dir" --json >"$raw"
salt=$(tr -d '\n' <"$salt_file")

jq -r '.decisions[].original_id' "$raw" | LC_ALL=C sort -u | while IFS= read -r id; do
  printf '%s:%s' "$salt" "$id" | shasum -a 256 | awk '{print $1}'
done >"$hashes"

track_hashes=$(jq -R -s 'split("\n") | map(select(length > 0))' "$hashes")
batch_hash=$(printf '%s' "$track_hashes" | shasum -a 256 | awk '{print $1}')
action_counts=$(jq -c '.decisions | group_by(.action) | map({action: .[0].action, count: length})' "$raw")
availability_counts=$(jq -c '.decisions | group_by(.availability) | map({availability: .[0].availability, count: length})' "$raw")

jq -cn \
  --arg observed_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg batch_hash "$batch_hash" \
  --argjson track_hashes "$track_hashes" \
  --argjson batch_size "$(jq '.private_fm_count' "$raw")" \
  --argjson client_calls "$(jq '.client_calls' "$raw")" \
  --argjson http_requests "$(jq '.http_requests' "$raw")" \
  --argjson action_counts "$action_counts" \
  --argjson availability_counts "$availability_counts" \
  '{observed_at: $observed_at, batch_hash: $batch_hash, track_hashes: $track_hashes, batch_size: $batch_size, client_calls: $client_calls, http_requests: $http_requests, action_counts: $action_counts, availability_counts: $availability_counts}' \
  >>"$output"

chmod 600 "$output" "$salt_file"
