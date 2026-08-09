# FreeFM v0.1 完成与发布执行方案

日期：2026-08-09（Asia/Shanghai）

## 1. 当前结论与目标状态

当前 FreeFM 已经完成安全的真实 append 闭环，但在 24 小时被动 FM 与
session 跨天/撤销证据补齐前，仍不宣称“完整达到原始产品目标”，也不启动
Hermes gateway 做长期无人值守运行。

已经完成并作为本方案基线保留：

- native Rust 单文件 CLI，包含 `auth`、`preview`、`sync`、`status`、`doctor`；
- 普通非 VIP 账号强制校验；
- privilege 与 player probe 的严格正证据免费判定；
- 受限歌曲的搜索候选仅在 preview 展示，不自动替换；
- owned playlist 唯一性校验、同步前远端复核、append-only 去重；
- Unix `flock` 并发锁、原子 state 写入；
- `RemoteApi` fake seam、31 个通过的离线测试、CI、MIT License、RustSec 和脱敏规则；
- release binary 为 1,802,256 bytes；最终真实 append sync 为 4.02 秒、
  峰值 RSS 15,269,888 bytes、19 个实际 HTTP 请求；
- 仅持久化 `MUSIC_U` 的 session 已通过重启、读取、append 与复读验证；
- Hermes no-agent 手动调度已证明 0 LLM 与空输出；job 已暂停，gateway 暂未启动。

本轮团队目标是交付一个可发布的 v0.1，并用可复核证据回答：

1. 最终安全版本是否真实创建/复用 owned playlist、追加免费原曲并在第二次运行保持幂等；
2. Private FM 在无播放、skip、trash、scrobble 时，跨小时/跨天被动读取是否仍产生新推荐；
3. session 能否跨进程、跨天恢复，失效时是否稳定 fail-closed；
4. 实际 HTTP 请求数、耗时、RSS、binary/state 大小是多少；
5. OpenClaw/Hermes 的正常定时周期是否确实不启动 Agent/LLM，token 数为 0；
6. 网易云是否提供足够强的同录音身份字段；没有则明确把自动免费同曲替换延期，而不是降低安全门槛。

## 2. 已锁定的产品决策

- v0.1 支持 macOS 与 Linux；不要声称支持 Windows。当前非 Unix 锁实现没有有效互斥，团队应删除该 fallback，并在非 Unix 构建时明确报 unsupported，或在未来单独实现跨平台锁。
- v0.1 默认只自动加入“原歌曲本身严格免费”的歌曲。
- 搜索到的免费同曲候选保持 `candidate_only`；只有通过第 7 节身份门槛后才允许启用自动替换。
- FreeFM 仍是 run-once CLI，不新增 scheduler、daemon、数据库、Web Server 或后台进程。
- CI 永远不使用真实账号、不访问网易云、不保存 session。
- `--quiet` 与 `--json` 同时出现时，quiet 优先：成功无输出，失败输出结构化错误；补测试并写入 README。
- 稳定退出码：`0` 成功，`1` 运行/API/人工处理错误，`2` 参数错误。
- 在本方案所有 P0 门槛通过前，不启用周期 `sync`。

## 3. P0：离线可靠性与仓库基线

负责人：Rust/测试工程师。该阶段不得进行远端写入。

### 实现任务

1. 扩展 `RemoteApi` 脚本化 fake，使每个方法都能排队返回成功或指定错误，并记录严格调用顺序。
2. 增加完整流程测试：
   - HTTP timeout、5xx、非 JSON、缺字段、未知 code；
   - 登录失效必须在 Private FM 调用前停止；
   - `preview` 的 create/add 调用数始终为 0；
   - 首次 sync、第二次 sync、远端已有相同 track、超过 500 首 track；
   - subscribed 同名歌单、无 owner、多个 owned 同名歌单、缓存歌单改名/换 owner；
   - create 成功后失败、add 成功后失败、复读失败、state 保存前失败，下一次运行不得重复建歌单或重复添加；
   - 两个并发进程只有一个获得锁；锁持有进程异常退出后，新进程可以继续；
   - `--json`、`--quiet`、组合参数、stdout/stderr、退出码；
   - 所有错误输出不包含 cookie 值、QR key、URL、Authorization 或 session 内容。
3. 将非 Unix 行为改为显式 unsupported；CI 保留 Ubuntu，增加 macOS job 验证 `flock` 路径。
4. 在 CI 增加 RustSec 审计，例如 `rustsec/audit-check@v2`；不把 `cargo-audit` 加入产品运行依赖。
5. 检查 session 最小化：
   - 在隔离 `--data-dir` 中先测试仅保存 `MUSIC_U`；
   - 若状态、FM、owned playlist 读取和 append 都正常，仅持久化该 cookie；
   - 若失败，一次只增加一个实际必需 cookie并记录原因；
   - 不在当前有效 session 上直接做破坏性试验。
6. QR 仅在终端渲染，不再生成 SVG/PNG 文件，因此所有退出路径均无 QR 文件残留。
7. 明确仓库基线：当前所有项目文件仍未跟踪。确认 `.freefm/`、`target/`、实验输出、QR 文件、`.DS_Store` 被忽略并通过 secret scan 后，再创建初始提交。

### P0 验收

- `cargo fmt --all -- --check`、`cargo test --all-targets`、Clippy、release build、RustSec、secret scan 全绿；
- 测试数和覆盖场景写入 `V01-VALIDATION.md`，不得只写“已通过”；
- 两进程锁与四个 crash window 均有自动化测试；
- 最小 cookie 集有隔离实测证据；
- Git 初始提交不包含任何凭证、账号 ID、真实歌单 ID或私人听歌数据。

## 4. P0：实际 HTTP 计数与协议证据

负责人：协议工程师。生产依赖继续使用 `netease-music = 0.1.1`，不要先重写协议。

### 实现方式

1. 固定 `netease-music = 0.1.1`，审计其所有公开调用：每次调用发送一次
   HTTP；`playlist_track_all` 额外按每 500 个 track ID 发送一次 song-detail
   请求。生产代码据此分别输出 `client_calls` 与 `http_requests`。
2. 用 fake 对 0/1/500/501/大于 500 首分块行为做确定性计数测试；真实 sync
   同时记录两个计数，且不得记录 endpoint body、header、cookie 或响应正文。
3. 每条记录只包含：递增序号、逻辑 endpoint 名、weapi/eapi/linuxapi、HTTP 状态、耗时；不得记录 URL query、body、header、cookie 或响应正文。
4. 用 instrumented build 分别测量：
   - `status`；
   - 无缓存 playlist 的 preview；
   - 有缓存 playlist 的 preview；
   - sync 无新增；
   - sync 有 1 首新增；
   - playlist 超过 500 首时的详情分块。
5. 将生产 JSON 中的 `client_calls` 保留为包装层指标，`http_requests` 仅在
   固定 crate 版本及已审计组合方法下代表实际请求数；升级 crate 时必须重审。
6. 对照当前 `netease-music`、`ncm-api-rs` 和活跃 NeteaseCloudMusicApi 源码，记录 endpoint、加密模式、参数形状和差异；社区实现只作参考，最终结论必须来自本次真实响应。

### 验收

- 给出每种场景的真实 HTTP 总数与 endpoint 序列；
- 同一场景至少运行 3 次，报告范围和中位数；
- 证据不含账号数据或完整响应；
- 若 instrumented 副本行为与生产 crate 不一致，停止发布并先解释差异。

## 5. P0：最终安全版本真实闭环

负责人：一名能够扫码的验证人员和一名复核人员。使用普通非 VIP 账号，禁止粘贴 cookie。

### 执行顺序

1. 构建最终 release binary，并复制到固定验证路径；验证期间不再改代码。
2. 使用隔离目录执行 `freefm auth`，本人通过官方客户端扫码。
3. 退出进程后执行 `status --json`，确认 `authenticated=true`、`vipType=0`。
4. 执行 `preview --json`：
   - 人工复核所有 `add_original` 均具有完整严格免费证据；
   - `candidate_only` 不得出现在 `would_add_ids`；
   - 记录脱敏摘要后立即删除原始 preview 输出。
5. 执行第一次 `sync --quiet`：成功必须 exit 0 且 stdout/stderr 均为 0 bytes。
6. 通过 owned playlist 复读确认新增 ID 存在；确认没有删除、重排或修改原有歌曲。
7. 立即执行第二次 `sync --quiet`；确认没有再次调用 add，歌单 track count 不因重复项增长。
8. 将 session/state 权限、大小、修改时间和 release SHA-256 写入验证报告；不写内容。

### 停止条件

- 账号不是明确 `vipType=0`；
- 任一歌曲字段矛盾或 probe fee 缺失；
- 找到多个 owned 同名歌单；
- preview 计划自动加入 searched candidate；
- sync 后复读缺失，或出现删除/重排行为；
- 任一日志/回复出现 credential 或播放 URL。

### 验收

- 最终代码的真实创建/复用、append、复读、第二次幂等均有同一 release SHA 的证据；
- 只允许普通免费原曲自动加入；
- 安全版本的真实结果替代旧 Phase 2 写入证据。

## 6. P1：Private FM 与 session 长期实验

负责人：验证工程师。实验期间仍不调用播放、skip、trash、scrobble。

### Private FM 计划

1. 使用同一 session、独立进程，在 T0、+1m、+10m、+1h、+6h、+24h 执行只读 preview。
2. 额外安排 24 次整点运行，用于验证日内周期行为。
3. 原始 JSON 仅存于权限为 `600` 的临时目录；立即转换成：时间戳、批次大小、加盐 batch hash、与上一批交集数、累计新匿名 track 数、HTTP 状态和请求数，然后删除原始 JSON。
4. 用第 4 节 transport 记录证明整个实验中没有播放/skip/trash/scrobble endpoint。

判定规则：

- 24 小时内出现至少 2 个不同 batch，且后续批次仍出现此前未见的匿名 track，才可声称“被动采样可持续获得新推荐”；
- 连续 3 次完全相同或 24 小时没有新增时，不判定程序失败，但必须把“推荐可能停滞”作为产品限制，cron 不得承诺每次都有新增。

### Session 计划

1. `auth` 进程退出后立即、+1h、+24h、+7d 分别只运行 `status --json`；
2. 不通过 sync 的 cookie refresh 混淆 session 原始寿命实验；长期同步场景另行记录每次 refresh 后的寿命；
3. 在隔离验证 session 上由用户主动撤销登录，确认下一次 status/sync 返回稳定 `login_required`，不打印凭证；
4. 删除本地 session 文件只验证“本地缺失”，不能冒充“服务端撤销”。

### 验收

- 提交一张带真实时间戳的脱敏 observation 表；
- 明确区分跨进程恢复、跨天有效、服务端撤销和本地文件缺失；
- 任何无法完成的时点保持 pending，不用推断补齐。

## 7. P1：免费同录音自动替换研究门槛

负责人：协议工程师与人工标注复核人员。

### 决策流程

1. 研究当前真实 song detail、album detail、song wiki/扩展元数据及社区实现，寻找稳定的官方 recording ID、ISRC 或等价不可变录音标识。
2. 构建至少 30 组脱敏标注样本：
   - 至少 10 组确认同录音的不同合法发行；
   - 至少 20 组 hard negatives，包含同歌手重录、remaster、radio edit、live 场次、remix、翻唱、伴奏、Acoustic、sped-up、slowed、语言版和未知标签。
3. 只有同时满足以下条件才允许实现 `add_free_replacement`：
   - 原曲与候选共享同一个非空权威录音标识；
   - 完整主要歌手集合一致；
   - 规范化标题一致；
   - 时长差不超过 1.5 秒；
   - 版本标记与语言/演出类型一致；
   - 候选通过与原曲相同的严格普通账号免费判定；
   - 标注集上 false positive 为 0，且两名复核人一致批准。
4. 标题、歌手和时长本身永远不能解锁自动替换。
5. 如果网易云当前接口没有权威录音标识，v0.1 正式保留 `candidate_only`，README 明确宣布自动同曲替换延期；这属于安全的产品范围调整，不得用模糊评分绕过。

## 8. P1：OpenClaw、Hermes 与 0-token 验证

负责人：自动化/运维工程师。第 3、4、5、6 节通过后才开始。

1. 安装固定 release binary 到绝对路径；scheduler 不运行 Cargo。
2. 读取实际安装的 OpenClaw/Hermes 版本和官方 job schema，更新 `SKILL.md` 中的示例，禁止凭记忆猜配置字段。
3. OpenClaw 使用 deterministic command cron；Hermes 使用 no-agent/script-only cron。
4. 正常命令固定为：`/absolute/path/freefm sync --quiet`。
5. 每个宿主连续运行至少 2 次并验证：
   - exit 0；
   - stdout/stderr 为 0 bytes；
   - 没有 Agent run、模型调用或 token 记录；
   - FreeFM 进程退出后 RSS/CPU 为 0；
   - 两次运行不重复添加。
6. 失败只产生本地非零退出和人工提示；不要自动把每次成功结果发给 Agent。宿主无法直接运行 deterministic command 时，不上线该集成。
7. 默认周期暂定每小时一次；只有第 6 节长期实验支持该频率后才写入最终文档。

## 9. P2：发布、性能与交付

负责人：release owner。

1. 合并最终证据，删除或明确标记旧的、已被替代的 Phase 1/Phase 2 数字。
2. 执行最终门禁：format、24+ tests、Clippy、release、RustSec、secret scan、`git diff --check`。
3. 对最终 release 重测并记录：
   - binary bytes 与 SHA-256；
   - cold `status`、preview、无新增 sync、有新增 sync 的 wall time；
   - 各场景峰值 RSS；
   - 真实 HTTP 请求数；
   - session/state/lock 大小，以及 24 小时运行后的 state 增长。
4. 只有完整依赖在真实指标上明显不达标时，才评估裁剪 feature 或最小协议客户端；不要为了几 MB 重写协议。
5. 创建干净初始提交和 `v0.1.0` tag；提交前使用 `git status --ignored` 复核 `.freefm/`、QR、target 和验证临时文件均未跟踪。

## 10. Go/No-Go 清单

满足以下全部条件才能启用 unattended cron 并发布 v0.1：

- [x] P0 离线错误、crash window、并发进程和 CLI 合约测试全部通过；
- [x] 最小 session cookie 集经过隔离实测；
- [x] 最终 release 在普通账号上完成真实 append、复读和第二次幂等；
- [x] 固定 crate 的实际 HTTP 请求数已与 `client_calls` 分开计数并覆盖分块测试；
- [ ] 24 小时被动 FM 证据完成，限制如实记录；
- [ ] session 跨天与服务端撤销行为有证据；
- [x] 当前接口未发现权威录音 ID，v0.1 正式保持 candidate-only；
- [x] Hermes 真实 no-agent 手动周期证明 0 LLM token；
- [ ] 最终性能、短期 state 大小与依赖审计已完成；24 小时 state 增长仍待观察；
- [ ] Git 历史和 release tag 不含任何凭证或私人账号数据。

任一项未完成时的默认结论是：可继续人工 `preview`，但不宣称完整产品已完成，不启用无人值守 sync。

## 11. 建议排期与并行方式

- 第 1–2 个工程日：P0 fake/error/crash/锁测试、Unix 平台声明、RustSec、session 最小化。
- 第 2–3 个工程日：instrumented request counter、协议源码对照、最终 release 冻结。
- 第 3 个工程日：普通账号最终真实闭环；失败则回到 P0，不进入自动化。
- 第 3–4 个工程日并行启动：24 小时 FM 实验、session 时点实验、同录音身份研究。
- 第 5 个工程日：OpenClaw/Hermes 宿主验证、最终性能测量和文档整理。
- +7 天：补齐 session 7-day 观察后决定是否把长期 session 声明写入 v0.1。

推荐角色分工：一名 Rust/测试 owner、一名协议/实测 owner、一名自动化/release owner；真实扫码和撤销操作由账号持有人本人完成。
