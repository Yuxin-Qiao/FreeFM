use crate::VERSION;
use netease_music::NeteaseError;
use std::fmt::{self, Display};
use std::io;

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    #[allow(dead_code)]
    Usage(String),
    Help(String),
    Version,
    LoginRequired,
    OrdinaryAccountRequired,
    ConcurrentSync,
    AmbiguousPlaylist,
    StateCorrupt(String),
    ApiIncompatible(String),
    SourceUrlInvalid(String),
    SourceAuthRequired(String),
    SourceApiIncompatible(String),
    SourceTimeout,
    SourceRemote(String),
    TargetUrlInvalid(String),
    TargetAuthRequired(String),
    TargetApiIncompatible(String),
    TargetTimeout,
    TargetRemote(String),
    TargetWriteUncertain(String),
    TargetNotOwned(String),
    TargetReadOnly(String),
    TargetMappingRequired(String),
    Timeout,
    Remote(String),
    Io(io::Error),
    Json(serde_json::Error),
    Netease(NeteaseError),
}

impl Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::Help(message) => write!(f, "{message}"),
            Self::Version => write!(f, "FreeFM {VERSION}"),
            Self::LoginRequired => write!(f, "登录已失效或尚未登录；请运行 freefm auth"),
            Self::OrdinaryAccountRequired => write!(
                f,
                "当前账号不是已确认的普通非 VIP 账号；为避免误加会员歌曲，请使用普通账号重新登录"
            ),
            Self::ConcurrentSync => write!(f, "已有另一个 FreeFM 进程正在同步，请稍后重试"),
            Self::AmbiguousPlaylist => write!(
                f,
                "发现多个同名且属于当前账号的 FreeFM · Auto 歌单；请人工保留一个后重试"
            ),
            Self::StateCorrupt(message) => write!(f, "本机状态文件损坏：{message}"),
            Self::ApiIncompatible(message) => write!(f, "网易云接口返回格式不兼容：{message}"),
            Self::SourceUrlInvalid(message) => write!(f, "外部歌单 URL 不受支持：{message}"),
            Self::SourceAuthRequired(message) => write!(f, "外部平台需要凭证：{message}"),
            Self::SourceApiIncompatible(message) => {
                write!(f, "外部平台接口返回格式不兼容：{message}")
            }
            Self::SourceTimeout => write!(f, "外部平台接口请求超时，请稍后重试"),
            Self::SourceRemote(message) => write!(f, "外部平台接口请求失败：{message}"),
            Self::TargetUrlInvalid(message) => write!(f, "目标歌单 URL 不受支持：{message}"),
            Self::TargetAuthRequired(message) => write!(f, "目标平台需要写入凭证：{message}"),
            Self::TargetApiIncompatible(message) => {
                write!(f, "目标平台接口返回格式不兼容：{message}")
            }
            Self::TargetTimeout => write!(f, "目标平台接口请求超时，请稍后重试"),
            Self::TargetRemote(message) => write!(f, "目标平台接口请求失败：{message}"),
            Self::TargetWriteUncertain(message) => {
                write!(f, "目标平台写入结果不确定：{message}")
            }
            Self::TargetNotOwned(message) => write!(f, "目标歌单归属校验失败：{message}"),
            Self::TargetReadOnly(message) => write!(f, "目标歌单不可写：{message}"),
            Self::TargetMappingRequired(message) => write!(f, "需要人工确认跨平台映射：{message}"),
            Self::Timeout => write!(f, "网易云接口请求超时，请稍后重试"),
            Self::Remote(message) => write!(f, "网易云接口请求失败：{message}"),
            Self::Io(error) => write!(f, "本机文件操作失败：{error}"),
            Self::Json(error) => write!(f, "本机 JSON 处理失败：{error}"),
            Self::Netease(error) => write!(f, "网易云请求失败：{error}"),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
impl From<NeteaseError> for AppError {
    fn from(value: NeteaseError) -> Self {
        match &value {
            NeteaseError::Http(error) if error.is_timeout() => Self::Timeout,
            _ => Self::Netease(value),
        }
    }
}
