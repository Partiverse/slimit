//! Tauri 2 GUI 壳：core 扫描 → 规则计划 → 执行（隔离/恢复）→ AI 解释。
//!
//! 桥接协议（命令名与负载即契约，前端 `ui/src/bridge.ts` 对应）：
//! - `scan_and_plan(root, top)` → `{ summary, plan }`（同步 scan + 规则匹配，
//!   进度经 `scan-progress` 事件流推送）
//! - `explain(req)` → `Explanation`（AI 提示层，永不影响执行授权）
//! - `apply_plan(items)` → `Vec<ApplyReport>`（仅 executable 项进隔离区）
//! - `restore_item(id)` → 恢复路径
//! - `list_quarantine()` → `Vec<Manifest>`
//! - `purge_expired_quarantine()` → `Vec<Manifest>`（清理迁入超 14 天条目，
//!   不可逆——UI 二次确认后调用；每项追加 `purge-expired` 审计事件）
//! - `list_snapshots(volume)` / `volume_summary(mount)` 同 W5
//!
//! 红线（SPEC §5）：AI 输出只进 UI 提示层；执行授权只来自规则库；
//! 清理动作默认"迁入隔离区"语义（可完整恢复），AI 永无删除权。

use slimit_ai::{Explainer, Explanation, ExplanationRequest, HeuristicExplainer};
use slimit_core::ScanSummary;
use slimit_exec::{ApplyReport, Manifest, PlanItem};
use slimit_rules::Rule;
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
        slimit_core::scan_with_progress(&root, &move |p| {
            // 进度事件：累计条目数 + 当前目录（总量未知，是进度而非百分比）。
            let _ = app.emit(
                "scan-progress",
                serde_json::json!({
                    "seq": seq,
                    "files_done": p.files_done,
                    "current_dir": p.current_dir.to_string_lossy(),
                }),
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

/// 应用设置：目前仅云端 AI。持久化在 `<app_data_dir>/config.json`。
fn load_settings(app: &AppHandle) -> Result<slimit_ai::AiSettings, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let path = data.join("config.json");
    if !path.exists() {
        return Ok(slimit_ai::AiSettings::default());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("config.json 解析失败: {e}"))
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<slimit_ai::AiSettings, String> {
    load_settings(&app)
}

#[tauri::command]
fn set_settings(app: AppHandle, settings: slimit_ai::AiSettings) -> Result<(), String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(data.join("config.json"), text).map_err(|e| e.to_string())
}

/// 规则库语义兜底解释（离线，可信来源）。
fn rules_explanation(req: &ExplanationRequest) -> Option<Explanation> {
    let rule_id = req.nearest_rule_hits.first()?;
    let rule = slimit_rules::embedded_rules()
        .ok()?
        .into_iter()
        .find(|r| &r.id == rule_id)?;
    let s = rule.semantics.as_ref()?;
    Some(Explanation {
        what: format!(
            "{}（规则库 {}）",
            s.what.clone().unwrap_or_default(),
            rule.id
        ),
        producer: s.producer.clone().unwrap_or_else(|| "未知".to_string()),
        consequence: s
            .consequence
            .clone()
            .or_else(|| s.safe_to_delete_because.clone())
            .unwrap_or_else(|| "规则库未描述删除后果".to_string()),
        suggested_risk: match rule.risk {
            slimit_rules::Risk::Green => slimit_ai::RiskHint::Green,
            slimit_rules::Risk::Yellow => slimit_ai::RiskHint::Yellow,
            slimit_rules::Risk::Red => slimit_ai::RiskHint::Red,
        },
        confidence: 0.95,
        source: "rules".into(),
    })
}

#[tauri::command]
fn explain(app: AppHandle, req: ExplanationRequest) -> Result<Explanation, String> {
    // 优先级：云端 AI（显式启用时，把规则库语义作为上下文）→ 规则库语义
    // → 启发式降级。任何 AI 失败都静默回落，永不阻塞、永不影响执行授权。
    let base = rules_explanation(&req).unwrap_or_else(|| {
        HeuristicExplainer
            .explain(&req)
            .expect("heuristic explainer is infallible")
    });
    let settings = load_settings(&app).unwrap_or_default();
    if settings.enabled {
        if let Ok(cloud) = slimit_ai::explain_cloud(&req, &settings) {
            return Ok(cloud);
        }
    }
    Ok(base)
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

/// 清理隔离区中迁入超过 14 天（`DEFAULT_RETENTION_DAYS`）的条目。
/// 这是不可逆删除——调用方（UI）负责二次确认；本命令负责审计：
/// 每个被清理条目追加一行 `purge-expired` 事件（红线④：删除必须可追溯）。
#[tauri::command]
fn purge_expired_quarantine(app: AppHandle) -> Result<Vec<Manifest>, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let q = quarantine(&app)?;
    let purged = q
        .purge_expired(slimit_exec::DEFAULT_RETENTION_DAYS)
        .map_err(|e| e.to_string())?;
    let mut audit = slimit_exec::AuditLog::new(&data).map_err(|e| e.to_string())?;
    for m in &purged {
        audit.record(&serde_json::json!({
            "event": "purge-expired",
            "id": m.id,
            "path": m.original_path,
            "rule": m.rule_id,
        }));
    }
    Ok(purged)
}

#[tauri::command]
fn list_snapshots(volume: String) -> Result<Vec<slimit_core::SnapshotInfo>, String> {
    slimit_core::list_snapshots(&volume).map_err(|e| e.to_string())
}

#[tauri::command]
fn volume_summary_cmd(mount: String) -> Result<slimit_core::VolumeSummary, String> {
    slimit_core::volume_summary(&mount).map_err(|e| e.to_string())
}

/// 列出全部嵌入规则（按 os 分组，按 risk 排序）。
/// 用于 UI 规则面板：用户可浏览规则库内容，不触发任何执行。
#[tauri::command]
fn list_rules() -> Result<Vec<Rule>, String> {
    slimit_rules::embedded_rules().map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_and_plan,
            explain,
            get_settings,
            set_settings,
            apply_plan,
            restore_item,
            list_quarantine,
            purge_expired_quarantine,
            list_snapshots,
            volume_summary_cmd,
            list_rules
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
