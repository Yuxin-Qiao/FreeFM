# FreeFM × Codex

Codex 不是无模型定时器：`codex exec` 和桌面端循环自动化都会启动 Agent 回合并
消耗 LLM token，只用于交互式安装和故障诊断，绝不做例行同步。

零 token 的周期同步直接执行二进制：

- 系统 cron/launchd 调用 `freefm sync --quiet`（与 OpenClaw/Hermes 相同原则）；
- 或确定性沙箱运行器：`codex sandbox -- freefm sync --quiet`（需要允许网络和
  `~/.freefm` 写入的 permissions profile，先用 `status --json` 验证同一 profile）。

## 安装技能

本仓库 `skills/freefm` 目录本身就是 Codex skill（frontmatter 含
`name`/`description`），安装到 `~/.codex/skills/freefm` 后重启 Codex 即可。

```sh
install -d -m 700 "$HOME/.codex/skills"
cp -R skills/freefm "$HOME/.codex/skills/freefm"
```

也可以在 Codex 会话中请 skill installer 从 `Yuxin-Qiao/FreeFM` 的
`skills/freefm` 路径安装。
