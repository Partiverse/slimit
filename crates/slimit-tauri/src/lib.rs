//! Tauri 2 GUI 壳：tauri → core 调用链顶层（exec/ai 后续周接入）。
//!
//! 桥接协议（命令名与负载即契约，前端 `ui/src/bridge.ts` 对应）：
//! - `scan_dir(root, top)` → `ScanSummary`（聚合视图，不传百万级明细）
//! - `list_snapshots(volume)` → `Vec<SnapshotInfo>`
//! - `volume_summary(mount)` → `VolumeSummary`
//!
//! 扫描可能长耗时（~/Library 量级约 1 分钟），W5 骨架用同步阻塞命令 +
//! 前端禁用按钮；进度事件流留待 W6 一并接入。

use slimit_core::ScanSummary;
use std::path::PathBuf;

#[tauri::command]
fn scan_dir(root: String, top: Option<usize>) -> Result<ScanSummary, String> {
    slimit_core::scan(&PathBuf::from(&root))
        .map(|res| res.summarize(top.unwrap_or(20)))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_snapshots(volume: String) -> Result<Vec<slimit_core::SnapshotInfo>, String> {
    slimit_core::list_snapshots(&volume).map_err(|e| e.to_string())
}

#[tauri::command]
fn volume_summary_cmd(mount: String) -> Result<slimit_core::VolumeSummary, String> {
    slimit_core::volume_summary(&mount).map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_dir,
            list_snapshots,
            volume_summary_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
