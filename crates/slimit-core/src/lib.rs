//! SlimIt 核心：扫描与真实占用计算。

pub mod scan;
pub mod types;

pub use scan::scan;
pub use types::{DirStat, FileEntry, ScanError, ScanResult};
