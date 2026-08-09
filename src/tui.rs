use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use std::io::{self, Write};
use std::path::Path;

pub(crate) struct Choice {
    pub(crate) command: &'static str,
    pub(crate) json: bool,
}

#[derive(Clone, Copy)]
struct MenuItem {
    title: &'static str,
    command: Option<&'static str>,
    description: &'static str,
    writes_remote: bool,
}

const ITEMS: &[MenuItem] = &[
    MenuItem {
        title: "扫码登录",
        command: Some("auth"),
        description: "使用网易云官方客户端扫码，凭证只保存在本机",
        writes_remote: false,
    },
    MenuItem {
        title: "预览本次推荐",
        command: Some("preview"),
        description: "只读检查将加入、候选和跳过的歌曲",
        writes_remote: false,
    },
    MenuItem {
        title: "同步到 FreeFM · Auto",
        command: Some("sync"),
        description: "仅追加严格确认可免费完整播放的原歌曲",
        writes_remote: true,
    },
    MenuItem {
        title: "查看登录状态",
        command: Some("status"),
        description: "检查本机会话与普通账号状态",
        writes_remote: false,
    },
    MenuItem {
        title: "运行诊断",
        command: Some("doctor"),
        description: "检查目录、权限、会话与接口可用性",
        writes_remote: false,
    },
    MenuItem {
        title: "退出",
        command: None,
        description: "不执行任何操作",
        writes_remote: false,
    },
];

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    }
}

pub(crate) fn choose(initial_json: bool, data_dir: Option<&Path>) -> io::Result<Option<Choice>> {
    let _guard = TerminalGuard::enter()?;
    let mut selected = 0;
    let mut json = initial_json;

    loop {
        draw(selected, json, data_dir, false)?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => selected = previous_index(selected),
            KeyCode::Down | KeyCode::Char('j') => selected = next_index(selected),
            KeyCode::Char('o') => json = !json,
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Enter => {
                let item = ITEMS[selected];
                let Some(command) = item.command else {
                    return Ok(None);
                };
                if item.writes_remote && !confirm_sync(selected, json, data_dir)? {
                    continue;
                }
                return Ok(Some(Choice { command, json }));
            }
            _ => {}
        }
    }
}

fn previous_index(index: usize) -> usize {
    index.checked_sub(1).unwrap_or(ITEMS.len() - 1)
}

fn next_index(index: usize) -> usize {
    (index + 1) % ITEMS.len()
}

fn confirm_sync(selected: usize, json: bool, data_dir: Option<&Path>) -> io::Result<bool> {
    loop {
        draw(selected, json, data_dir, true)?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(confirmed) = sync_confirmation(key.code) {
            return Ok(confirmed);
        }
    }
}

fn sync_confirmation(key: KeyCode) -> Option<bool> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(true),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => Some(false),
        _ => None,
    }
}

fn draw(selected: usize, json: bool, data_dir: Option<&Path>, confirm: bool) -> io::Result<()> {
    let mut output = io::stdout();
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let divider = "─".repeat(usize::from(width.clamp(36, 88)));
    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    queue!(
        output,
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        Print("  FreeFM  私人 FM → 免费歌单\r\n"),
        ResetColor,
        SetAttribute(Attribute::Reset),
        SetForegroundColor(Color::DarkGrey),
        Print(format!("  {divider}\r\n")),
        ResetColor,
        Print("  ↑/↓ 选择  Enter 执行  o 切换输出  q 退出\r\n\r\n")
    )?;

    for (index, item) in ITEMS.iter().enumerate() {
        if index == selected {
            queue!(
                output,
                SetForegroundColor(Color::Cyan),
                SetAttribute(Attribute::Bold),
                Print(format!("  › {}\r\n", item.title)),
                SetAttribute(Attribute::Reset),
                SetForegroundColor(Color::DarkGrey),
                Print(format!("    {}\r\n", item.description)),
                ResetColor
            )?;
        } else {
            queue!(
                output,
                Print(format!("    {}\r\n", item.title)),
                SetForegroundColor(Color::DarkGrey),
                Print(format!("    {}\r\n", item.description)),
                ResetColor
            )?;
        }
    }

    let data_dir = data_dir
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "~/.freefm".to_string());
    queue!(
        output,
        Print("\r\n"),
        SetForegroundColor(Color::DarkGrey),
        Print(format!(
            "  输出：{}   数据目录：{}\r\n",
            if json { "JSON" } else { "易读文本" },
            data_dir
        )),
        ResetColor
    )?;

    if confirm {
        queue!(
            output,
            Print("\r\n"),
            SetForegroundColor(Color::Yellow),
            SetAttribute(Attribute::Bold),
            Print("  确认执行 append-only 同步？按 y 继续，Enter 取消 [y/N] "),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
    }
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_navigation_wraps() {
        assert_eq!(previous_index(0), ITEMS.len() - 1);
        assert_eq!(next_index(ITEMS.len() - 1), 0);
    }

    #[test]
    fn only_sync_is_remote_write() {
        let writes: Vec<_> = ITEMS
            .iter()
            .filter(|item| item.writes_remote)
            .filter_map(|item| item.command)
            .collect();
        assert_eq!(writes, vec!["sync"]);
    }

    #[test]
    fn sync_requires_explicit_yes() {
        assert_eq!(sync_confirmation(KeyCode::Char('y')), Some(true));
        assert_eq!(sync_confirmation(KeyCode::Char('Y')), Some(true));
        assert_eq!(sync_confirmation(KeyCode::Enter), Some(false));
        assert_eq!(sync_confirmation(KeyCode::Esc), Some(false));
        assert_eq!(sync_confirmation(KeyCode::Char('x')), None);
    }
}
