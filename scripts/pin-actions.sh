#!/bin/sh
# Resolve every `uses: owner/repo@ref` in .github/workflows to a full commit
# SHA. Supply-chain hardening: refs like tags and moving branches are replaced
# by their current commit, so a compromised tag or branch cannot change what
# CI runs.
#
# Usage:
#   scripts/pin-actions.sh          # rewrite workflow files in place
#   scripts/pin-actions.sh --check  # report un-pinned refs; exit 1 if any
#
# Requires network access to api.github.com and python3. Run it on a machine
# with normal connectivity, then commit the rewritten workflows.
set -eu

check_only=0
case "${1:-}" in
    --check) check_only=1 ;;
    "") ;;
    *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

resolve_sha() {
    owner_repo=$1
    ref=$2
    # Tags and branches resolve through the refs API. Annotated tags return a
    # tag object, which must be followed to the underlying commit.
    json=$(curl -fsS --max-time 30 \
        "https://api.github.com/repos/${owner_repo}/git/ref/tags/${ref}" 2>/dev/null || \
        curl -fsS --max-time 30 \
        "https://api.github.com/repos/${owner_repo}/git/ref/heads/${ref}" 2>/dev/null || true)
    [ -n "$json" ] || return 0
    otype=$(printf '%s' "$json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("object",{}).get("type",""))')
    osha=$(printf '%s' "$json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("object",{}).get("sha",""))')
    if [ "$otype" = "tag" ] && [ -n "$osha" ]; then
        tag_json=$(curl -fsS --max-time 30 \
            "https://api.github.com/repos/${owner_repo}/git/tags/${osha}" 2>/dev/null || true)
        [ -n "$tag_json" ] || return 0
        osha=$(printf '%s' "$tag_json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("object",{}).get("sha",""))')
    fi
    printf '%s' "$osha"
}

rewrite_file() {
    file=$1
    tmp=${file}.pin-tmp
    : > "$tmp"
    changed=0
    unpinned=0
    unset IFS
    while IFS= read -r line; do
        case "$line" in
            *"uses: "*)
                spec=$(printf '%s' "$line" | sed -n 's/.*uses: \([A-Za-z0-9_.-]*\/[A-Za-z0-9_.-]*@[^ ]*\).*/\1/p')
                if [ -n "$spec" ]; then
                    owner_repo=${spec%@*}
                    ref=${spec#*@}
                    if printf '%s' "$ref" | grep -Eq '^[0-9a-f]{40}$'; then
                        # already pinned
                        printf '%s\n' "$line" >> "$tmp"
                        continue
                    fi
                    unpinned=1
                    sha=$(resolve_sha "$owner_repo" "$ref")
                    if [ -z "$sha" ]; then
                        echo "warning: could not resolve ${owner_repo}@${ref}" >&2
                        printf '%s\n' "$line" >> "$tmp"
                        continue
                    fi
                    new_line=$(printf '%s' "$line" | sed "s|@${ref}|@${sha}|")
                    printf '%s\n' "$new_line" >> "$tmp"
                    echo "${file}: ${owner_repo}@${ref} -> ${owner_repo}@${sha}"
                    changed=1
                    continue
                fi
                ;;
        esac
        printf '%s\n' "$line" >> "$tmp"
    done < "$file"
    if [ "$check_only" = 1 ]; then
        rm -f "$tmp"
        { [ "$changed" = 1 ] || [ "$unpinned" = 1 ]; } && return 1
        return 0
    fi
    if [ "$changed" = 1 ] && [ "$unpinned" = 0 ]; then
        mv "$tmp" "$file"
    else
        rm -f "$tmp"
    fi
}

exit_code=0
for file in .github/workflows/*.yml; do
    [ -e "$file" ] || continue
    if ! rewrite_file "$file"; then
        exit_code=1
    fi
done

if [ "$check_only" = 1 ]; then
    if [ "$exit_code" = 1 ]; then
        echo "un-pinned action refs found; run scripts/pin-actions.sh" >&2
    else
        echo "all workflow action refs are pinned to commit SHAs"
    fi
fi
exit "$exit_code"
