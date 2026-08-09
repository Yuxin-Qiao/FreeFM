<div align="center">

# 🎧 FreeFM

**把私人 FM 里真正免费可播的歌，安静地收进一张歌单。**

网易云音乐专用 · 原生 Rust · 扫码登录 · 严格免费判定 · 只追加不破坏

[![CI](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml/badge.svg)](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-native-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![平台](https://img.shields.io/badge/平台-macOS%20%7C%20Linux-222)](#安装)
[![License](https://img.shields.io/badge/license-MIT-6E56CF)](LICENSE)
[![ClawHub](https://img.shields.io/badge/ClawHub-FreeFM-13B8A6)](https://clawhub.ai/yuxin-qiao/skills/freefm)

[快速开始](#三分钟开始) · [交互界面](#轻量-tui) · [AI 帮装](#让-ai-帮你安装) ·
[自动化平台](#openclawhermesworkbuddy) · [English](README.md)

</div>

![FreeFM：把私人 FM 收进一张干净的免费歌单](assets/freefm-hero.svg)

> [!IMPORTANT]
> FreeFM 是独立社区项目，不是网易云音乐官方产品。它不会破解 VIP、解灰、
> 替换播放地址或下载音频。请只操作自己的账号，并遵守平台条款和当地法律。

## 一眼看懂

```text
网易云官方客户端扫码
          ↓
      私人 FM 推荐
          ↓
普通账号免费完整播放的严格正证据
          ↓
  FreeFM · Auto 专属歌单
      只追加 · 不删除 · 不重排
```

| 你关心的事 | FreeFM 怎么做 |
|---|---|
| 会不会偷偷解灰？ | 不会。受限歌曲直接跳过。 |
| 会不会拿 Live/Remix 顶替？ | 不会。搜索候选只展示，不自动加入。 |
| 会不会弄乱我的歌单？ | 不会。只向本人拥有的目标歌单追加。 |
| 重复运行会不会重复加歌？ | 不会。按远端歌曲 ID 去重，并用本地锁防并发。 |
| 平时占不占资源？ | 不占。每次执行后退出，空闲时 0 进程。 |
| Cookie 要不要发给 AI？ | 永远不要。只在本机终端扫码。 |

## 安装

目前支持 macOS 和 Linux，需要先安装 Rust/Cargo：

```sh
cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked
freefm --version
```

预期输出：

```text
FreeFM 0.1.0
```

### 为什么现在没有 Homebrew？

这是刻意的，不是漏做。按照 [Homebrew 官方 tap 规范](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)，
可靠安装需要稳定 tag、固定 SHA-256、持续维护的 `homebrew-tap` 和
macOS/Linux 构建验证。FreeFM 仍处于实验 alpha，
此时塞进一个只会从 `main` 拉源码的 Formula，安全性和可复现性都不如上面的
Cargo 命令。仓库已经为 tag 自动构建原生压缩包和 SHA-256；等首个稳定 release
产物固定后再提供：

```text
brew install Yuxin-Qiao/tap/freefm   # 规划中，目前不要执行
```

## 三分钟开始

### 方式一：用交互菜单

```sh
freefm tui
```

### 方式二：直接运行命令

```sh
freefm auth       # 在可见终端扫码
freefm preview    # 只读预览
freefm sync       # 确认后才执行写入
freefm sync --quiet
```

`--quiet` 的正常成功路径完全无输出，适合 cron、OpenClaw 和 Hermes。失败、登录
失效或接口不兼容仍会输出稳定且脱敏的错误。

## 轻量 TUI

FreeFM 现在提供原生 Rust 终端界面：

```text
  FreeFM  私人 FM → 免费歌单
  ─────────────────────────────────────
  ↑/↓ 选择  Enter 执行  o 切换输出  q 退出

  › 扫码登录
    预览本次推荐
    同步到 FreeFM · Auto
    查看登录状态
    运行诊断
    退出
```

- 方向键或 `j` / `k` 选择；
- `o` 切换易读文本与 JSON；
- `q` 或 `Esc` 退出；
- 选择同步后必须明确按 `y`；Enter、`n` 或 Esc 都会取消；
- TUI 只是现有命令的界面，不复制协议逻辑，也不改变安全边界。

自动化不要调用 TUI，固定使用：

```sh
freefm sync --quiet
```

## 让 AI 帮你安装

可以把下面整段交给 Codex、Claude Code、CodeBuddy 或其他能操作终端的 AI：

```text
请在这台 macOS 或 Linux 电脑上安装 FreeFM：
https://github.com/Yuxin-Qiao/FreeFM

开始前先阅读仓库的 AGENTS.md 和 README.zh-CN.md。不要让我粘贴网易云
Cookie、MUSIC_U、session 或二维码 key；不要读取、打印或上传这些凭证。
使用 cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked 安装。
安装后让我本人在可见终端运行 freefm auth，并用网易云官方客户端扫码。
先运行 freefm preview；没有得到我确认前，不要运行 sync、创建歌单或添加定时任务。
如果配置自动同步，只能直接执行 freefm sync --quiet，不能启动 Agent/LLM。
遇到权限、登录或接口错误时停止并给我脱敏提示，不要修改 DNS、VPN、代理或镜像。
```

仓库根目录的 [`AGENTS.md`](AGENTS.md) 是给“修改这个项目代码的 AI”看的；上面
这段 Prompt 是给“帮普通用户安装的 AI”看的。两者不能互相替代。

## 命令速查

| 命令 | 是否写远端歌单 | 作用 |
|---|---:|---|
| `freefm auth` | 否 | 生成二维码并等待官方客户端确认 |
| `freefm preview` | 否 | 展示将加入、候选和跳过的歌曲 |
| `freefm sync` | 只追加 | 更新 `FreeFM · Auto` 并复读确认 |
| `freefm status` | 否 | 检查 session 与账号状态 |
| `freefm doctor` | 否 | 检查目录、权限、登录和接口结构 |
| `freefm tui` | 取决于选择 | 上述命令的可视化入口 |

全局选项：

```text
--json             稳定机器输出
--quiet            成功时静默
--data-dir PATH    使用隔离状态目录
FREEFM_HOME=PATH   通过环境变量指定状态目录
```

## 免费判定到底有多严格？

FreeFM 只在以下信息互相一致时加入原歌曲：

- 当前账号明确是普通账号，`vipType == 0`；
- privilege 的 fee 明确是数值 `0`；
- privilege 没有不可用状态，播放能力字段为正；
- 官方播放能力接口明确返回 fee `0`；
- URL 非空，但只作为内存中的能力证据；
- 没有免费试用标记。

字段缺失、类型错误、响应未知或互相冲突，一律归为 `unknown` 并跳过。播放 URL
不会打印、保存、替换或下载。

搜索到的“同名免费版本”也不会自动加入。歌名、歌手、时长和专辑不能证明是同一
录音；Live、Remix、翻唱、伴奏、Acoustic、sped-up、slowed、重录和 radio edit
尤其容易误判，所以 v0.1 只显示 `candidate_only`。

## OpenClaw、Hermes、WorkBuddy

FreeFM 本体始终是 Rust 二进制。Skill 只负责安装、说明和调度，不承载音乐协议。

| 平台 | 安装方式 | 正常定时路径 | 当前证据 |
|---|---|---|---|
| OpenClaw | ClawHub 安装 | deterministic command | 已实装并显示 `ready` |
| Hermes | GitHub/skills.sh | `--no-agent` script | 已审计 `SAFE / ALLOWED` |
| 腾讯 WorkBuddy | 上传本地 ZIP Skill | 本地命令能力 | 格式/内容已离线验证，客户端待实装 |

### OpenClaw

```sh
openclaw skills install @yuxin-qiao/freefm
```

定时任务必须使用 deterministic command，而不是 Agent message：

```sh
openclaw automations create "0 * * * *" \
  --name "FreeFM hourly sync" \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver \
  --timeout-seconds 120
```

### Hermes

```sh
hermes skills tap add Yuxin-Qiao/FreeFM
hermes skills install Yuxin-Qiao/FreeFM/skills/freefm
```

仓库提供固定命令 helper。Hermes 0.17 如果只安装了 `SKILL.md`，按照
[`skills/freefm/SKILL.md`](skills/freefm/SKILL.md) 内的不可变 commit URL 和
SHA-256 校验回退安装，校验不一致时立即停止。

```sh
hermes cron create "0 * * * *" \
  --name freefm-hourly \
  --script freefm-sync.sh \
  --no-agent
```

### 腾讯 WorkBuddy

[腾讯 WorkBuddy 官方文档](https://cloud.tencent.com/document/product/1831/134432)
支持在“技能”页面上传本地技能包。生成包：

```sh
git clone https://github.com/Yuxin-Qiao/FreeFM
cd FreeFM
scripts/package-workbuddy.sh
```

然后在 WorkBuddy 的“专家·技能·连接器 → 添加技能 → 上传技能”中选择：

```text
target/freefm-workbuddy.zip
```

包内只包含：

```text
freefm/
├── SKILL.md
└── scripts/freefm-sync.sh
```

当前仓库已在 CI 检查 ZIP 结构和脚本语法，但还没有腾讯 WorkBuddy 客户端实际导入
记录，因此只能称为“实验兼容”，不能称为已上架或官方认证。

## 本地数据与隐私

```text
~/.freefm/
├── session.json   600
├── state.json     600
└── sync.lock      600
```

目录权限为 `700`。FreeFM 不缓存音乐、封面、歌词、完整 API 响应或长期日志。
二维码只显示在终端，不写入文件。

不要把 `~/.freefm/session.json` 发到聊天、Issue 或 PR。怀疑泄漏时，先在网易云
官方客户端撤销登录，再删除本地 session 并重新扫码。

## 常见问题

<details>
<summary><code>login_required</code></summary>

session 缺失、失效或被撤销。重新运行 `freefm auth`。

</details>

<details>
<summary><code>ordinary_account_required</code></summary>

当前账号不是明确的普通非 VIP 账号。v0.1 会在任何写入前拒绝继续。

</details>

<details>
<summary><code>api_incompatible</code></summary>

网易云接口结构发生变化。先暂停定时任务，再提交脱敏 Issue；不要附带 Cookie、
账号 ID、播放 URL 或完整响应。

</details>

<details>
<summary><code>concurrent_sync</code></summary>

已有 preview/sync 正在运行。等待退出后重试，不要删除正在使用的锁文件。

</details>

<details>
<summary>出现多个同名歌单怎么办？</summary>

FreeFM 会 fail-closed，不会随机选择。请在官方客户端人工重命名或只保留一个本人
拥有的 `FreeFM · Auto`。

</details>

## 卸载

先删除平台定时任务，再卸载二进制：

```sh
cargo uninstall freefm
```

确认不再需要登录状态后，可以删除 `~/.freefm/`。如果担心凭证泄漏，先在网易云
官方客户端撤销登录。

## 当前状态

- 只支持网易云音乐、macOS 和 Linux；
- 当前版本是实验 alpha；
- FreeFM 依赖未公开接口，网易云更新后可能需要适配；
- 短期被动 FM 变化和 session 重启恢复已有真实证据；长期观察仍在继续；
- WorkBuddy 客户端导入和稳定 Homebrew release 尚未完成。

真实接口、请求数、性能数据、session 结论和仍未关闭的验证项见
[`V01-VALIDATION.md`](V01-VALIDATION.md)。开发规范见 [`AGENTS.md`](AGENTS.md)，
安全报告方式见 [`SECURITY.md`](SECURITY.md)。

---

<div align="center">

**少做一点，但每一步都能解释、能复现、能安全退出。**

FreeFM 使用 [MIT License](LICENSE) 开源。

</div>
