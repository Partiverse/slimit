//! Tauri 2 GUI 壳：core 扫描 → 规则计划 → 执行（隔离/恢复）→ AI 解释。
//!
//! 桥接协议（命令名与负载即契约，前端 `ui/src/bridge.ts` 对应）：
//! - `scan_and_plan(root, top)` → `{ summary, plan }`（同步 scan + 规则匹配，
//!   进度经 `scan-progress` 事件流推送）
//! - `explain(req)` → `Explanation`（AI 提示层，永不影响执行授权）
//! - `apply_plan(items)` → `Vec<ApplyReport>`（仅 executable 项进隔离区）
//! - `restore_item(id)` → 恢复路径
//! - `list_quarantine()` → `Vec<Manifest>`
//! - `list_snapshots(volume)` / `volume_summary(mount)` 同 W5
//!
//! 红线（SPEC §5）：AI 输出只进 UI 提示层；执行授权只来自规则库；
//! 清理动作默认"迁入隔离区"语义（可完整恢复），AI 永无删除权。

use slimit_ai::{Explainer, Explanation, ExplanationRequest, HeuristicExplainer};
use slimit_core::ScanSummary;
use slimit_exec::{ApplyReport, Manifest, PlanItem};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// 事件序号：区分先后两次扫描，前端丢弃过期序号的进度事件。
static SCAN_SEQ: AtomicU64 = AtomicU64::new(0);

/// 扫描 + 计划的合并响应：一次遍历产出聚合视图与规则命中计划。
#[derive(Debug, serde::Serialize)]
pub struct ScanPlanResponse {
    pub summary: ScanSummary,
    /// 按 estimated_bytes 降序。仅 executable 项可被 apply_plan 执行。
    pub plan: Vec<PlanItem>,
}

#[tauri::command]
fn scan_and_plan(
    app: AppHandle,
    root: String,
    top: Option<usize>,
) -> Result<ScanPlanResponse, String> {
    let seq = SCAN_SEQ.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(&root);
    let result = {
        let app = app.clone();
        slimit_core::scan_with_progress(&root, &move |files_done| {
            // 进度事件：只带 files_done，总量未知（这正是进度而非百分比）。
            let _ = app.emit(
                "scan-progress",
                serde_json::json!({ "seq": seq, "files_done": files_done }),
            );
        })
    }
    .map_err(|e| e.to_string())?;

    let summary = result.summarize(top.unwrap_or(20));
    let rules = slimit_rules::embedded_rules().map_err(|e| e.to_string())?;
    let dirs: Vec<slimit_rules::DirSnapshot> = result
        .dirs
        .iter()
        .map(|d| slimit_rules::DirSnapshot::new(&d.path, d.apparent, d.actual))
        .collect();
    let plan = slimit_exec::plan_from_snapshots(&rules, &dirs);
    Ok(ScanPlanResponse { summary, plan })
}

#[tauri::command]
fn explain(req: ExplanationRequest) -> Result<Explanation, String> {
    HeuristicExplainer.explain(&req)
}

/// 隔离区根：`<app_data_dir>/quarantine/`；审计日志 `<app_data_dir>/audit/`。
fn quarantine(app: &AppHandle) -> Result<slimit_exec::Quarantine, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(slimit_exec::Quarantine::new(&data))
}

#[tauri::command]
fn apply_plan(app: AppHandle, items: Vec<PlanItem>) -> Result<Vec<ApplyReport>, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let q = quarantine(&app)?;
    let mut audit = slimit_exec::AuditLog::new(&data).map_err(|e| e.to_string())?;
    Ok(slimit_exec::apply(&items, &q, &mut audit))
}

#[tauri::command]
fn restore_item(app: AppHandle, id: String) -> Result<PathBuf, String> {
    let q = quarantine(&app)?;
    slimit_exec::restore(&q, &id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_quarantine(app: AppHandle) -> Result<Vec<Manifest>, String> {
    quarantine(&app)?.list().map_err(|e| e.to_string())
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
            scan_and_plan,
            explain,
            apply_plan,
            restore_item,
            list_quarantine,
            list_snapshots,
            volume_summary_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
