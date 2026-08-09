<div align="center">

# 🎧 FreeFM

**把私人 FM 里真正免费可播的歌，安静地收进一张歌单。**

原生 Rust · 官方客户端扫码 · 严格免费判定 · 只追加不破坏

[![CI](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml/badge.svg)](https://github.com/Yuxin-Qiao/FreeFM/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-native-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![平台](https://img.shields.io/badge/平台-macOS%20%7C%20Linux-222)](#安装)
[![License](https://img.shields.io/badge/license-MIT-6E56CF)](LICENSE)
[![ClawHub](https://img.shields.io/badge/ClawHub-FreeFM-13B8A6)](https://clawhub.ai/yuxin-qiao/skills/freefm)

[快速开始](#快速开始) · [TUI](#tui) · [AI 帮装](#让-ai-帮你安装) ·
[Agent 平台](#agent-平台) · [English](README.md)

</div>

![FreeFM：把私人 FM 收进一张干净的免费歌单](assets/freefm-hero.svg)

> [!IMPORTANT]
> FreeFM 是独立社区项目，不是网易云音乐官方产品。它不会破解 VIP、解灰、
> 替换播放地址或下载音频。请只操作自己的账号。

## FreeFM 做什么

FreeFM 读取网易云私人 FM，只把“普通账号有明确正证据可以免费完整播放”的原歌曲
追加到本人拥有的 `FreeFM · Auto`。字段缺失、格式异常或互相矛盾，一律跳过。
搜索到的相似免费发行只在预览中展示；v0.1 不会用 Live、Remix、翻唱、Edit 或重录
偷偷替换原曲。

`preview` 永远只读，只有 `sync` 写歌单；不删除、不重排。重复或并发运行按远端 ID
去重。每次执行后立即退出，空闲时 0 进程、0 RAM、0 CPU。

## 安装

目前支持 macOS 和 Linux：

```sh
cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked
freefm --version
```

Homebrew tap 会随首个稳定 tag 开放；实验 alpha 阶段请使用上面的可复现 Cargo 安装。

## 快速开始

```sh
freefm tui          # 推荐：终端交互界面

# 或直接使用命令
freefm auth         # 用网易云官方客户端扫码
freefm preview      # 只读预览
freefm sync         # 只追加写入
freefm sync --quiet # 定时任务；成功时无输出
```

任何时候都不要把 Cookie、`MUSIC_U`、session 或二维码 key 发给 AI。凭证只保存在
`~/.freefm/`，不会写日志；程序也不缓存音乐、封面、歌词或播放 URL。

## TUI

`freefm tui` 是原生 Rust 终端菜单，包含登录、预览、同步、状态和诊断。方向键或
`j`/`k` 选择，`o` 切换文本/JSON，`q` 退出。同步必须明确按 `y`；Enter 默认取消。
自动化固定调用非交互命令 `freefm sync --quiet`。

## 让 AI 帮你安装

把下面整段交给能操作终端的 AI：

```text
请在这台 macOS 或 Linux 电脑安装 FreeFM：
https://github.com/Yuxin-Qiao/FreeFM

先阅读 AGENTS.md 和 README.zh-CN.md。禁止索取、读取、打印或上传网易云 Cookie、
MUSIC_U、session 和二维码 key。使用
`cargo install --git https://github.com/Yuxin-Qiao/FreeFM --locked` 安装。
让我本人在可见终端运行 `freefm auth` 并扫码。先运行 `freefm preview`；未经我确认，
不要运行 sync 或修改定时任务。定时同步必须直接执行 `freefm sync --quiet`，不能
启动 Agent/LLM。遇到权限、登录或接口错误时只给脱敏提示，不要改 DNS、VPN、代理。
```

根目录 [`AGENTS.md`](AGENTS.md) 用于约束“修改项目代码的 AI”；上面的 Prompt 用于
普通用户安装，两者职责不同。

## 命令

| 命令 | 远端写入 | 用途 |
|---|---:|---|
| `freefm auth` | 不写歌单 | 官方客户端扫码登录 |
| `freefm preview` | 否 | 展示加入、候选和跳过 |
| `freefm sync` | 只追加 | 加入严格验证的免费原曲 |
| `freefm status` | 否 | 检查 session 与账号 |
| `freefm doctor` | 否 | 检查权限、状态和接口结构 |
| `freefm tui` | 取决于选择 | 上述命令的交互入口 |

`--json` 提供稳定机器输出，`--quiet` 在成功时静默；`--data-dir PATH` 或
`FREEFM_HOME` 可指定隔离状态目录。

## Agent 平台

真正的产品始终是 Rust 二进制；Skill 只负责安装和确定性调用。正常定时同步不需要
Agent，也不消耗 LLM token。

| 平台 | 安装 | 无模型定时路径 | 已有证据 |
|---|---|---|---|
| 🦞 **OpenClaw** | `openclaw skills install @yuxin-qiao/freefm` | Gateway `--command-argv` | 隔离实跑退出 0、输出为空 |
| 🪽 **Hermes** | `hermes skills install Yuxin-Qiao/FreeFM/skills/freefm` | `--no-agent` script | 实跑并通过 `SAFE / ALLOWED` |
| 🤖 **腾讯 WorkBuddy** | 上传 `freefm-workbuddy.zip` | 本地命令能力 | 签名客户端已安装，登录后完成导入验证 |

### 🦞 OpenClaw

```sh
openclaw automations add --every 1h --name freefm-hourly \
  --command-argv '["/absolute/path/to/freefm","sync","--quiet"]' \
  --no-deliver --timeout-seconds 120
```

隔离 Gateway 已真实执行该类型任务：退出码 0、stdout/stderr 为空、未请求消息投递，
随后验证任务与 Gateway 均已关闭。

### 🪽 Hermes

安装 Skill 后使用仓库提供的 `freefm-sync.sh` 创建 `--no-agent` script cron。
Hermes 0.17 的实际运行记录已确认 `Mode: no_agent (script)` 与静默成功。

### 🤖 腾讯 WorkBuddy

```sh
scripts/package-workbuddy.sh
# target/freefm-workbuddy.zip
```

在“专家·技能·连接器 → 添加技能 → 上传技能”中选择 ZIP。包内只有 `SKILL.md` 和
确定性 helper，结构遵循腾讯的[本地技能上传文档](https://cloud.tencent.com/document/product/1831/134432)。

长期真实验证完成前，不要开启无人值守同步。

## 安全边界

FreeFM 只接受明确的 `vipType == 0`，且 privilege 与官方播放能力信息同时证明：
fee 为数值 0、播放能力可用、内存中 URL 非空、没有免费试用标记。URL 不打印、
不持久化、不下载、不替换。出现多个本人拥有的同名歌单会 fail-closed；本地锁避免
并发创建和追加。

常见错误：`login_required` 请重新 `auth`；`ordinary_account_required` 表示账号不
满足普通账号要求；`api_incompatible` 表示接口结构变化，应暂停定时任务并提交脱敏
Issue；`concurrent_sync` 表示已有任务运行，等待结束即可。

## 卸载与数据

先删除平台定时任务，再执行 `cargo uninstall freefm`。确认不再需要登录状态后删除
`~/.freefm/`；怀疑泄漏时，先在网易云官方客户端撤销登录。

## 当前验证状态

- 普通账号扫码、进程重启 session 恢复、真实 append-only 写入与第二次幂等已验证；
- 当前实测一次同步 19 个 HTTP 请求、4.02 秒、峰值 RSS 15,269,888 字节；
- 被动读取可在不调用 play/skip/trash/scrobble 时得到变化批次，但 24 小时和 7 天门槛仍在运行；
- session 服务端过期/撤销后的重新认证仍需单独验证；
- WorkBuddy 客户端已安装并通过签名校验，实际导入等待用户本人登录；
- 首个稳定 tag 与 Homebrew tap 以长期验证门槛通过为发布条件。

完整接口、测试、性能、平台实跑及剩余限制见
[`V01-VALIDATION.md`](V01-VALIDATION.md)。开发与安全规范见
[`AGENTS.md`](AGENTS.md)、[`CONTRIBUTING.md`](CONTRIBUTING.md) 和
[`SECURITY.md`](SECURITY.md)。

<div align="center">

**少做一点，但每一步都能解释、能复现、能安全退出。**

[MIT License](LICENSE)

</div>
