#!/bin/sh
set -eu

CDPATH=''
export CDPATH
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
output=${1:-"$repo/target/freefm-workbuddy.zip"}
case "$output" in
  /*) ;;
  *) output="$PWD/$output" ;;
esac

stage=$(mktemp -d "${TMPDIR:-/tmp}/freefm-workbuddy.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM

mkdir -p "$stage/freefm/scripts" "$(dirname -- "$output")"
cp "$repo/skills/freefm/SKILL.md" "$stage/freefm/SKILL.md"
cp "$repo/skills/freefm/scripts/freefm-sync.sh" "$stage/freefm/scripts/freefm-sync.sh"
chmod 755 "$stage/freefm/scripts/freefm-sync.sh"

(cd "$stage" && zip -q -r "$output" freefm)
printf '%s\n' "$output"
