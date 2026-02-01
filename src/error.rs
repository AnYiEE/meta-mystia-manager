use crate::metrics::report_event;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("未在游戏根目录下运行")]
    GameNotFound,

    #[error("游戏正在运行，请关闭游戏后重试")]
    GameRunning,

    #[error("进程列表错误：{0}")]
    ProcessListError(String),

    #[error("权限不足：{0}")]
    PermissionDenied(String),

    #[error("文件被占用：{0}")]
    FileInUse(String),

    #[error("网络错误：{0}")]
    NetworkError(String),

    #[error("被限流：{0}")]
    RateLimited(String),

    #[error("下载失败：{0}")]
    DownloadFailed(String),

    #[error("解压失败：{0}")]
    ExtractFailed(String),

    #[error("版本信息无效或解析失败")]
    InvalidVersionInfo,

    #[error("IO 错误：{0}")]
    Io(#[source] std::io::Error),

    #[error("对话框错误：{0}")]
    Dialog(String),

    #[error("其他错误：{0}")]
    Other(String),

    #[error("用户取消了操作")]
    UserCancelled,
}

impl From<anyhow::Error> for ManagerError {
    fn from(err: anyhow::Error) -> Self {
        let s = err.to_string();
        report_event("Error.From.Anyhow", Some(&s));
        ManagerError::Other(s)
    }
}

impl From<dialoguer::Error> for ManagerError {
    fn from(err: dialoguer::Error) -> Self {
        let s = err.to_string();
        report_event("Error.From.Dialog", Some(&s));
        ManagerError::Dialog(s)
    }
}

impl From<std::io::Error> for ManagerError {
    fn from(err: std::io::Error) -> Self {
        let s = err.to_string();
        report_event("Error.From.Io", Some(&s));
        ManagerError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, ManagerError>;
