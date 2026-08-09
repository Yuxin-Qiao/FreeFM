# FreeFM 中文操作指南

FreeFM 是一个 macOS/Linux 原生 Rust 命令行工具。它读取网易云音乐私人
FM，只把能够明确证明“普通非 VIP 账号可免费完整播放”的原歌曲追加到用户
自己的 `FreeFM · Auto` 歌单。

> 当前状态是实验性 alpha，FreeFM 不是网易云音乐官方项目。它依赖未公开的
> 接口行为，接口可能随时变化。请只操作自己的账号，并遵守平台条款和当地法律。

## 1. 安全边界

FreeFM 不会：

- 破解 VIP、解灰或绕过购买限制；
- 替换播放地址或下载音频；
- 删除、重排歌单歌曲；
- 自动使用搜索到的 Live、Remix、翻唱或其他疑似免费版本；
- 要求你把 Cookie、`MUSIC_U` 或 session 粘贴给 Agent。

受限歌曲的搜索结果只会作为 `candidate_only` 预览。无法严格确认时一律跳过。

## 2. 环境要求

- macOS 或 Linux；
- Rust/Cargo（从源码安装时需要）；
- 网易云音乐官方客户端，用于扫码登录；
- 普通非 VIP 网易云账号，`vipType` 必须明确为 `0`。

Windows 暂不支持。

## 3. 安装与更新

将 alpha 安装到 `~/.local/bin/freefm`：

```sh
cargo install --git https://github.com/Yuxin-Qiao/freefm \
  --locked \
  --root "$HOME/.local"
```

如果 shell 找不到命令，把目录加入 `PATH`：

```sh
export PATH="$HOME/.local/bin:$PATH"
```

确认安装：

```sh
freefm --version
freefm --help
```

更新时重新运行 `cargo install`，并增加 `--force`：

```sh
cargo install --git https://github.com/Yuxin-Qiao/freefm \
  --locked \
  --root "$HOME/.local" \
  --force
```

## 4. 首次扫码登录

在你能直接看到的终端中运行：

```sh
freefm auth
```

终端会显示二维码。使用网易云音乐官方客户端扫码并确认。二维码登录得到的凭证
只保存在本机 `~/.freefm/session.json`，程序不会把它打印出来。

登录后检查：

```sh
freefm status --json
```

正常结果应包含：

```json
{
  "authenticated": true,
  "account_vip_type": 0
}
```

如果不是 `vipType=0`，FreeFM 会拒绝同步，也不会创建或修改歌单。

## 5. 先预览，再同步

先执行只读预览：

```sh
freefm preview
freefm preview --json
```

常见决策：

- `add_original`：原歌曲已通过严格免费判定，可以追加；
- `candidate_only`：发现可能的免费版本，但不会自动使用；
- `skip`：VIP、购买、不可用、证据缺失或无法确认，跳过。

`preview` 不会创建歌单，也不会追加歌曲。

确认预览后执行真实同步：

```sh
freefm sync
```

第一次同步会查找或创建属于当前账号的 `FreeFM · Auto`。同步只追加，不删除、
不重排，也不修改用户手工加入的歌曲。再次执行会按远端歌曲 ID 去重：

```sh
freefm sync --quiet
```

正常无新增或追加成功时，`--quiet` 不输出任何内容。非零退出或可见错误表示需要
人工处理。

## 6. 诊断与自动化输出

```sh
freefm doctor
freefm doctor --json
freefm status --quiet
```

`--json` 适合自动化宿主解析。`--quiet` 优先于普通成功输出；失败仍会给出稳定、
脱敏的错误类别。

主要退出码：

- `0`：成功；
- `1`：登录、API、并发或人工处理错误；
- `2`：命令行参数错误。

## 7. OpenClaw 安装与零 Token 定时同步

### 安装 companion skill

ClawHub 页面：<https://clawhub.ai/yuxin-qiao/skills/freefm>

安装已公开的 alpha skill：

```sh
openclaw skills install @yuxin-qiao/freefm
```

也可以从 GitHub 安装仓库根 skill：

```sh
openclaw skills install git:Yuxin-Qiao/freefm@main --as freefm
```

### 创建 deterministic command job

先取得绝对路径：

```sh
command -v freefm
```

把输出替换到下面 JSON 数组的第一项：

```sh
openclaw automations create "0 * * * *" \
  --name "FreeFM hourly sync" \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver \
  --timeout-seconds 120
```

`--command-argv` 是 Gateway 内的 deterministic command payload，不会启动
Agent 或模型。不要改成 `--message`。

检查和手动验证：

```sh
openclaw automations list
openclaw automations run <job-id> --wait
openclaw automations runs --id <job-id>
```

OpenClaw Gateway 必须运行，计划任务才会触发。

## 8. Hermes 安装与零 Token 定时同步

可以把本仓库加入 Hermes tap：

```sh
hermes skills tap add Yuxin-Qiao/freefm
hermes skills install Yuxin-Qiao/freefm/freefm
```

也可以直接安装：

```sh
hermes skills install Yuxin-Qiao/freefm/skills/freefm
```

Hermes 0.17 当前可能只安装 `SKILL.md`，不会把 community GitHub/skills.sh
来源的 `scripts/` 一起下载。因此先检查安装目录；如果其中存在
`scripts/freefm-sync.sh`，直接安装它：

```sh
install -d -m 700 "$HOME/.hermes/scripts"
install -m 755 /path/to/freefm-skill/scripts/freefm-sync.sh \
  "$HOME/.hermes/scripts/freefm-sync.sh"
```

如果脚本不存在，从包含该脚本的不可变 Git commit 下载，并验证 SHA-256：

```sh
helper=$(mktemp)
curl -fsSL \
  https://raw.githubusercontent.com/Yuxin-Qiao/freefm/c7bcf10dce142fd85c84f82173a307e91ea99adc/skills/freefm/scripts/freefm-sync.sh \
  -o "$helper"
test "$(shasum -a 256 "$helper" | awk '{print $1}')" = \
  "b9dd3bd85e32c8ce57ba11ef474149839ad898090495daf7336d396d37830fd1"
install -d -m 700 "$HOME/.hermes/scripts"
install -m 755 "$helper" "$HOME/.hermes/scripts/freefm-sync.sh"
rm -f "$helper"
```

哈希不一致时立即停止，不要安装或执行该文件。

创建 no-agent cron：

```sh
hermes cron create "0 * * * *" \
  --name freefm-hourly \
  --script freefm-sync.sh \
  --no-agent
```

检查和手动运行：

```sh
hermes cron list
hermes cron run <job-id>
```

`--no-agent` 表示脚本本身就是任务。成功时脚本 stdout 为空，因此正常周期为
0 LLM token。

## 9. 本地状态与权限

默认目录：

```text
~/.freefm/
├── session.json
├── state.json
└── sync.lock
```

- 目录权限为 `700`；
- session/state 权限为 `600`；
- session 只保存当前验证所需的最小凭证；
- 不保存歌曲、封面、歌词、音频、完整 API 响应或长期运行日志。

不要备份、上传或提交 `~/.freefm/session.json`。怀疑泄漏时，应先在网易云官方
客户端撤销登录，再删除本地 session 并重新执行 `freefm auth`。

## 10. 常见问题

### `login_required`

session 缺失、过期或被撤销。重新执行：

```sh
freefm auth
```

### `ordinary_account_required`

当前账号不是明确的普通非 VIP 账号。v0.1 为避免语义混淆会拒绝写入。

### `api_incompatible`

网易云响应结构或接口行为发生变化。停止定时任务，保留脱敏错误类别，并到
[GitHub Issues](https://github.com/Yuxin-Qiao/freefm/issues) 报告；不要附带
Cookie、账号 ID、播放 URL 或完整响应。

### `concurrent_sync`

另一个 preview/sync 正在运行。等待它退出后重试，不要删除正在使用的锁文件。

### 同名歌单冲突

如果当前账号拥有多个名为 `FreeFM · Auto` 的歌单，FreeFM 会 fail-closed。
请在官方客户端人工重命名或整理，程序不会随机选择。

### 为什么搜索到免费候选却不加入？

歌名、歌手、时长和专辑信息不能可靠证明是同一录音。当前接口没有验证到稳定的
ISRC 或 recording ID，因此 v0.1 只展示候选，不自动替换。

## 11. 暂停、移除与清理

删除 OpenClaw job：

```sh
openclaw automations rm <job-id>
```

暂停或删除 Hermes job：

```sh
hermes cron pause <job-id>
hermes cron remove <job-id>
```

卸载二进制：

```sh
cargo uninstall --root "$HOME/.local" freefm
```

确认不再需要登录状态后，可以删除本地 FreeFM 数据目录。删除前如有泄漏疑虑，
先在网易云官方客户端撤销对应登录。

## 12. 当前限制

- 只支持网易云音乐；
- 只支持 macOS/Linux；
- 免费同录音候选不会自动替换；
- 网易云接口没有官方稳定性保证；
- 长期被动 FM 与 session 验证仍在持续进行；
- 当前版本是 alpha，不应宣传为网易云官方或稳定生产服务。
