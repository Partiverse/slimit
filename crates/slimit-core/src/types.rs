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

/// UI 桥接用的扫描聚合（前端不接收百万级 `files` 明细）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub root: PathBuf,
    pub file_count: u64,
    pub actual: u64,
    pub apparent: u64,
    /// 按 actual 降序的前 N 个目录（含根自身）。
    pub top_dirs: Vec<DirStat>,
    /// 按 actual 降序的前 N 个大文件（手动清理大安装包 .dmg/.zip/.pkg 场景，
    /// 目录聚合看不到单文件）。
    #[serde(default)]
    pub top_files: Vec<FileStat>,
}

/// 单文件聚合条目（大文件列表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
    pub path: PathBuf,
    pub apparent: u64,
    pub actual: u64,
}

/// 扫描进度事件。总量未知——这是进度而非百分比；`current_dir` 是
/// 最近被遍历的目录，UI 可据此展示"正在扫哪里"。
#[derive(Debug, Clone)]
pub struct ScanProgress {
    /// 已发现文件/符号链接条目累计数。
    pub files_done: u64,
    /// 最近处理的目录路径。
    pub current_dir: PathBuf,
}

impl ScanResult {
    /// 从完整扫描结果生成前端聚合视图；`top` 截断 top_dirs/top_files。
    pub fn summarize(&self, top: usize) -> ScanSummary {
        let mut dirs = self.dirs.clone();
        dirs.sort_unstable_by(|a, b| b.actual.cmp(&a.actual).then(a.path.cmp(&b.path)));
        let (actual, apparent) = self
            .dirs
            .iter()
            .find(|d| d.path == self.root)
            .map(|d| (d.actual, d.apparent))
            .unwrap_or((0, 0));
        // 大文件：非 shared 硬链接按 actual 降序取前 N（排除 0 字节与 symlink 级碎项）。
        let mut files: Vec<FileStat> = self
            .files
            .iter()
            .filter(|f| !f.shared && f.actual > 0)
            .map(|f| FileStat {
                path: f.path.clone(),
                apparent: f.apparent,
                actual: f.actual,
            })
            .collect();
        files.sort_unstable_by(|a, b| b.actual.cmp(&a.actual).then(a.path.cmp(&b.path)));
        files.truncate(top);
        ScanSummary {
            root: self.root.clone(),
            file_count: self.file_count,
            actual,
            apparent,
            top_dirs: dirs.into_iter().take(top).collect(),
            top_files: files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(path: &str, actual: u64) -> DirStat {
        DirStat {
            path: PathBuf::from(path),
            apparent: actual,
            actual,
            file_count: 1,
        }
    }

    #[test]
    fn summarize_orders_and_takes_top() {
        let res = ScanResult {
            root: PathBuf::from("/r"),
            file_count: 3,
            files: vec![],
            dirs: vec![
                dir("/r", 100),
                dir("/r/b", 60),
                dir("/r/a", 40),
                dir("/r/a/x", 30),
            ],
        };
        let s = res.summarize(2);
        assert_eq!(s.actual, 100);
        assert_eq!(s.top_dirs.len(), 2);
        assert_eq!(s.top_dirs[0].path, PathBuf::from("/r"));
        assert_eq!(s.top_dirs[1].path, PathBuf::from("/r/b"));
    }

    #[test]
    fn summarize_lists_top_files_deduped_and_sorted() {
        // 大文件列表（手动清理 .dmg 场景）：硬链接共享文件只计一次，
        // 0 字节文件剔除，按 actual 降序。
        let f = |p: &str, actual: u64, shared: bool| FileEntry {
            path: PathBuf::from(p),
            dev: 1,
            ino: 1,
            apparent: actual,
            actual,
            shared,
        };
        let res = ScanResult {
            root: PathBuf::from("/r"),
            file_count: 4,
            files: vec![
                f("/r/setup.dmg", 5000, false),
                f("/r/hardlink", 9000, true), // shared：与别处同 inode，剔除
                f("/r/tiny", 0, false),       // 0 字节剔除
                f("/r/movie.mp4", 9000, false),
            ],
            dirs: vec![dir("/r", 14000)],
        };
        let s = res.summarize(10);
        let paths: Vec<_> = s.top_files.iter().map(|x| x.path.clone()).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("/r/movie.mp4"), PathBuf::from("/r/setup.dmg")]
        );
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("root does not exist: {0}")]
    RootMissing(PathBuf),
    #[error("walk error: {0}")]
    Walk(String),
    #[error("diskutil error: {0}")]
    DiskUtil(String),
    #[error("plist parse error: {0}")]
    Plist(String),
}
