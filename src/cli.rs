use crate::error::{AppError, AppResult};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Cli {
    pub command: String,
    pub json: bool,
    pub quiet: bool,
    pub data_dir: Option<PathBuf>,
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
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--json" => json_output = true,
                "--quiet" => quiet = true,
                "--data-dir" => {
                    data_dir = Some(PathBuf::from(args.next().ok_or_else(|| {
                        AppError::Usage("--data-dir 需要一个路径".to_string())
                    })?));
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
        Ok(Self {
            command,
            json: json_output,
            quiet,
            data_dir,
        })
    }
}

pub(crate) fn usage() -> String {
    "FreeFM\n\n用法：freefm <auth|preview|sync|audit|review|status|doctor|tui> [--json] [--quiet] [--data-dir PATH]\n\n\
auth     生成二维码并等待网易云官方客户端确认\n\
preview  读取私人 FM 并预览加入、候选、跳过；绝不写远端歌单\n\
sync     读取私人 FM，并 append-only 写入 FreeFM · Auto\n\
audit    只读复查 FreeFM · Auto 全部歌曲当前是否仍可免费完整播放\n\
review   交互式确认免费同曲候选，仅本机保存 trusted mapping\n\
status   检查本机会话、登录状态和本地运行统计\n\
doctor   检查本机状态、权限和 API 登录可用性
tui      打开轻量交互界面；不会改变命令的安全边界
version  输出版本号"
        .to_string()
}
