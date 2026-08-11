# FreeFM v0.1 待完成清单与发布执行计划

更新：2026-08-10（Asia/Shanghai）
适用仓库：`Yuxin-Qiao/FreeFM`
目标：补齐长期验证、平台实机与供应链门禁，发布可复核的 `v0.1.0`，最后再开启无人值守同步。
后续所有版本遵循 `RELEASING.md` 的版本发布规则；`v0.1.0` 仍按本计划执行。

## 0. 当前基线

### 已完成，不要重复实现

- Native Rust CLI/TUI；产品运行时不是 Skill，也不依赖 Agent、Node、Python、Docker、数据库或 Web 服务。
- `auth`、`preview`、`sync`、`audit`、`review`、`status`、`doctor`、`tui`；
  `preview`/`audit` 只读，`sync` 只追加，`review` 仅本机写 trusted mapping。
- 仅接受明确 `vipType == 0` 的普通账号；免费判定缺失、异常或矛盾时 fail-closed。
- 受限歌曲的免费同曲搜索结果仅 `candidate_only`，v0.1 不自动替换；用户经
  `freefm review` 明确确认后才保存 trusted mapping，且每次使用都重新验证目标可播。
- `freefm audit` 只读复查 `FreeFM · Auto` 全部歌曲（still_free /
  became_restricted / unavailable / unknown），需要关注时以结构化输出和 exit 3 提示，
  不删除、不替换、不 repair。
- `FUNCTIONAL-ACCEPTANCE.md` 是唯一验收记录：P0/P1 逐项黑盒验收，含
  `BLOCKED_USER_ACTION`（真实账号主链、review 闭环、手工歌曲安全）和
  `BLOCKED_EXTERNAL`（7 天观察）清单；`scripts/acceptance-live.sh` 用
  `~/.freefm-acceptance/` 干净目录跑真实账号主链，只记录脱敏汇总。
- 长期健康自动化：`sync` 之外新增每日 `freefm audit --quiet` 调度助手
  （`skills/freefm/scripts/freefm-audit.sh` 与 `automation/hermes/freefm-audit.sh`
  双份一致，WorkBuddy ZIP 与 CI 同步校验）。
- owned playlist 名称和 owner 校验、重名 fail-closed、跨进程锁、远端复读、崩溃恢复和幂等。
- fake transport、脱敏 fixture、分页/500 首分块、超时/5xx/登录失效/状态损坏等离线测试。
- 真实 session 重启恢复、真实 owned playlist append、复读确认和第二次静默幂等。
- OpenClaw deterministic command job 与 Hermes `no_agent` 手动周期已证明不启动 LLM；Hermes 周期仍暂停。
- Codex Skill 结构、WorkBuddy ZIP、TUI 设置页、Rust 模块拆分、MSRV 和四个平台官方彩色图标已在本地完成。

### 2026-08-10 实时基线

- 本地门禁：35 项测试通过，另有 1 个子进程辅助测试 ignored；format、Clippy、release build、`git diff --check` 全绿。
- RustSec：1,198 条 advisory，188 个锁定依赖，无漏洞报告。
- gitleaks：当前工作区无泄漏发现。
- arm64 release binary：1,854,240 bytes；SHA-256
  `aadf7af20bda4301a9aceacf3f3e2c7c26b94809c8be4423afc327cee4db5979`。
- session 观察：13/13 成功、13/13 authenticated、13/13 `vipType=0`，无失败。
- 被动 FM：15 批、45 个盐化 track hash、15 个盐化 batch hash，全部唯一，无失败。
- LaunchAgent `com.freefm.validation` 每小时只执行 `status` 和 `preview`，不执行
  `sync`、play、skip、trash 或 scrobble。
- GitHub `main` 最新已提交版本 CI 绿色，但当前模块拆分、Codex、图标和文档改动尚未提交；旧 CI 不能作为当前工作区证明。
- 尚无 `v0.1.0` tag、GitHub Release 或 Homebrew Formula。

### 时间门槛

- +24h：2026-08-10 23:21:34（Asia/Shanghai）。
- +7d：2026-08-16 23:21:34（Asia/Shanghai）。
- +7d 全部门禁完成前，Hermes `freefm-hourly` 保持暂停，不建立其他周期 `sync`。
- +7d 到达后可直接运行 `scripts/release-closeout.sh`（验证时间门槛、移除观察
  LaunchAgent、本地全门禁、main 干净、打 tag 并等待 Release workflow、更新并验证
  Homebrew tap、恢复 Hermes no-agent cron）；每步 fail-closed。

## 1. 角色与交付规则

建议指定四名 owner；一人可兼任，但每个证据必须有第二人复核。

| 角色 | 责任 |
|---|---|
| Release owner | 排期、门禁、提交、tag、Release、Homebrew、最终 Go/No-Go |
| Rust owner | 当前 diff 复核、测试、CI、MSRV、供应链配置 |
| Validation owner | +24h/+7d 脱敏统计、session 撤销/重扫、性能与大小 |
| Platform owner | OpenClaw、Hermes、Codex、WorkBuddy 实机与商店材料 |

统一规则：

1. 不输出、复制、上传或提交 Cookie、`MUSIC_U`、session、QR key、原始歌曲/歌单 ID、标题、播放 URL或完整响应。
2. 自动化证据只允许时间戳、批次数、盐化去重数、认证布尔值、`vipType`、请求数、文件大小和失败类型。
3. CI 不访问网易云，不使用真实账号；真实验证只能由账号持有人扫码。
4. 任一登录失效、API 结构异常、owner 歧义、免费证据矛盾或泄漏扫描失败，立即 No-Go，并保持所有周期同步暂停。
5. 不修改 macOS DNS、VPN、代理或 Cargo 镜像来绕过环境问题。
6. 每项任务完成后，把“命令、退出码、脱敏摘要、commit/tag SHA”写入 `V01-VALIDATION.md`；不只写“已验证”。

## 2. P0 发布阻断项

### P0-1 当前工作区收口

负责人：Rust owner；复核：Release owner。
现在即可执行。

- [x] 逐文件审查当前 diff，确认模块拆分没有改变协议、免费判定、append-only 或输出契约。
- [x] 确认 `src/main.rs` 只保留 CLI/TUI 入口，业务逻辑位于 `lib.rs` 及模块中。
- [x] 确认 README 中四个平台使用 `assets/platforms/` 的官方彩色标志，来源和许可证可说明。
- [x] 把中文 WorkBuddy 状态改成“客户端签名已验证，真实导入待登录”，不得暗示已上架。
- [x] 删除或忽略工作树中的 `.DS_Store`；确认任何 `.DS_Store` 均未跟踪。
- [x] 更新 `V01-VALIDATION.md` 中过时的测试数、依赖数和 binary 数据。
- [x] 运行 `scripts/package-workbuddy.sh`，核对 ZIP 仅包含允许的 Skill 文件和 helper。

验收证据：`git diff --stat`、`git status --short`、包内容清单、第二人 review 记录。
通过标准：没有用户 WIP 被覆盖；没有凭证、验证目录、构建产物或 `.DS_Store` 进入待提交列表。

### P0-2 +24 小时被动 FM 结论

负责人：Validation owner。
最早执行时间：2026-08-10 23:21:34（Asia/Shanghai）。
状态：已完成（commit 80660d8 记录 +24h 结论；2026-08-11 09:25 CST 复核：35 次
session 全部 authenticated 且 `vipType=0`，37 批被动 FM、104 个唯一盐化 track
hash，零失败；LaunchAgent 仍在运行观察）。

- [x] 读取 `~/.freefm-validation/started-at`、`session.jsonl`、`passive.jsonl` 和可选 `failures.jsonl`。
- [x] 只聚合首末时间、样本数、成功数、authenticated 数、`vipType=0` 数、盐化 batch/track 总数与去重数、HTTP 请求数范围和失败类型。
- [x] 确认 LaunchAgent 安装脚本和实际运行脚本只包含 `status` 与 `preview`。
- [x] 在 `V01-VALIDATION.md` 记录 +24h 结论，并明确“读取 FM 可能由服务端视为消费批次”仍未知。
- [x] 不停止 LaunchAgent；继续观察到 +7d。

通过标准：至少覆盖完整 24 小时；无写接口；后续批次仍有新的盐化 track，或如实记录停滞限制。
失败处理：出现认证失败、异常字段或证据文件结构变化时，记录稳定失败类型，暂停一切同步，不删除失败证据。

### P0-3 +7 天 session 与证据大小

负责人：Validation owner；复核：Release owner。
最早执行时间：2026-08-16 23:21:34（Asia/Shanghai）。

- [ ] 汇总 7 天 session 有效率、普通账号确认率、被动批次/track 去重数和失败类型。
- [ ] 记录 `session.jsonl`、`passive.jsonl`、`failures.jsonl`、state/session 文件的字节数；不得读取或报告内容。
- [ ] 确认观察期间没有调用播放、skip、trash、scrobble 或 `sync`。
- [ ] 停止、bootout 并移除 `com.freefm.validation` LaunchAgent；确认 `launchctl print` 已不存在。
- [ ] 停止后不得删除脱敏证据，直至 Release owner 完成复核；原始响应不得保留。
- [ ] 更新 `V01-VALIDATION.md` 的最终 7 天结论。

通过标准：跨天 session 行为有时间戳证据；证据增长有界；LaunchAgent 已完全移除。
No-Go：任一未解释失败、证据中出现原始标识或 LaunchAgent 曾执行写操作。

### P0-4 session 撤销与重新扫码

负责人：账号持有人 + Validation owner。
前置：P0-3 完成并停止长期观察，以免提前破坏 7 天 session。

- [ ] 复制最终 release binary 的 SHA-256，不复制 session。
- [ ] 由账号持有人在网易云官方客户端撤销对应登录。
- [ ] 运行 `status --json` 和 `sync --quiet`，确认稳定返回 `login_required`，且没有创建/追加歌单。
- [ ] 检查 stdout/stderr 不包含凭证、账号 ID、歌曲 ID或 URL。
- [ ] 运行 `freefm auth`，由本人扫码；退出并重启进程后确认 authenticated 且 `vipType=0`。
- [ ] 运行一次只读 `preview`；如确需再次验证写入，只能由 Release owner 单独批准。

通过标准：服务端撤销、fail-closed、重新扫码、重启恢复四个状态均有脱敏证据。
No-Go：撤销后仍允许 sync 写入，或重新认证要求用户粘贴 Cookie。

### P0-5 供应链与 CI 固定

负责人：Rust owner。

- [x] 在有网络的受控环境运行 `scripts/pin-actions.sh`，把所有 GitHub Actions 引用固定为完整 commit SHA，并保留版本注释。
- [x] 保持 `rust-version = 1.86`，CI 用同一版本执行 `cargo check --all-targets --locked`。
- [x] CI 的 Rust、MSRV、RustSec、Skill/WorkBuddy 包验证均只使用 fake/fixture。
- [x] Release workflow 为二进制、校验和和 WorkBuddy ZIP生成 GitHub artifact attestation。
- [x] 生成 CycloneDX 或 SPDX SBOM；SBOM 不包含本机路径或环境信息。
- [x] Release tarball 内只包含 binary、README、LICENSE；不要包含 session、日志、target 树或实验输出。
- [x] 审查 workflow permissions，遵循最小权限；只有 Release job 使用 `contents: write`、`id-token: write` 和 `attestations: write`。

通过标准：workflow 中不存在 `uses: ...@vN`、`@stable` 或 branch；CI 绿色；Release dry review 通过。
No-Go：无法确认 Action commit 来源，或 attestation/SBOM 绑定的不是最终 tag commit。

### P0-6 最终本地门禁

负责人：Rust owner；复核：Release owner。
前置：P0-1 至 P0-5 完成。

在干净候选 commit 上依次执行：

```sh
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
cargo audit
git diff --check
gitleaks dir . --no-banner --redact
sh -n skills/freefm/scripts/freefm-sync.sh
sh -n scripts/package-workbuddy.sh
scripts/package-workbuddy.sh target/freefm-workbuddy.zip
```

- [x] 记录每条命令退出码和测试数。
- [x] 记录最终 binary 字节数、SHA-256、`--version` 冷启动和峰值 RSS。
- [x] 记录最终 session/state/evidence 字节数与实际 HTTP 请求数，禁止记录内容。
- [x] 检查 `git status --ignored`，确认 `.freefm/`、`.freefm-validation/`、QR、target 和实验临时文件均未跟踪。

通过标准：全部退出 0；gitleaks 零发现；工作区只含预期发布改动。
No-Go：不得用跳过测试、降低 lint 或删除证据的方式“修绿”。

### P0-7 提交、推送与 GitHub CI

负责人：Release owner。
前置：P0-6 通过。

- [ ] 使用显式文件列表 `git add`，不得 `git add -A`。
- [ ] 提交信息概括 Rust 模块化、平台支持、图标、长期证据和发布硬化。
- [ ] 推送后确认远端 commit SHA 与本地 `HEAD` 一致。
- [ ] 等待 macOS、Linux、MSRV、RustSec 和打包检查全部成功。
- [ ] 若 CI 修改代码，回到 P0-6，从头重跑；不得直接修 tag。
- [ ] 最终确认 `main` 工作区干净、GitHub CI 绿色。

通过标准：远端 `main` 精确对应已审计 commit；没有未提交或未推送 diff。

### P0-8 `v0.1.0` tag 与 GitHub Release

负责人：Release owner。
前置：P0-7 全绿。

- [ ] 创建 annotated `v0.1.0` tag，确认 tag 指向通过 CI 的精确 commit。
- [ ] 推送 tag，等待 Release workflow 完成。
- [ ] 核对 macOS arm64、Linux x86_64、WorkBuddy ZIP、SHA-256、SBOM 和 attestations 均存在。
- [ ] 在空临时目录下载 Release，验证 checksum、attestation、解压文件清单和 `freefm --version`。
- [ ] Release notes 明确：实验性未公开网易云接口、普通账号限定、candidate-only、append-only、零常驻、已验证平台与剩余限制。
- [ ] 确认 Release 页面没有 session、账号数据或内部验证路径。

通过标准：公开 Release 可从零下载、校验、运行；tag、二进制和 SBOM 指向同一源码。

### P0-9 Homebrew tap

负责人：Release owner。
前置：P0-8 Release 验证完成。

- [ ] 在 `Yuxin-Qiao/homebrew-tap` 创建或更新 `Formula/freefm.rb`。
- [ ] Formula 使用固定 `v0.1.0` Release URL 和实际 tarball SHA-256，不使用 `main`。
- [ ] `test do` 至少执行 `freefm --version` 和只读 help；不得访问网易云或用户目录。
- [ ] 在干净环境实跑：

```sh
brew tap Yuxin-Qiao/tap
brew install Yuxin-Qiao/tap/freefm
brew test Yuxin-Qiao/tap/freefm
brew audit --strict --online Yuxin-Qiao/tap/freefm
```

- [ ] 卸载后重新安装一次，确认没有依赖源码工作区或 Cargo cache。
- [ ] README 的 Homebrew 命令只能在上述步骤成功后解除“即将开放”状态。

通过标准：tap/install/test/audit 全部通过，安装 binary SHA 与 Formula 指定 Release 对应。

## 3. 平台实机与发布后任务

### P1-1 Codex

负责人：Platform owner；需要用户本机配合。

- [ ] 由 cc-switch 重新生成或安全移走无法解析的 `~/.codex/cc-switch-model-catalog.json`；不要手工猜字段。
- [ ] 安装 `skills/freefm` 到 `~/.codex/skills/freefm`，重启 Codex，确认 Skill 可发现。
- [ ] 用只读 `freefm status --json` 验证 Codex sandbox permissions profile 可访问网络和指定 `~/.freefm`。
- [ ] 实跑 `codex sandbox -- freefm sync --quiet`，确认 exit 0、stdout/stderr 为空且未启动 Agent 回合。
- [ ] 文档明确：`codex exec` 和桌面循环自动化会消耗 token；零 token 周期必须由 OS cron/launchd 或 deterministic sandbox command 执行。

通过标准：Codex Skill 可发现；确定性路径有本机证据；不把 Agent 自动化宣传为 0 token。

### P1-2 Tencent WorkBuddy

负责人：Platform owner + 已登录用户。

- [ ] 从 `v0.1.0` Release 下载 WorkBuddy ZIP并验证 checksum/attestation。
- [ ] 在已签名、已公证的 WorkBuddy 客户端登录后，通过“专家·技能·连接器 → 添加技能 → 上传技能”真实导入。
- [ ] 确认平台只调用本地 `freefm` binary，不读取或上传 `~/.freefm`。
- [ ] 先执行 `status`/`preview`；未经明确人工批准不执行 `sync`。
- [ ] 截取不含账号、歌曲、路径和凭证的导入成功证据。
- [ ] 真实导入成功前，只宣称“本地 Skill 包兼容”，不宣称已上架 WorkBuddy 市场。

通过标准：真实客户端导入与只读调用成功；权限范围清晰；没有凭证进入平台。

### P1-3 OpenClaw、Hermes 与 ClawHub stable

负责人：Platform owner。
前置：P0-8/P0-9 完成，P0-2/P0-3/P0-4 无异常。

- [ ] 用固定 `v0.1.0` tag/commit 重新安装 OpenClaw 与 Hermes Skill，禁止继续引用移动的 `main`。
- [ ] OpenClaw isolated Gateway command job 再跑两次 `freefm sync --quiet`，确认无 Agent message、无 delivery、空输出且幂等。
- [ ] Hermes `no_agent/script-only` 再跑两次，确认 run record 为 no-agent、空输出且幂等。
- [ ] 发布 ClawHub `0.1.0` stable channel；确认安全扫描通过且安装内容包含 helper。
- [ ] 最后解除 Hermes `freefm-hourly` 暂停；保留失败时的人工提示，不自动唤醒 LLM。
- [ ] 观察首个正式周期；成功时应为空输出，失败时立即再次暂停 job。

通过标准：正常周期 0 LLM token、0 resident process、空输出、无重复追加。
No-Go：宿主不能证明 deterministic/no-agent，或需要 Agent 解释每次成功结果。

## 4. 文档与项目展示复核

负责人：Release owner + 一名非开发者复核。

- [ ] README 首屏在 30 秒内回答“是什么、安全吗、怎么安装、当前成熟度”。
- [ ] 中文/英文 README 命令、平台状态、版本和限制一致。
- [ ] Agent platforms 表格使用官方彩色标志，白色/深色背景下均可辨识，图片不依赖第三方热链。
- [ ] AI 安装 prompt 禁止 AI 读取凭证，并要求用户本人扫码、先 preview、后确认 sync。
- [ ] `AGENTS.md` 保留产品 invariant、测试要求和平台 Skill 边界。
- [ ] `V01-VALIDATION.md` 区分“真实证明”“离线 fixture”“仍未验证”，删除已过时或互相冲突的数据。
- [ ] SECURITY、CONTRIBUTING、LICENSE、release notes 和仓库 About 信息一致。
- [ ] 不在 README 堆砌未完成平台徽章；未实跑的能力明确标记 pending。

## 5. 最终 Go/No-Go

只有以下项目全部勾选，才允许宣布 FreeFM v0.1 完成并开启周期同步：

- [ ] +24h 和 +7d 被动 FM/session 脱敏证据完成，无未解释失败。
- [ ] session 服务端撤销后 fail-closed，重新扫码与重启恢复成功。
- [ ] 当前工作区代码门禁、RustSec、gitleaks、MSRV 和 CI 全绿。
- [ ] GitHub Actions 已固定完整 SHA；Release 有 checksum、SBOM 和 attestation。
- [ ] `main` 干净，远端 commit、tag、Release 源码完全一致。
- [ ] `v0.1.0` Release 从零下载与运行验证通过。
- [ ] Homebrew tap/install/test/audit 通过。
- [ ] OpenClaw 与 Hermes 固定 tag 的零 LLM 路径复验通过。
- [ ] Codex deterministic sandbox 路径完成实机验证，或在 Release 中明确标为 pending。
- [ ] WorkBuddy 完成真实导入，或只以“本地兼容包”发布且明确未上架。
- [ ] 所有公开材料不含凭证、原始歌曲/歌单数据、播放 URL 或完整响应。

任何一项失败：保持 Hermes 和其他周期 `sync` 暂停；允许人工执行 `status`、`doctor` 和 `preview`，但不得为了赶版本降低免费判定、owner 校验、幂等或 credential 边界。

## 6. 团队每日回报模板

```text
任务 ID：P0-x / P1-x
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

环境阻塞必须与产品失败分开。例如网络沙箱无法更新 RustSec 数据库应写
`BLOCKED_BY_ENVIRONMENT`，在获准联网的受控环境重跑；不得据此判断 FreeFM 不可行。
