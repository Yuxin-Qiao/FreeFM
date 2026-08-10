mod tui;

use freefm::{AppError, Cli, Paths, VERSION, emit, json_error, run};
use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut cli = match Cli::parse(env::args()) {
        Ok(cli) => cli,
        Err(AppError::Help(message)) => {
            println!("{message}");
            return ExitCode::SUCCESS;
        }
        Err(AppError::Version) => {
            println!("FreeFM {VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    if cli.command == "tui" {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            eprintln!("tui 需要交互式终端；自动化请直接运行 freefm sync --quiet");
            return ExitCode::from(2);
        }
        let tui_data_dir = Paths::from_cli(&cli).root;
        match tui::choose(cli.json, Some(&tui_data_dir)) {
            Ok(Some(choice)) => {
                cli.command = choice.command.to_string();
                cli.json = choice.json;
                cli.quiet = choice.quiet;
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("无法打开 FreeFM TUI：{error}");
                return ExitCode::from(1);
            }
        }
    }
    if cli.command == "review" && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        eprintln!("review 需要交互式终端；自动化请直接运行 freefm audit --quiet");
        return ExitCode::from(2);
    }
    match run(cli.clone()) {
        Ok(value) => {
            let human = match cli.command.as_str() {
                "auth" => "登录成功，会话已保存到本机。".to_string(),
                "status" => {
                    if value["authenticated"] == true {
                        "已登录，会话可用。".to_string()
                    } else {
                        "未登录或会话已失效。".to_string()
                    }
                }
                "preview" => format!(
                    "预览完成：{} 首 FM 推荐，计划加入 {} 首。",
                    value["private_fm_count"],
                    value["would_add_ids"].as_array().map_or(0, Vec::len)
                ),
                "sync" => format!(
                    "同步完成：计划加入 {} 首，已验证歌单写入。",
                    value["would_add_ids"].as_array().map_or(0, Vec::len)
                ),
                "audit" => format!(
                    "审计完成：{} 首仍然免费；{} 首需要关注。audit 只读，未修改歌单。",
                    value["summary"]["still_free"],
                    value["summary"]["became_restricted"].as_i64().unwrap_or(0)
                        + value["summary"]["unavailable"].as_i64().unwrap_or(0)
                        + value["summary"]["unknown"].as_i64().unwrap_or(0)
                ),
                "review" => format!(
                    "review 完成：确认 {} 个映射，跳过 {} 个。",
                    value["approved_count"], value["skipped_count"]
                ),
                "doctor" => "doctor 检查完成。".to_string(),
                _ => "完成。".to_string(),
            };
            let attention = cli.command == "audit" && value["needs_attention"] == true;
            emit(&cli, &value, human, attention);
            if attention {
                ExitCode::from(3)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            let value = json_error(&error);
            emit(&cli, &value, error, true);
            ExitCode::from(1)
        }
    }
}
