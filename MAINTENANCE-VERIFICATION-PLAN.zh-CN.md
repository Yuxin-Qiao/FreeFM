# FreeFM 维修验证计划（v0.1 发布收口）

更新：2026-08-12 修订 3（Asia/Shanghai）
适用仓库：`Yuxin-Qiao/FreeFM`（本地 `free-music-agent`）
目标：在 `v0.1.0` 发布前，把全部剩余缺口逐项验证、修复并留下脱敏证据；
只有本计划所有 Go 门槛通过，才允许创建 tag、发布 Release、更新 Homebrew、
恢复无人值守同步。

修订 3：完成 `scripts/release-closeout.sh` 的 fail-closed 加固：必需工具、
独立 release 提交、tarball/ZIP 精确清单、checksum、attestation、下载后二进制
smoke 和 Hermes 创建后确认；同步 08-12 当前审计事实。G14 的问题复盘仍保留在
2.5 节，作为变更依据。

本计划与 `TEAM-COMPLETION-PLAN.zh-CN.md` 配套：后者是总纲，本文是
“从 2026-08-11 到今天”的逐日执行手册。所有步骤 fail-closed：
证据缺失、结构异常、登录失效、免费判定矛盾、泄漏扫描失败 → 立即 No-Go，
保持一切周期同步暂停。

---

## 0. 审计结论（2026-08-12 当前刷新）

### 2026-08-12 当前审计刷新

- 本轮代码/closeout 提交 `e146707` 已由主 CI `31564640529` 和 CodeQL
  `31564640565` 全绿验证，覆盖 Linux/macOS、MSRV、RustSec、secrets、verification
  和 coverage；随后仅有事实文档收口提交直接推送 `main`，当前 `HEAD == origin/main`
  且工作区干净。由于按用户要求直推 `main`，GitHub ruleset 记录了 PR/7 个
  required checks 被 bypass；本轮没有 `ai-review` check，不能把它记作规则集完整通过。
- 当前本地门禁为 `cargo test --all-targets --locked`：87 个库测试通过、1 个
  ignored，主程序 5 个测试通过；Clippy、release build、format、diff-check、
  shell、Skill/WorkBuddy 检查均通过。
- 当前 `cargo audit` 扫描 188 个依赖、0 漏洞；gitleaks 扫描约 1.69 GB、0
  泄漏。当前 release binary 为 1,970,848 bytes，SHA-256 为
  `1458f5c5fab5ebb91cb7d83090c553ce981e108859c0a86080045e37a75648c4`。
- 本机没有 Spotify、Apple Music 或 YouTube 外部凭证；三平台真实 E2E 仍未声明
  完成。当前仍无 `v0.1.0` tag、GitHub Release 或已发布 Homebrew Formula。
- 观察目录当前为 61 条 session、63 条 passive 记录；LaunchAgent 已加载但当前
  不运行。Hermes 当前有一个暂停的旧 `freefm-hourly` 任务，但没有 active job；
  观察脚本实际使用已安装的
  `$HOME/.local/bin/freefm`，与当前 checkout 的 release binary provenance 尚需
  在最终发布前对齐。

### 历史基线（截至 2026-08-11；不覆盖上面的当前刷新）

- 产品运行时：native Rust CLI/TUI，`auth / preview / sync / audit / review /
  status / doctor / tui / version` 九个命令；`preview`/`audit` 只读，
  `sync` 仅 append-only，`review` 仅本机写 trusted mapping。
- 安全边界：`vipType == 0` 才能写歌单；免费判定缺字段/矛盾即跳过；
  owned playlist 名称+owner 校验、重名 fail-closed、跨进程锁、崩溃恢复、
  幂等去重；candidate-only 免费同曲搜索，自动替换永久关闭。
- 本地门禁曾全绿：fmt / test（45+4 通过，1 ignored）/ clippy / release /
  diff-check；gitleaks 零发现；`cargo audit` 0 漏洞（1,207 advisories，
  188 依赖）。
- release binary（arm64）：1,887,472 字节，SHA-256
  `414d82055d8cf3295b9dd71b3648fc22b32da325bafd56f9db593f20c4039703`。
- 长期观察（截至 2026-08-11 01:25 UTC）：35 次 session 检查全部
  authenticated、`vipType=0`；37 批被动 FM、104 个唯一盐化 track hash，
  零失败。LaunchAgent `com.freefm.validation` 已加载、处于暂停，
  脚本只含 `status` 和 `preview`，无任何写接口。
- FM-queue 实验：n=2，两次官方客户端队列均“无推进”（主观观察，非证明），
  已记录于 `V01-VALIDATION.md` 与 `~/.freefm-validation/fmqueue-experiment.jsonl`。
- 供应链：CI 和 Release workflow 已固定完整 Action SHA；Release 含
  attestation（`attest-build-provenance`）与 CycloneDX SBOM job。
- Hermes：当前只有暂停的旧 `freefm-hourly`，没有 active job（正确保持周期同步
  暂停）；`~/.hermes/scripts/freefm-sync.sh` 已安装且与仓库副本一致。
- GitHub：`main` = `8963d0f`（PR #14 合并后 CI 全绿），本地工作区干净；
  `homebrew-tap` 仓库已建（HEAD `07ca41a`），Formula 模板
  `scripts/formula/freefm.rb` 就绪。
- 08-11 每日聚合：36/36 session 检查全部 authenticated 且 `vipType=0`；
  38 批被动 FM、106 个唯一盐化 track hash、零失败；每批 HTTP 11-17 次；
  证据文件约 25.8 KB；LaunchAgent 只含 `status`/`preview`；Hermes cron 为空。
- `freefm --version` 实测退出码 0（输出 `FreeFM 0.1.0`），Formula `test do`
  与 `scripts/release-smoke-test.sh` 的 `--version` 用法均有效，无需修改；
  smoke test 完全离线可跑（mock curl + 本地 tarball）。

### 未达成（本计划要消灭的缺口）

| 编号 | 缺口 | 阻塞方 | 当前状态 |
|---|---|---|---|
| G1 | ~~工作区 4 个文件改动未提交~~（PR #14 已合并） | 无 | ✅ 完成 |
| G14 | closeout 脚本发布缺陷：轻量 tag、产物/attestation 校验不足、Hermes 创建失败被吞掉、Release 下载后二进制未验证 | 无 | ✅ 已修复并由 `e146707` 的主 CI/CodeQL 验证 |
| G2 | +7d 被动 FM / session 观察未完成 | 时间 | 门槛 2026-08-16 23:21:34 CST，还差约 5 天 |
| G3 | session 服务端撤销 → fail-closed → 重新扫码 → 重启恢复未实跑 | 账号持有人扫码 | 未开始 |
| G4 | 手工歌曲安全验证（L2）未实跑 | 账号持有人 1 分钟 | 未开始 |
| G5 | `v0.1.0` tag / GitHub Release / checksum / SBOM / attestation 下载验证 | 先 G1+G2 | 未开始 |
| G6 | Homebrew Formula 发布与 tap/install/test/audit | 先 G5 | 模板就绪未发布 |
| G7 | Codex Skill 实机：cc-switch catalog 问题、`~/.codex/skills` 安装、`codex sandbox -- freefm sync --quiet` | 本机环境 | Skill 已安装；隔离 `status` 已通过；默认 catalog 因 `audio` 类型解析失败，`sync` 未实跑 |
| G8 | WorkBuddy 真实客户端导入 | 已登录客户端 | 客户端导入已记录；正式 Release ZIP checksum/attestation/只读调用未验证 |
| G9 | OpenClaw/Hermes 固定 tag 重装、ClawHub stable channel | 先 G5 | 未开始 |
| G10 | Hermes `freefm-sync`（每 6h、`--no-agent`）创建与 24h 正式周期观察 | 先 G2+G5 | closeout 最后一步 |
| G11 | trusted mapping 真实“重命中”（同一原曲再次出现） | 自然 FM 轮转 | fixture 已覆盖，真实未发生 |
| G12 | PR #13（ai-review-bot）去留决策 | 团队 | ✅ PR #13 已合并；当前仍需确认 main 直推 commit 的 ai-review check 缺口 |
| G13 | README/文档最终一致性复核（30 秒首屏、中英一致、平台状态如实） | 无 | 本轮已刷新事实；Release 前仍需最终复核 |

### 已知风险（如实记录，不掩盖）

- 网易接口为未公开行为：burst 轮询 FM 端点会触发 HTTP 405（实测约 37 次
  快速循环后出现；`status` 不受影响）。结论：禁止高频率轮询，默认周期
  保持 6 小时，不做 quota 计数器。
- FM fetch 是否“消费”服务端队列仍未知：n=2 无推进只是客户端侧主观观察。
  因此产品话术是“自动采样推荐”，不是“无副作用观察”；若后续出现推进
  证据，降低频率或要求 opt-in。
- 免费判定依赖 privilege/`st`/播放能力字段组合，网易改字段即可能误判
  （fail-closed 方向：宁可跳过）。audit 只能发现“已变 VIP”，v0.1 不修复。

---

## 1. 每日验证循环（2026-08-11 → 08-16）

负责人：Validation owner；复核：Release owner。
每天一次，写进 `V01-VALIDATION.md` 的每日追加区（禁止覆盖历史）。

命令（全部脱敏聚合，禁止打印原始内容）：

```sh
EV=~/.freefm-validation
echo "started_at=$(cat $EV/started-at)"
jq -s '{n: length,
        auth_ok: map(select(.authenticated==true))|length,
        vip0: map(select((.account_vip_type//-1)==0))|length,
        failures: map(.failure//.failure_type//"ok")|group_by(.)|map({type:.[0],n:length})}' \
  "$EV/session.jsonl"
jq -s '{batches: length,
        uniq_batch: (map(.batch_hash)|unique|length),
        uniq_track: ([.[].track_hashes[]]|unique|length),
        failures: map(.failure//.failure_type//"ok")|group_by(.)|map({type:.[0],n:length})}' \
  "$EV/passive.jsonl"
wc -c "$EV/session.jsonl" "$EV/passive.jsonl"   # 只记录字节数
```

同时确认：

```sh
launchctl print "gui/$(id -u)/com.freefm.validation" >/dev/null && echo loaded
rg -o 'freefm [a-z]+' ~/.local/libexec/freefm-validation-observe.sh | sort -u
# 期望输出只含 status 和 preview
```

每日通过标准：authenticated 数与样本数相等、全部 `vipType=0`、无失败类型、
track 去重数仍在增长或如实记录停滞、LaunchAgent 只读。
每日 No-Go：任何未解释失败、证据文件出现原始标识、LaunchAgent 曾执行写操作。

## 2. G1 工作区收口（立即执行，1-2 小时）

状态：✅ 历史基线已完成（PR #14 已合并，旧 `main` = `8963d0f`）。本节保留作
复核依据；本轮 closeout/文档变更按用户要求直接推送 `main`，不创建 PR。

负责人：Rust owner；复核：Release owner。

1. 先把本地 `main` 更新到 `origin/main`（`111ea17`），确认无冲突：

```sh
git fetch origin
git status --porcelain=v1 -b        # 期望只有 G1 的 4 个文件
git merge --ff-only origin/main     # 或按团队惯例 rebase
```

2. 显式暂存（禁止 `git add -A`）：

```sh
git add .gitignore TEAM-COMPLETION-PLAN.zh-CN.md V01-VALIDATION.md scripts/release-closeout.sh
```

3. 逐文件复核 diff：closeout 脚本新增的 `sh -n` 循环、WorkBuddy 打包、
   Hermes helper 安装逻辑不改变任何门禁顺序；计划文档只改频率表述；
   V01 只追加 Hermes 空 cron 备注。
4. 显式提交（消息建议：`chore: closeout script hardening and plan refresh`），
   直接推送当前 `main`；等 CI 全绿（macOS/Linux/MSRV/RustSec/secrets/打包）。
5. 确认本地 `main` 与远端 commit 一致、工作区干净。

通过标准：远端 `main` 包含该 commit、CI 全绿、本地无未提交 diff。
No-Go：CI 任一 job 失败且与本批改动相关 → 修复重跑，禁止“修 tag 绕过”。

## 2.5 G14 closeout 脚本发布缺陷修复（立即执行，先于一切发布动作）

负责人：Release owner；复核：Rust owner。
背景：2026-08-11 对 `scripts/release-closeout.sh` 逐行复核并本机实测
`./target/release/freefm --version`（exit 0）后，确认 4 处缺陷、1 处无需
修改。所有修改保持“每步 fail-closed、先验证后发布”的顺序不变，不改变
8 步流程结构。

### C1 轻量 tag → annotated tag

定位：`git tag "$version"`（步骤 5）。改后：

```sh
git tag -a "$version" -m "FreeFM v0.1.0"
```

验收：`git cat-file -t v0.1.0` 输出 `tag`；推送后 Release workflow 的
`--verify-tag` 不受影响。

### C2 步骤 6 补产物完整性校验（当前只验 3 个 tarball）

`gh release download` 后必须确认以下产物全部存在（缺任一即 exit 1）：

- `freefm-v0.1.0-darwin-arm64.tar.gz` 及 `.sha256`
- `freefm-v0.1.0-linux-x86_64.tar.gz` 及 `.sha256`
- `freefm-v0.1.0-linux-arm64.tar.gz` 及 `.sha256`
- `freefm-workbuddy.zip`（`unzip -l` 非空，含 `SKILL.md`）
- `freefm-sbom.cdx.json`（`jq -e '.bomFormat=="CycloneDX"'` 可解析）

同时把计算出的 SHA-256 与各 `.sha256` sidecar 内容比对，不一致即 No-Go。
下载可用 `--pattern 'freefm-v0.1.0-*'`、`--pattern 'freefm-workbuddy.zip'`、
`--pattern 'freefm-sbom.cdx.json'` 分次拉取，避免静默跳过。

### C3 `brew audit` 补 `--online`（步骤 7）

```sh
# 改前
brew audit --strict freefm
# 改后（与第 7 节计划一致）
brew audit --strict --online Yuxin-Qiao/tap/freefm
```

说明：本机 audit 不等于干净环境；如 CI 有 macOS runner，可在
`.github/workflows/ci.yml` 增加可选 tap-audit job（P2，非发布阻塞），
结果如实记录。

### C4 `gh run list --branch` 不可靠 → API 兜底（步骤 5 轮询处）

轮询循环内先试 `gh run list --workflow release.yml --branch "$version"`；
连续 3 次无 completed run 时改用：

```sh
run_id=$(gh api "repos/Yuxin-Qiao/FreeFM/actions/runs?event=push&per_page=20" \
  --jq '.workflow_runs[] | select(.head_branch=="'"$version"'" and .name=="Release") | select(.status=="completed") | .id' \
  | head -1)
```

仍为空则保留现有人工检查提示并 exit 1。禁止 `--watch` 或单次 sleep 超过
15 分钟。

### C5 Release notes 内容校验（步骤 6 之后新增）

```sh
gh release view "$version" --repo Yuxin-Qiao/FreeFM --json body --jq .body
```

正文必须覆盖：实验性/未公开接口、普通账号限定（`vipType=0`）、
append-only、candidate-only、零常驻、剩余限制。缺任一项先
`gh release edit "$version" --notes-file <文件>` 补齐，再继续 Homebrew。

### 修复后门禁（必须全过再提交）

```sh
sh -n scripts/release-closeout.sh
git diff --check
bash scripts/release-smoke-test.sh        # 离线可跑，须全绿
./target/release/freefm --version         # 期望 exit 0
```

显式 `git add scripts/release-closeout.sh MAINTENANCE-VERIFICATION-PLAN.zh-CN.md`
（禁止 `git add -A`），提交消息建议：
`fix: closeout annotated tag, asset completeness, brew audit online, run lookup fallback`，
推送分支开 PR，等 CI 全绿后合并。

通过标准：脚本逐行复核无未处理缺陷；PR CI 全绿；`sh -n`、diff-check、
smoke test 均 exit 0。No-Go：任何一项失败 → 修复重跑，禁止绕过。

## 3. G2 +7d 门槛收口（2026-08-16 23:21:34 CST 之后）

负责人：Validation owner；复核：Release owner。

1. 先运行第 1 节的聚合命令，把 7 天汇总写进 `V01-VALIDATION.md`：
   首末时间、样本数、authenticated 数、`vipType=0` 数、批次/track 去重、
   失败类型、证据文件字节数、期间 HTTP 请求数范围。
2. 确认观察期间无播放/skip/trash/scrobble/`sync` 调用（查 shell history
   中 LaunchAgent 调用记录与 `launchctl` 日志）。
3. 停止并移除 LaunchAgent：

```sh
launchctl unload ~/Library/LaunchAgents/com.freefm.validation.plist
rm ~/Library/LaunchAgents/com.freefm.validation.plist
launchctl print "gui/$(id -u)/com.freefm.validation" >/dev/null 2>&1 && echo STILL_LOADED || echo REMOVED
```

4. 脱敏证据保留在 `~/.freefm-validation/`，不提交到 Git、不删除，直到
   Release owner 复核完成。

通过标准：7 天证据完整、跨天 session 有效、证据增长有界、LaunchAgent
已完全移除。No-Go：任何未解释失败或证据异常 → 全部同步保持暂停并报告。

## 4. G3 session 撤销与重新扫码（P0-4，需要账号持有人）

前置：G2 完成（避免破坏 7 天 session）。

1. 记录最终 release binary 的 SHA-256，不复制 session。
2. 账号持有人在网易云官方客户端撤销该登录。
3. 运行 `freefm status --json` 与 `freefm sync --quiet`，期望稳定
   `login_required` 且无任何歌单创建/追加；检查 stdout/stderr 无凭证、
   账号 ID、歌曲 ID 或 URL。
4. 运行 `freefm auth`，本人扫码；进程退出后重启，确认 authenticated 且
   `vipType=0`。
5. 运行只读 `preview` 确认可读；如需再次验证写入，只能由 Release owner
   单独批准。

通过标准：撤销 → fail-closed → 重扫 → 重启恢复四个状态均有脱敏证据。
No-Go：撤销后仍能写入，或重新认证要求粘贴 Cookie。

## 5. G4 手工歌曲安全（L2，1 分钟）

账号持有人在官方客户端向 `FreeFM · Auto` 手动加入任意一首歌（或复制一份
歌单），运行 `bash scripts/acceptance-live.sh`，然后回官方客户端确认
手工歌曲仍在原位、顺序未变。结果记录到 `FUNCTIONAL-ACCEPTANCE.md`。

## 6. G5 v0.1.0 发布（P0-6/7/8）

负责人：Release owner。前置：G1-G4 全过。

1. 最终本地门禁（在干净候选 commit 上依次执行，全部要求退出码 0）：

```sh
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
cargo audit
git diff --check
gitleaks dir . --no-banner --redact
sh -n scripts/*.sh automation/hermes/*.sh automation/launchd/*.sh skills/freefm/scripts/*.sh
scripts/package-workbuddy.sh target/freefm-workbuddy.zip
```

记录每条退出码、测试数、binary 字节数与 SHA-256、`freefm --version`
冷启动和峰值 RSS、`~/.freefm` 与证据目录字节数。
2. 提交（显式文件列表），推送，确认远端 SHA 与本地 HEAD 一致，等 CI 全绿。
3. 运行 `bash scripts/release-closeout.sh`（G14 修复后的版本；脚本自身校验
   +7d 门槛；分 8 步每步 fail-closed）：
   - 步骤 1：时间门槛（已由 G2 保证）；
   - 步骤 2：移除 LaunchAgent（已由 G2 做，脚本会复核）；
   - 步骤 3-4：本地门禁 + main 干净检查；
   - 步骤 5：annotated `v0.1.0` tag，推送并等待 Release workflow（macOS
     arm64、Linux x86_64、Linux arm64、WorkBuddy ZIP、SHA-256、SBOM、
     attestation）；
   - 步骤 6：下载校验全部产物（含 G14-C2 的 SBOM/WorkBuddy/sidecar 比对），
     并按 G14-C5 校验 Release notes；
   - 步骤 7：brew tap/install/test/audit（audit 含 `--online`）；
   - 步骤 8：恢复 Hermes `freefm-sync`（每 6h、`--no-agent`）。

## 7. G6 Homebrew（P0-9）

前置：G5 Release 验证完成。

1. 用固定 `v0.1.0` Release URL 和实际 tarball SHA-256 填充
   `scripts/formula/freefm.rb`，推送到 `Yuxin-Qiao/homebrew-tap` 的
   `Formula/freefm.rb`。
2. 干净环境实跑：

```sh
brew tap Yuxin-Qiao/tap
brew install Yuxin-Qiao/tap/freefm
brew test Yuxin-Qiao/tap/freefm
brew audit --strict --online Yuxin-Qiao/tap/freefm
```

3. 卸载后重装一次，确认不依赖源码工作区或 Cargo cache；安装 binary SHA
   与 Release 对应。
4. 通过后更新 README 的 Homebrew 安装段（此前如标 pending 才解除）。

## 8. G7-G10 平台实机（P1-1/2/3）

负责人：Platform owner。需要本机用户配合的步骤明确标注。

### Codex（G7）

1. 处理 `~/.codex/cc-switch-model-catalog.json` 解析问题：由 cc-switch
   重新生成或安全移走，不手工猜字段。
2. 安装 `skills/freefm` 到 `~/.codex/skills/freefm`，重启 Codex 确认可发现。
3. 用只读 `freefm status --json` 验证 sandbox permissions profile 可访问
   网络与 `~/.freefm`；实跑 `codex sandbox -- freefm sync --quiet`，
   确认 exit 0、无输出、未启动 Agent 回合。
4. 文档明确：`codex exec`/桌面循环会消耗 token；零 token 周期只能走
   OS cron/launchd 或 deterministic sandbox command。

### WorkBuddy（G8，需要已登录客户端）

1. 从 `v0.1.0` Release 下载 ZIP，校验 checksum/attestation。
2. 在已登录 WorkBuddy 客户端“添加技能 → 上传技能”真实导入；确认平台只
   调用本地 binary，不读取/上传 `~/.freefm`。
3. 先 `status`/`preview`；未经人工批准不 `sync`。真实导入成功前，只宣称
   “本地 Skill 包兼容”，不宣称已上架。

### OpenClaw / Hermes / ClawHub（G9/G10）

1. 用固定 `v0.1.0` tag/commit 重装 OpenClaw 与 Hermes Skill（禁止引用
   移动的 `main`）。
2. OpenClaw isolated Gateway command job 再跑两次 `freefm sync --quiet`：
   无 Agent message、无 delivery、空输出、幂等。
3. Hermes `no_agent/script-only` 再跑两次：run record 为 no-agent、空输出。
4. 发布 ClawHub `0.1.0` stable channel（安全扫描通过、包含 helper）。
5. closeout 步骤 8 已安装 helper 并创建 `freefm-sync`（每 6h、`--no-agent`）；
   观察首个正式周期：成功时空输出，失败立即再次暂停。

## 9. G11-G13 收尾

- G11 trusted mapping 真实重命中：等自然 FM 轮转，出现同原曲时确认
  `preview` 输出 `trusted_mapping` 且重新验证目标仍可播；不出现则以
  fixture 证据 + 明示“真实重命中未发生”收尾。
- G12 PR #13：发布收口前由团队决策合并或关闭；不得让未审 bot 混入发布。
- G13 文档最终复核：README 30 秒回答“是什么/安全吗/怎么装/成熟度”；
  中英命令与平台状态一致；平台表格官方彩色标志不依赖第三方热链；
  `V01-VALIDATION.md` 区分“真实证明/离线 fixture/未验证”；SECURITY、
  CONTRIBUTING、LICENSE、About 一致。

## 10. 最终 Go/No-Go

全部通过才允许宣布 v0.1 完成并开启周期同步：

- [ ] +24h 与 +7d 脱敏证据完成，无未解释失败
- [ ] G14 closeout 脚本修复已合并且 CI 全绿（annotated tag / 产物完整性 /
      brew audit `--online` / run 查询兜底 / Release notes 校验）
- [ ] G3 撤销/重扫/重启恢复通过
- [ ] G4 手工歌曲安全通过
- [ ] 本地门禁、RustSec、gitleaks、MSRV、CI 全绿；`main` 干净且与 tag 一致
- [ ] `v0.1.0` Release 从零下载、校验、运行通过（checksum+SBOM+attestation）
- [ ] Homebrew tap/install/test/audit 通过
- [ ] OpenClaw/Hermes 固定 tag 零 LLM 路径复验通过
- [ ] Codex deterministic sandbox 完成实机验证，或 Release 中明确标 pending
- [ ] WorkBuddy 真实导入完成，或只以“本地兼容包”发布并明确未上架
- [ ] 所有公开材料无凭证、原始歌曲/歌单数据、播放 URL 或完整响应

任何一项失败：保持 Hermes 及其他周期 `sync` 暂停；允许人工执行
`status`/`doctor`/`preview`；不得为赶版本降低免费判定、owner 校验、
幂等或凭证边界。

## 11. 每日回报模板

```text
任务 ID：G1-G13 / 阶段 1-10
负责人：
候选 commit/tag：
执行时间（UTC + Asia/Shanghai）：
执行命令：
退出码：
脱敏结果：只写计数、布尔值、大小、SHA 和失败类型
产物位置：仓库相对路径或公开 Release URL
复核人：
结论：PASS / NO-GO / BLOCKED_BY_ENVIRONMENT
下一步：
```

环境阻塞（如沙箱无网、客户端未登录）必须标
`BLOCKED_BY_ENVIRONMENT`，与产品失败分开；不据此判断 FreeFM 不可行，
也不得为此修改 DNS、VPN、代理或 Cargo 镜像。
