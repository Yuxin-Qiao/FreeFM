#!/bin/sh
set -eu

# FreeFM v0.1.0 release closeout. Run this only AFTER the +7d passive-FM
# observation gate (2026-08-16 23:21 Asia/Shanghai) has passed.
# Every phase fails closed: stop, report, and let a human decide.

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
evidence_dir=${FREEFM_VALIDATION_DIR:-"$HOME/.freefm-validation"}
launch_agent="$HOME/Library/LaunchAgents/com.freefm.validation.plist"
version=${FREEFM_VERSION:-v0.1.0}
tap_dir=${FREEFM_TAP_DIR:-""}
hermes_job=${FREEFM_HERMES_JOB:-freefm-sync}

echo "=== FreeFM $version release closeout ==="

if ! command -v gh >/dev/null 2>&1; then
  echo "gh 未安装；请先安装并 gh auth login" >&2
  exit 1
fi

# 1. Time gate: +7d since the first passive-FM sample.
if [ ! -s "$evidence_dir/started-at" ]; then
  echo "缺少 $evidence_dir/started-at，无法验证时间门槛" >&2
  exit 1
fi
started_at=$(cat "$evidence_dir/started-at")
started_epoch=$(date -j -f "%Y-%m-%dT%H:%M:%SZ" "$started_at" +%s 2>/dev/null || date -u -d "$started_at" +%s)
gate_epoch=$((started_epoch + 7 * 24 * 3600))
now_epoch=$(date -u +%s)
if [ "$now_epoch" -lt "$gate_epoch" ]; then
  gate_display=$(date -u -r "$gate_epoch" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$gate_epoch" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo "$gate_epoch")
  echo "时间门槛未到：首样本 $started_at，+7d 为 $gate_display，当前 $(date -u +%Y-%m-%dT%H:%M:%SZ)" >&2
  exit 1
fi
echo "[1/8] +7d 门槛已通过（$started_at + 7d）。"

# 2. Stop and remove the read-only validation LaunchAgent.
if launchctl print "gui/$(id -u)/com.freefm.validation" >/dev/null 2>&1; then
  launchctl unload "$launch_agent"
fi
if [ -f "$launch_agent" ]; then
  rm -f "$launch_agent"
fi
if launchctl print "gui/$(id -u)/com.freefm.validation" >/dev/null 2>&1; then
  echo "LaunchAgent 仍在运行，请人工检查" >&2
  exit 1
fi
echo "[2/8] 观察 LaunchAgent 已停止并移除。"

cd "$repo_dir"

# 3. Local release gates.
echo "[3/8] 本地门禁..."
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
git diff --check
if command -v gitleaks >/dev/null 2>&1; then
  gitleaks dir . --no-banner --redact
fi
if cargo audit --no-fetch >/dev/null 2>&1; then
  :
else
  cargo audit
fi
echo "[3/8] 本地门禁通过。"

# 4. Main must be clean and green.
if [ -n "$(git status --porcelain)" ]; then
  echo "工作区不干净，先提交或还原再发布" >&2
  exit 1
fi
git fetch origin main
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
  echo "本地 main 与远端不一致" >&2
  exit 1
fi
echo "[4/8] main 干净且与远端一致。"

# 5. Tag and let the Release workflow build, attest, and publish.
if git rev-parse "$version" >/dev/null 2>&1; then
  echo "tag $version 已存在；如需重发请先删除" >&2
  exit 1
fi
git tag "$version"
git push origin "$version"
echo "[5/8] tag $version 已推送；等待 Release workflow（构建/校验/attest/SBOM）完成后继续。"

run_id=""
attempt=0
while [ "$attempt" -lt 60 ]; do
  attempt=$((attempt + 1))
  run_id=$(gh run list --workflow release.yml --branch "$version" --limit 1 --json databaseId,status --jq 'select(.[0].status == "completed") | .[0].databaseId // empty' 2>/dev/null || true)
  [ -n "$run_id" ] && break
  sleep 15
done
if [ -z "$run_id" ]; then
  echo "Release workflow 未在 15 分钟内完成，请人工检查 Actions" >&2
  exit 1
fi
conclusion=$(gh run view "$run_id" --json conclusion --jq .conclusion)
if [ "$conclusion" != "success" ]; then
  echo "Release workflow 结论为 $conclusion，终止" >&2
  exit 1
fi
echo "[5/8] Release workflow 全绿。"

# 6. Publish the Homebrew tap formula pinned to this tag with real SHA-256.
art_dir=$(mktemp -d "${TMPDIR:-/tmp}/freefm-release.XXXXXX")
gh release download "$version" --repo Yuxin-Qiao/FreeFM --dir "$art_dir"
darwin_sha=$(shasum -a 256 "$art_dir/freefm-$version-darwin-arm64.tar.gz" 2>/dev/null | awk '{print $1}')
linux_x86_sha=$(shasum -a 256 "$art_dir/freefm-$version-linux-x86_64.tar.gz" 2>/dev/null | awk '{print $1}')
linux_arm_sha=$(shasum -a 256 "$art_dir/freefm-$version-linux-arm64.tar.gz" 2>/dev/null | awk '{print $1}')
if [ -z "$darwin_sha" ] || [ -z "$linux_x86_sha" ] || [ -z "$linux_arm_sha" ]; then
  echo "Release 产物不完整，请检查下载目录 $art_dir" >&2
  exit 1
fi
if [ -z "$tap_dir" ]; then
  tap_dir=$(mktemp -d "${TMPDIR:-/tmp}/freefm-tap.XXXXXX")
  git clone --depth 1 https://github.com/Yuxin-Qiao/homebrew-tap.git "$tap_dir"
fi
formula="$tap_dir/Formula/freefm.rb"
mkdir -p "$(dirname "$formula")"
sed \
  -e "s|# sha256 \"<checksum-darwin-arm64>\"|sha256 \"$darwin_sha\"|" \
  -e "s|# sha256 \"<checksum-linux-x86_64>\"|sha256 \"$linux_x86_sha\"|" \
  -e "s|# sha256 \"<checksum-linux-arm64>\"|sha256 \"$linux_arm_sha\"|" \
  scripts/formula/freefm.rb >"$formula"
cd "$tap_dir"
if git diff --quiet; then
  echo "[6/8] Formula 无变化（可能已发布）。"
else
  git add Formula/freefm.rb
  git commit -m "freefm: pin $version with verified checksums"
  git push origin HEAD
  echo "[6/8] Homebrew tap 已更新。"
fi

# 7. Verify the formula for real.
brew tap Yuxin-Qiao/tap
brew install freefm
brew test freefm
brew audit --strict freefm
echo "[7/8] brew tap/install/test/audit 全部通过。"

# 8. Un-pause the deterministic Hermes cron (no-agent, every 6h).
if command -v hermes >/dev/null 2>&1; then
  hermes cron create "0 */6 * * *" --name "$hermes_job" --script freefm-sync.sh --no-agent || true
  echo "[8/8] Hermes $hermes_job 已启用（0 */6 * * *，--no-agent）。"
else
  echo "[8/8] 未检测到 hermes CLI；请人工启用：hermes cron create \"0 */6 * * *\" --name $hermes_job --script freefm-sync.sh --no-agent"
fi

echo
echo "=== FreeFM $version 发布收口完成 ==="
