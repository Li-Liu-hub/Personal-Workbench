use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeskError {
    #[error("本地数据库操作失败: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("加密操作失败")]
    Crypto,
    #[error("数据格式无效: {0}")]
    Data(#[from] serde_json::Error),
    #[error("导出文件处理失败: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("此文件格式暂不支持")]
    UnsupportedFormat,
    #[error("文件内容已在其他位置发生变化，未覆盖原文件")]
    ConcurrentModification,
    #[error("已有同名项目，请更换名称")]
    AlreadyExists,
    #[error("路径不可用: {0}")]
    InvalidPath(String),
}

pub type Result<T> = std::result::Result<T, DeskError>;
