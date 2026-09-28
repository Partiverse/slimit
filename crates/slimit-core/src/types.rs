use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 单个文件/符号链接的元数据（目录只作为聚合节点存在）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub dev: u64,
    pub ino: u64,
    /// 表观大小（st_size）。
    pub apparent: u64,
    /// 真实占用（st_blocks * 512）。
    pub actual: u64,
    /// 该 inode 已在其他路径出现过（硬链接共享，聚合时不重复计数）。
    pub shared: bool,
}

/// 目录聚合统计。`actual`/`apparent` 均已做硬链接 inode 去重。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirStat {
    pub path: PathBuf,
    pub apparent: u64,
    pub actual: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub root: PathBuf,
    pub file_count: u64,
    /// 全体文件（含 shared 标记），按路径字典序。
    pub files: Vec<FileEntry>,
    /// 目录聚合（含根自身），按路径字典序。
    pub dirs: Vec<DirStat>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("root does not exist: {0}")]
    RootMissing(PathBuf),
    #[error("walk error: {0}")]
    Walk(String),
}
