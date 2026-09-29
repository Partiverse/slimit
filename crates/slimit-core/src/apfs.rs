//! APFS 快照与卷容量探测（数据层，为 System Data 解剖视图供数）。
//!
//! 数据来源全部是官方 `diskutil -plist` 输出，无私有 API：
//! - `diskutil apfs listSnapshots <volume> -plist`
//! - `diskutil info -plist <mount>`
//!
//! Finder 显示的 purgeable 数值来自私有 API 启发式，无公开来源，这里不估算；
//! 但快照的 `Purgeable` / `LimitingContainerShrink` 标志是 APFS 直接给出的。

use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::ScanError;

/// 一条 APFS 快照记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    #[serde(rename = "SnapshotName")]
    pub name: String,
    #[serde(rename = "SnapshotUUID")]
    pub uuid: String,
    #[serde(rename = "SnapshotXID", default)]
    pub xid: u64,
    #[serde(rename = "Purgeable", default)]
    pub purgeable: bool,
    /// true＝该快照阻止 APFS 容器收缩（删掉才能分区缩小）。
    #[serde(rename = "LimitingContainerShrink", default)]
    pub limiting_container_shrink: bool,
}

/// 卷容量摘要（来自 `diskutil info -plist`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeSummary {
    pub volume_name: Option<String>,
    pub device_identifier: Option<String>,
    /// 本卷文件系统口径的剩余空间。
    pub free_space: Option<u64>,
    pub capacity_in_use: Option<u64>,
    /// APFS 容器总量 / 剩余（容器内所有卷共享）。
    pub apfs_container_size: Option<u64>,
    pub apfs_container_free: Option<u64>,
    /// 系统卷密封快照名（仅当该挂载点本身就是快照时出现，如 `/`）。
    pub system_snapshot_name: Option<String>,
}

/// `diskutil apfs listSnapshots -plist` 的顶层结构。
#[derive(Deserialize)]
struct SnapshotsPlist {
    #[serde(rename = "Snapshots", default)]
    snapshots: Vec<SnapshotInfo>,
}

/// 解析 `diskutil apfs listSnapshots <volume> -plist` 输出。
pub fn parse_snapshots_plist(xml: &[u8]) -> Result<Vec<SnapshotInfo>, ScanError> {
    let parsed: SnapshotsPlist =
        plist::from_bytes(xml).map_err(|e| ScanError::Plist(format!("snapshots: {e}")))?;
    Ok(parsed.snapshots)
}

/// 解析 `diskutil info -plist <mount>` 输出为卷摘要。
pub fn parse_volume_info_plist(xml: &[u8]) -> Result<VolumeSummary, ScanError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Raw {
        volume_name: Option<String>,
        device_identifier: Option<String>,
        free_space: Option<u64>,
        capacity_in_use: Option<u64>,
        #[serde(rename = "APFSContainerSize")]
        apfs_container_size: Option<u64>,
        #[serde(rename = "APFSContainerFree")]
        apfs_container_free: Option<u64>,
        #[serde(rename = "APFSSnapshotName")]
        apfs_snapshot_name: Option<String>,
    }
    let raw: Raw =
        plist::from_bytes(xml).map_err(|e| ScanError::Plist(format!("volume info: {e}")))?;
    Ok(VolumeSummary {
        volume_name: raw.volume_name,
        device_identifier: raw.device_identifier,
        free_space: raw.free_space,
        capacity_in_use: raw.capacity_in_use,
        apfs_container_size: raw.apfs_container_size,
        apfs_container_free: raw.apfs_container_free,
        system_snapshot_name: raw.apfs_snapshot_name,
    })
}

fn run_diskutil(args: &[&str]) -> Result<Vec<u8>, ScanError> {
    let out = Command::new("diskutil")
        .args(args)
        .output()
        .map_err(|e| ScanError::DiskUtil(format!("spawn diskutil: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(ScanError::DiskUtil(format!(
            "diskutil {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(out.stdout)
}

/// 列出某卷的全部 APFS 快照（`volume` 可以是挂载点或 diskXsY）。
pub fn list_snapshots(volume: &str) -> Result<Vec<SnapshotInfo>, ScanError> {
    let xml = run_diskutil(&["apfs", "listSnapshots", volume, "-plist"])?;
    parse_snapshots_plist(&xml)
}

/// 摘要某挂载点的容量信息。
pub fn volume_summary(mount: &str) -> Result<VolumeSummary, ScanError> {
    let xml = run_diskutil(&["info", "-plist", mount])?;
    parse_volume_info_plist(&xml)
}
