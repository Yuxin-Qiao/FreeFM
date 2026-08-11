# FreeFM 版本发布规则

FreeFM 的版本发布相当克制、求稳。每个版本都必须 fail-closed；发布节奏由人
决定，不由自动化或期限驱动。

## 1. 版本号规则

FreeFM 遵循语义化版本（SemVer），当前处于 `0.x.y` 阶段。一次有效发布必须同时
满足：

- `Cargo.toml` 的 `version`
- git tag `v<版本号>`
- 固定到该 tag（含真实校验和）的 Homebrew formula

版本类型：

- major（`X.0.0`）：破坏 CLI、协议或输出契约；改变 append-only 歌单行为、
  免费判定、凭据处理或 MSRV；新增平台义务。
- minor（`0.X.0`）：向后兼容的新功能或命令；新增平台支持。
- patch（`0.0.X`）：纯修复、文档、依赖升级或 CI 改动，且不改变产品行为。

行为变更不得藏在 patch 里。任何影响 `sync`、免费判定或凭据处理的改动至少是
minor；任何破坏既有契约的改动必须是 major。

## 2. 发布节奏与克制

- 无定时、无自动发布。每次发布都需要 Release owner 的人工 Go/No-Go。
- 同一自然日（Asia/Shanghai）最多一个发布 tag。
- 只读观察窗（仅 `status`/`preview`）：
  - patch：24 小时
  - minor：7 天
  - major：7 天
- 观察期间记录 session 有效性、普通账号确认（`vipType == 0`）、被动 FM 批次
  盐化去重与证据增长；不得执行 play、skip、trash、scrobble 或 `sync`。
- hotfix 无豁免：任何版本，包括紧急修复，都必须完整通过门禁和观察窗。

## 3. 发布门禁（每个版本无例外）

- 版本号 bump 必须是独立提交（`release: vX.Y.Z`），只含版本变更与发布说明，
  不与功能提交混在一起。
- 打 tag 前的本地门禁：
  - `cargo fmt --all -- --check`
  - `cargo test --all-targets --locked`
  - `cargo clippy --all-targets --locked -- -D warnings`
  - `cargo build --release --locked`
  - `git diff --check`
  - `cargo audit`
  - `gitleaks`
  - `scripts/release-smoke-test.sh`
- CI 与仓库状态：`main` 的 CI 绿色；工作区干净；`HEAD == origin/main`。
- 供应链：Actions 引用固定到 commit SHA；Release workflow 生成 artifact
  attestation 与 CycloneDX SBOM；发布 tarball 只包含 binary、README、LICENSE。
- tag 与发布：所有门禁通过后才推送 `vX.Y.Z`；等待 Release workflow 全绿
  （构建/校验/attest/SBOM/发布）；核对 checksum 与 Homebrew
  `brew install` / `brew test` / `brew audit`。
- 证据：把“命令、退出码、脱敏摘要、commit/tag SHA”写入
  `V01-VALIDATION.md`；不写“已验证”而没有证据。

## 4. No-Go 条件

出现以下任一情况立即停止，并保持所有周期 `sync` 暂停：

- 登录失效或认证失败
- API 结构出现意外变化
- 歌单 owner 歧义
- 免费播放证据缺失、异常或矛盾
- 泄漏扫描失败
- 试图绕过环境限制（DNS、VPN、代理、Cargo 镜像）
- 任一门禁或 workflow 失败

## 5. 范围纪律

- 保持 `preview` 只读、`sync` 只追加。绝不解锁受限音频、替换播放 URL、下载
  媒体或自动替换录音。
- 保持一次性模型：正常运行不引入 daemon、调度器、数据库服务、Web 服务或
  模型调用。
- 凭据只留在本机：绝不打印或提交 Cookie、`MUSIC_U`、QR key、播放 URL、完整
  API 响应或账号标识。
- `v0.1.0` 由 `TEAM-COMPLETION-PLAN.zh-CN.md` 与 `scripts/release-closeout.sh`
  管理；本规则适用于后续版本。
