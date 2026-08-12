use crate::error::{AppError, AppResult};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Cli {
    pub command: String,
    pub json: bool,
    pub quiet: bool,
    pub data_dir: Option<PathBuf>,
    pub max_additions: Option<usize>,
    pub source: Option<String>,
    pub target: Option<String>,
}

impl Cli {
    pub fn parse<I>(args: I) -> AppResult<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = args.into_iter();
        let _program = args.next();
        let command = args.next().unwrap_or_default();
        if command.is_empty() {
            return Err(AppError::Help(usage()));
        }
        if command == "--help" || command == "-h" {
            return Err(AppError::Help(usage()));
        }
        if command == "--version" || command == "version" {
            return Err(AppError::Version);
        }
        let mut json_output = false;
        let mut quiet = false;
        let mut data_dir = None;
        let mut max_additions = None;
        let mut source = None;
        let mut target = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--json" => json_output = true,
                "--quiet" => quiet = true,
                "--data-dir" => {
                    data_dir = Some(PathBuf::from(args.next().ok_or_else(|| {
                        AppError::Usage("--data-dir 需要一个路径".to_string())
                    })?));
                }
                "--max-additions" => {
                    let raw = args.next().ok_or_else(|| {
                        AppError::Usage("--max-additions 需要一个正整数".to_string())
                    })?;
                    let value = raw.parse::<usize>().map_err(|_| {
                        AppError::Usage("--max-additions 必须是 1 到 10000 的整数".to_string())
                    })?;
                    if !(1..=10_000).contains(&value) {
                        return Err(AppError::Usage(
                            "--max-additions 必须是 1 到 10000 的整数".to_string(),
                        ));
                    }
                    max_additions = Some(value);
                }
                "--source" => {
                    source = Some(args.next().ok_or_else(|| {
                        AppError::Usage(
                            "--source 需要一个 Spotify、Apple Music 或 YouTube Music 歌单 URL"
                                .to_string(),
                        )
                    })?);
                }
                "--target" => {
                    target = Some(args.next().ok_or_else(|| {
                        AppError::Usage(
                            "--target 需要一个 Spotify、Apple Music 或 YouTube Music 歌单 URL"
                                .to_string(),
                        )
                    })?);
                }
                "--help" | "-h" => return Err(AppError::Help(usage())),
                other => return Err(AppError::Usage(format!("未知参数：{other}\n\n{}", usage()))),
            }
        }
        if quiet && matches!(command.as_str(), "auth" | "tui" | "review") {
            return Err(AppError::Usage(format!(
                "{command} 需要交互式终端，不能使用 --quiet"
            )));
        }
        if max_additions.is_some() && !matches!(command.as_str(), "preview" | "sync") {
            return Err(AppError::Usage(
                "--max-additions 仅支持 preview 和 sync".to_string(),
            ));
        }
        if source.is_some()
            && !matches!(
                command.as_str(),
                "preview" | "sync" | "review" | "doctor" | "tui"
            )
        {
            return Err(AppError::Usage(
                "--source 仅支持 preview、sync、review、doctor 和 tui".to_string(),
            ));
        }
        if target.is_some() && !matches!(command.as_str(), "sync" | "review" | "doctor") {
            return Err(AppError::Usage(
                "--target 仅支持 sync、review 和 doctor".to_string(),
            ));
        }
        if target.is_some() && source.is_none() && matches!(command.as_str(), "sync" | "review") {
            return Err(AppError::Usage(
                "--target 需要同时提供 --source 外部歌单 URL".to_string(),
            ));
        }
        Ok(Self {
            command,
            json: json_output,
            quiet,
            data_dir,
            max_additions,
            source,
            target,
        })
    }
}

pub(crate) fn usage() -> String {
    [
        "FreeFM",
        "",
        "用法：freefm <auth|preview|sync|audit|review|status|doctor|tui> [--json] [--quiet] [--data-dir PATH] [--max-additions N] [--source URL] [--target URL]",
        "",
        "auth     生成二维码并等待网易云官方客户端确认",
        "preview  读取私人 FM，或用 --source 读取外部歌单并预览；绝不写远端歌单",
        "sync     读取私人 FM，或用 --source 同步外部歌单；--target 可追加到已验证外部目标歌单",
        "audit    只读复查 FreeFM · Auto 全部歌曲当前是否仍可免费完整播放",
        "review   交互式确认免费同曲或跨平台候选；--source/--target 可保存外部映射",
        "status   检查本机会话、登录状态和本地运行统计",
        "doctor   检查本机状态、权限和 API 登录可用性；--source/--target 检查外部凭证配置",
        "tui      打开轻量交互界面；不会改变命令的安全边界",
        "version  输出版本号",
    ]
    .join("\n")
}
