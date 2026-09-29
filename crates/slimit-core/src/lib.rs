//! SlimIt 核心：扫描与真实占用计算。

pub mod apfs;
pub mod scan;
pub mod types;

/// macOS 快路径（getattrlistbulk）；其余平台走 `scan::walk_ignore`。
#[cfg(target_os = "macos")]
pub(crate) mod bulk;

pub use apfs::{
    list_snapshots, parse_snapshots_plist, parse_volume_info_plist, volume_summary, SnapshotInfo,
    VolumeSummary,
};
pub use scan::scan;
pub use types::{DirStat, FileEntry, ScanError, ScanResult, ScanSummary};
