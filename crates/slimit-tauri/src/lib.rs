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
/// 与前端任务 id 对齐（前端 `++seqRef` 首个任务 id=1）：配合
/// `fetch_add(1) + 1`（fetch_add 返回旧值）⇒ 首个扫描 seq=1、第二个 seq=2，
/// 与前端逐一对应。**必须成对修改**：起点 0 + 取加后值；起点 1 + 取旧值
/// 是同一结果，任一处单独改动都会导致错位 1（进度数字恒 0）。
static SCAN_SEQ: AtomicU64 = AtomicU64::new(0);

/// 进度事件最小发射间隔（毫秒）：防事件风暴冻屏（见 scan_and_plan 注释）。
const PROGRESS_EMIT_INTERVAL_MS: u64 = 100;

/// 扫描根解析：先做 `~` 展开（与规则路径同一套展开语义），再校验存在。
///
/// 手测发现（2026-10-01）：直接输入 `~/Library/Caches` 报 "root does not exist"
/// ——此前只有规则路径走 tilde 展开，扫描根要求绝对路径，而 `~` 是小白最顺手的
/// 写法。展开失败或路径不存在时给中文可操作提示，而不是英文 Err 原文。
fn resolve_scan_root(input: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("请输入要扫描的路径".into());
    }
    let expanded = slimit_rules::matcher::expand_tilde(trimmed)
        .ok_or_else(|| "无法展开 ~：系统未设置主目录环境变量（HOME / USERPROFILE）".to_string())?;
    if !expanded.exists() {
        return Err(format!("路径不存在：{}", expanded.display()));
    }
    if !expanded.is_dir() {
        return Err(format!(
            "这是一个文件不是文件夹，请指定文件夹：{}",
            expanded.display()
        ));
    }
    Ok(expanded)
}

/// 扫描 + 计划的合并响应：一次遍历产出聚合视图与规则命中计划。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ScanPlanResponse {
    pub summary: ScanSummary,
    /// 按 estimated_bytes 降序。仅 executable 项可被 apply_plan 执行。
    pub plan: Vec<PlanItem>,
}

#[tauri::command]
async fn scan_and_plan(
    app: AppHandle,
    root: String,
    top: Option<usize>,
    task_id: Option<u64>,
) -> Result<ScanPlanResponse, String> {
    // 必须异步 + spawn_blocking（2026-10-02 终极根因）：Tauri 同步命令在
    // **主线程**执行——扫描百万文件期间整个应用事件循环停摆：彩球、窗口
    // 无响应、进度事件无法送达 WebView（用户所见全部「无进度」症状的
    // 总根源）。async 命令 + spawn_blocking 把扫描挪到工作线程。
    tauri::async_runtime::spawn_blocking(move || scan_and_plan_blocking(app, root, top, task_id))
        .await
        .map_err(|e| format!("scan task join failed: {e}"))?
}

fn scan_and_plan_blocking(
    app: AppHandle,
    root: String,
    top: Option<usize>,
    task_id: Option<u64>,
) -> Result<ScanPlanResponse, String> {
    // 进度事件序号必须与前端任务 id 一致（2026-10-02 修复「进度条不走」：
    // 此前后端自增序号从 0 起、前端任务 id 从 1 起，`t.id === p.seq` 永不
    // 匹配，进度数字永远停在 0）。由前端显式传 task_id，后端原样回传。
    // fetch_add 返回旧值：起点 0、首次调用返回 0，+1 ⇒ 首个 seq=1，与前端
    // 首个任务 id 对齐（见 SCAN_SEQ 注释：起点与「旧值/加后值」必须成对）。
    let seq = task_id.unwrap_or_else(|| SCAN_SEQ.fetch_add(1, Ordering::Relaxed) + 1);
    let root = resolve_scan_root(&root)?;
    // 测试钩子：`--scan-delay-ms N` 启动参数让每次进度回调停顿 N 毫秒，
    // 模拟慢盘（真机验证进度条中间态用；`open --args` 可传入 GUI 进程，
    // 环境变量对 launchd 启动的 app 不可靠）。生产不带参数即零开销。
    let delay_ms: u64 = std::env::args()
        .position(|a| a == "--scan-delay-ms")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    // 进度事件节流（2026-10-02 根因修复）：大目录扫描每目录批次 emit 一次，
    // 百万级文件 = 每秒数百事件，前端每事件 setState 全面板重渲染 → WKWebView
    // 主线程打满、UI 冻结（用户所见「光标一直转、不知道在不在扫」）。改为
    // 每 100ms 最多发一次 + 结束必发一次。
    let result = {
        let app = app.clone();
        let last_emit = std::sync::Mutex::new(
            std::time::Instant::now() - std::time::Duration::from_millis(PROGRESS_EMIT_INTERVAL_MS),
        );
        slimit_core::scan_with_progress(&root, &move |p| {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
            let mut last = last_emit.lock().unwrap();
            if last.elapsed().as_millis() < PROGRESS_EMIT_INTERVAL_MS as u128 {
                return;
            }
            *last = std::time::Instant::now();
            drop(last);
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
    // project 规则：把年龄证据与重建成本写进「删除后果」——这是小白
    // 敢不敢点执行的关键信息（RECLAIM-STRATEGY §4「把担保做进语义」）。
    let age_note = match (rule.project.as_ref(), req.age_days) {
        (Some(proj), Some(age)) => {
            let base = s
                .consequence
                .clone()
                .or_else(|| s.safe_to_delete_because.clone())
                .unwrap_or_else(|| "删除后果见规则库描述".to_string());
            if req.below_min_age {
                let need = proj
                    .max_age_days
                    .map(|m| format!("（规则要求闲置满 {m} 天）"))
                    .unwrap_or_default();
                format!("{base}。⚠️ 距上次构建/使用已 {age} 天，仍在活跃使用中，建议暂不清理{need}")
            } else {
                format!("{base}。已 {age} 天未动，回收收益是一次性的（不会像缓存那样反复长回来）")
            }
        }
        _ => s
            .consequence
            .clone()
            .or_else(|| s.safe_to_delete_because.clone())
            .unwrap_or_else(|| "规则库未描述删除后果".to_string()),
    };
    Some(Explanation {
        what: format!(
            "{}（规则库 {}）",
            s.what.clone().unwrap_or_default(),
            rule.id
        ),
        producer: s.producer.clone().unwrap_or_else(|| "未知".to_string()),
        consequence: age_note,
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
    // 红线（SPEC §5）：执行授权只来自规则库。前端传来的 PlanItem 来自 IPC——
    // `executable` 是客户端自证。按规则库重新推导可执行性（复用 canonical
    // `match_rules`+`plan` 逻辑），伪造、未知 rule_id 或 red 规则命中的项
    // 一律降级为不可执行。
    let rules = slimit_rules::embedded_rules().map_err(|e| e.to_string())?;
    let authorized = slimit_exec::authorize_items(items, &rules);
    Ok(slimit_exec::apply(&authorized, &q, &mut audit))
}

#[tauri::command]
fn restore_item(app: AppHandle, id: String) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let q = quarantine(&app)?;
    let mut audit = slimit_exec::AuditLog::new(&data).map_err(|e| e.to_string())?;
    slimit_exec::restore(&q, &id, &mut audit).map_err(|e| e.to_string())
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

/// AI 设置「测试连接」：轻量探测 /models 端点（不发对话、不耗 token）。
#[tauri::command]
fn test_ai(app: AppHandle) -> Result<String, String> {
    let s = load_settings(&app)?;
    slimit_ai::test_connection(&s)
}

// ---------- 定时扫描（无感化 L1，设计见 HANDOFF） ----------

/// 定时扫描配置（持久化 `<app_data>/schedule.json`）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanSchedule {
    pub enabled: bool,
    pub root: String,
    /// L3 预授权（docs/L3-AUTOCLEAN-DESIGN.md，默认关闭）：定时扫描时自动
    /// 隔离迁移 green+purge-dir 命中项。授权主体是用户 opt-in，AI 不参与。
    #[serde(default)]
    pub auto_clean: bool,
}

const LAUNCH_AGENT_LABEL: &str = "dev.partiverse.slimit.scan";
const LAUNCH_AGENT_WEEKDAY: i32 = 6; // 周六 10:00
const LAUNCH_AGENT_HOUR: i32 = 10;

fn schedule_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("schedule.json"))
}

#[tauri::command]
fn get_scan_schedule(app: AppHandle) -> Result<ScanSchedule, String> {
    let p = schedule_path(&app)?;
    match std::fs::read_to_string(p) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("schedule.json 损坏: {e}")),
        Err(_) => Ok(ScanSchedule::default()),
    }
}

/// 保存定时扫描配置并安装/卸载 LaunchAgent。返回给人话结果。
#[tauri::command]
async fn set_scan_schedule(app: AppHandle, schedule: ScanSchedule) -> Result<String, String> {
    let p = schedule_path(&app)?;
    std::fs::write(&p, serde_json::to_string_pretty(&schedule).unwrap())
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_display = exe.to_string_lossy().to_string();
    let plist = dirs_home().join("Library/LaunchAgents/dev.partiverse.slimit.scan.plist");
    let uid_output = std::process::Command::new("id")
        .args(["-u"])
        .output()
        .map_err(|e| e.to_string())?;
    let uid = String::from_utf8_lossy(&uid_output.stdout)
        .trim()
        .to_string();
    // 先卸载旧任务（不存在时报错忽略）。
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{LAUNCH_AGENT_LABEL}")])
        .output();
    if !schedule.enabled {
        let _ = std::fs::remove_file(&plist);
        return Ok("已关闭定时扫描".into());
    }
    let root = resolve_scan_root(&schedule.root)?;
    let root_display = root.to_string_lossy().to_string();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCH_AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe_display}</string>
    <string>--scheduled-scan</string>
    <string>{root_display}</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Weekday</key><integer>{LAUNCH_AGENT_WEEKDAY}</integer>
    <key>Hour</key><integer>{LAUNCH_AGENT_HOUR}</integer>
    <key>Minute</key><integer>0</integer>
  </dict>
</dict>
</plist>
"#,
    );
    std::fs::create_dir_all(plist.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&plist, xml).map_err(|e| e.to_string())?;
    let out = std::process::Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        // 已加载等情况：幂等 kickstart 验证
        let _ = std::process::Command::new("launchctl")
            .args([
                "kickstart",
                "-k",
                &format!("gui/{uid}/{LAUNCH_AGENT_LABEL}"),
            ])
            .output();
    }
    Ok(format!(
        "已开启：每周六 10:00 自动扫描 {}（仅通知，不自动清理）",
        root.display()
    ))
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// LaunchAgent 触发的无头扫描：扫 + 规则计划 + 系统通知摘要后退出。
fn scheduled_scan_headless(root_str: &str) {
    let root = match resolve_scan_root(root_str) {
        Ok(r) => r,
        Err(e) => {
            notify("Slimit 定时扫描失败", &e);
            return;
        }
    };
    let result = match slimit_core::scan(&root) {
        Ok(r) => r,
        Err(e) => {
            notify("Slimit 定时扫描失败", &e.to_string());
            return;
        }
    };
    let rules = match slimit_rules::embedded_rules() {
        Ok(r) => r,
        Err(e) => {
            notify("Slimit 定时扫描失败", &e.to_string());
            return;
        }
    };
    let dirs: Vec<slimit_rules::DirSnapshot> = result
        .dirs
        .iter()
        .map(|d| slimit_rules::DirSnapshot::new(&d.path, d.apparent, d.actual))
        .collect();
    let items = slimit_exec::plan_from_snapshots(&rules, &dirs);
    let (count, bytes) = items.iter().fold((0u64, 0u64), |(n, b), i| {
        (
            n + i.executable as u64,
            b + if i.executable { i.estimated_bytes } else { 0 },
        )
    });

    // L3 预授权自动清理（docs/L3-AUTOCLEAN-DESIGN.md）：仅在用户 opt-in
    // （schedule.auto_clean）时执行，范围封闭于「green + purge-dir 的可执行
    // 项」——yellow/red/project/user-manual 永不自动执行。仍走隔离区+审计。
    let schedule: ScanSchedule = std::fs::read_to_string(
        dirs_home().join("Library/Application Support/dev.partiverse.slimit/schedule.json"),
    )
    .ok()
    .and_then(|t| serde_json::from_str(&t).ok())
    .unwrap_or_default();
    let mut confirm_count = 0u64;
    let mut confirm_bytes = 0u64;
    if schedule.enabled && schedule.auto_clean {
        let app_data = dirs_home().join("Library/Application Support/dev.partiverse.slimit");
        let quarantine = slimit_exec::Quarantine::new(&app_data);
        let Ok(mut audit) = slimit_exec::AuditLog::new(&app_data) else {
            return;
        };
        // 按 path 去重：多条规则可命中同一目录（首条成功后其余必报
        // target missing，审计噪音）。保留净收益最大的一条。
        let mut seen = std::collections::HashSet::new();
        let green: Vec<slimit_exec::PlanItem> = items
            .iter()
            .filter(|i| i.executable && i.risk == slimit_rules::Risk::Green)
            .filter(|i| seen.insert(i.path.clone()))
            .cloned()
            .collect();
        for i in items.iter() {
            if i.executable && i.risk != slimit_rules::Risk::Green {
                confirm_count += 1;
                confirm_bytes += i.estimated_bytes;
            }
        }
        let authorized = slimit_exec::authorize_items(green.clone(), &rules);
        let reports = slimit_exec::apply(&authorized, &quarantine, &mut audit);
        for r in &reports {
            if r.error.is_some() {
                notify(
                    "Slimit 自动清理部分失败",
                    &r.error.clone().unwrap_or_default(),
                );
            }
        }
        let done = reports.iter().filter(|r| r.error.is_none()).count();
        if done > 0 {
            notify(
                "Slimit 已自动清理可再生缓存",
                &format!(
                    "已隔离 {} 项（{}），14 天内可在隔离区恢复。另有 {} 项（{}）需要您确认。",
                    done,
                    fmt_bytes(
                        reports
                            .iter()
                            .filter(|r| r.error.is_none())
                            .map(|r| r.item.estimated_bytes)
                            .sum()
                    ),
                    confirm_count,
                    fmt_bytes(confirm_bytes)
                ),
            );
        }
        return;
    }
    let summary = format!(
        "发现 {} 项可清理（{}）。打开 Slimit 查看详情。",
        count,
        fmt_bytes(bytes)
    );
    notify("Slimit 周度扫描完成", &summary);
    // L2（无感化第二档）：发现可清理项时自动拉起应用并加载结果——
    // 用户点开就是现成的清理计划，不用重新扫描。
    if count > 0 {
        let response = ScanPlanResponse {
            summary: result.summarize(20),
            plan: items,
        };
        {
            let pending = dirs_home()
                .join("Library/Application Support/dev.partiverse.slimit")
                .join("pending-scan-results.json");
            if std::fs::write(
                &pending,
                serde_json::to_string(&response).unwrap_or_default(),
            )
            .is_ok()
            {
                launch_gui();
            }
        }
    }
}

/// pending 定时扫描结果的落盘位置（GUI 启动时消费）。
pub fn pending_results_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // <…>/Slimit.app/Contents/MacOS/slimit-tauri → 向上找 .app，再取
    // 同级 app data 目录（~/Library/Application Support/dev.partiverse.slimit）。
    let mut anc = exe.parent();
    while let Some(dir) = anc {
        if dir.extension().map(|e| e == "app").unwrap_or(false) {
            let home = dirs_home();
            return Some(
                home.join("Library/Application Support/dev.partiverse.slimit")
                    .join("pending-scan-results.json"),
            );
        }
        anc = dir.parent();
    }
    None
}

fn launch_gui() {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return,
    };
    // 从无头二进制路径向上找 .app bundle，用 LaunchServices 拉起 GUI。
    let mut anc = exe.parent();
    while let Some(dir) = anc {
        if dir.extension().map(|e| e == "app").unwrap_or(false) {
            let _ = std::process::Command::new("open").arg(dir).spawn();
            return;
        }
        anc = dir.parent();
    }
}

/// GUI 启动时消费定时扫描结果：读取并删除 pending 文件。
#[tauri::command]
fn take_pending_results(app: AppHandle) -> Result<Option<ScanPlanResponse>, String> {
    let Some(path) = pending_results_path() else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let _ = std::fs::remove_file(&path);
            match serde_json::from_str::<ScanPlanResponse>(&text) {
                Ok(r) => {
                    // A3：通知改走用户通知框架（osascript 在 LaunchAgent 上下文
                    // 静默失败，插件走 UNUserNotificationCenter 身份正确）。
                    use tauri_plugin_notification::NotificationExt;
                    let count = r.plan.iter().filter(|p| p.executable).count();
                    app.notification()
                        .builder()
                        .title("Slimit 定时扫描完成")
                        .body(format!(
                            "发现 {} 项可清理（{}），已在清理页加载",
                            count,
                            fmt_bytes(r.summary.actual)
                        ))
                        .show()
                        .map_err(|e| e.to_string())?;
                    Ok(Some(r))
                }
                Err(_) => Ok(None),
            }
        }
        Err(_) => Ok(None),
    }
}

fn fmt_bytes(b: u64) -> String {
    let gib = b as f64 / 1073741824.0;
    if gib >= 1.0 {
        format!("{gib:.1} GiB")
    } else {
        format!("{:.0} MiB", b as f64 / 1048576.0)
    }
}

fn notify(title: &str, body: &str) {
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display notification \"{body}\" with title \"{title}\"",
            body = body.replace('"', "'"),
            title = title
        ))
        .output();
}

/// 手动清理探测（2026-10-02）：用户在 UI 里点「加入计划」前先探一次——
/// 返回存在性与真实占用（UI 展示用）；不存在返回错误。
#[derive(Debug, serde::Serialize)]
pub struct ManualProbe {
    pub path: PathBuf,
    pub is_dir: bool,
    /// 真实占用字节（目录 = 聚合全部内容）。
    pub actual_bytes: u64,
    pub apparent_bytes: u64,
}

#[tauri::command]
fn probe_manual(path: String) -> Result<ManualProbe, String> {
    let expanded = slimit_rules::matcher::expand_tilde(path.trim())
        .ok_or_else(|| "无法展开 ~：系统未设置主目录环境变量".to_string())?;
    if !expanded.exists() {
        return Err(format!("路径不存在：{}", expanded.display()));
    }
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(&expanded).map_err(|e| format!("读取元数据失败: {e}"))?;
    let is_dir = meta.is_dir();
    // 单文件直接取 metadata；目录做一次轻量聚合（复用扫描器，目录可能很大，
    // 但 probe 是用户显式动作，可接受；超大目录进度不推送——秒级内一般完成）。
    let (actual, apparent) = if is_dir {
        let r = slimit_core::scan(&expanded).map_err(|e| e.to_string())?;
        let root = r
            .dirs
            .iter()
            .find(|d| d.path == expanded)
            .map(|d| (d.actual, d.apparent))
            .unwrap_or((0, 0));
        root
    } else {
        (meta.blocks() * 512, meta.len())
    };
    Ok(ManualProbe {
        path: expanded,
        is_dir,
        actual_bytes: actual,
        apparent_bytes: apparent,
    })
}

/// 检测「完全磁盘访问」授权状态（种子反馈：一个文件夹一个弹窗点得累 +
/// 授权后软件状态不更新）。探测方式：尝试列读 FDA 保护目录 ~/Library/Safari
/// （未授权时 read_dir 返回权限错误；授权后可读）。两目录互为备份。
#[tauri::command]
fn check_fda() -> Result<bool, String> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Err("HOME 未设置".into());
    };
    for probe in [
        home.join("Library").join("Safari"),
        home.join("Library").join("Mail"),
    ] {
        if !probe.exists() {
            continue;
        }
        return Ok(std::fs::read_dir(&probe).is_ok());
    }
    // 两个探测目录都不存在（罕见）：按未授权处理并提示人工确认。
    Ok(false)
}

/// 立即彻底删除单个隔离条目（不可逆；UI 二次确认后调用，审计必记）。
#[tauri::command]
fn purge_quarantine_item(app: AppHandle, id: String) -> Result<Manifest, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let q = quarantine(&app)?;
    let m = q.purge_entry(&id).map_err(|e| e.to_string())?;
    let mut audit = slimit_exec::AuditLog::new(&data).map_err(|e| e.to_string())?;
    audit.record(&serde_json::json!({
        "event": "purge-entry",
        "id": m.id,
        "path": m.original_path,
        "rule": m.rule_id,
    }));
    Ok(m)
}

/// 打开「完全磁盘访问」系统设置面板（权限引导用；种子反馈：扫描时才
/// 发现缺权限，希望一开始就引导到位）。
#[tauri::command]
fn open_fda_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开系统设置: {e}"))
}

pub fn run() {
    // 无头模式：LaunchAgent 周度扫描（设置页开关安装的定时任务会带此参数
    // 调起本二进制）。扫完发系统通知后退出，不启动 GUI。
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--scheduled-scan") {
        let root = args.get(pos + 1).cloned().unwrap_or_default();
        scheduled_scan_headless(&root);
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
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
            list_rules,
            test_ai,
            open_fda_settings,
            probe_manual,
            check_fda,
            purge_quarantine_item,
            get_scan_schedule,
            set_scan_schedule,
            take_pending_results
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 种子反馈：点 Dock 图标偶发无法回到窗口（应用运行但窗口未聚焦）。
            // macOS 的 reopen 事件（点 Dock/再次启动）必须显式唤起主窗口。
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_req(age: Option<u64>, below: bool) -> ExplanationRequest {
        ExplanationRequest {
            path: "/Users/x/proj/target".into(),
            actual_bytes: 5 << 30,
            apparent_bytes: 5 << 30,
            owner_bundle: None,
            nearest_rule_hits: vec!["macos-project-cargo-target".into()],
            age_days: age,
            below_min_age: below,
        }
    }

    /// R4：年龄证据必须进入解释文案（GUI 手测时 UI 表格会把长文案截断，
    /// 断言只能落在后端输出上）。
    #[test]
    fn project_explanation_includes_age_evidence() {
        let e = rules_explanation(&project_req(Some(1200), false)).unwrap();
        assert!(
            e.consequence.contains("1200 天未动"),
            "got: {}",
            e.consequence
        );
        assert!(e.consequence.contains("一次性"), "got: {}", e.consequence);
        assert!(!e.consequence.contains("暂不清理"));
    }

    #[test]
    fn guarded_project_explanation_warns_and_cites_threshold() {
        let e = rules_explanation(&project_req(Some(3), true)).unwrap();
        assert!(e.consequence.contains("暂不清理"), "got: {}", e.consequence);
        assert!(
            e.consequence.contains("14 天"),
            "threshold must be cited: {}",
            e.consequence
        );
    }

    #[test]
    fn path_rule_explanation_has_no_age_note() {
        let mut req = project_req(None, false);
        req.nearest_rule_hits = vec!["macos-homebrew-cache".into()];
        let e = rules_explanation(&req).unwrap();
        assert!(!e.consequence.contains("天未动"), "got: {}", e.consequence);
    }

    // 扫描根 tilde 展开（手测发现 `~/Library/Caches` 报 root does not exist）。
    // HOME 是进程全局变量，动它的测试全部串行。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct HomeGuard(Option<std::ffi::OsString>);
    impl HomeGuard {
        fn set(v: Option<&str>) -> Self {
            let old = std::env::var_os("HOME");
            match v {
                Some(x) => std::env::set_var("HOME", x),
                None => std::env::remove_var("HOME"),
            }
            Self(old)
        }
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn scan_root_expands_tilde_and_validates() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join("code/proj")).unwrap();
        std::fs::create_dir_all(&home.join("code/proj/target")).unwrap();
        let _guard = HomeGuard::set(Some(home.to_str().unwrap()));

        // 绝对路径原样通过
        assert_eq!(
            resolve_scan_root(home.join("code/proj").to_str().unwrap()).unwrap(),
            home.join("code/proj")
        );
        // ~ 与 ~/ 展开
        assert_eq!(resolve_scan_root("~").unwrap(), home);
        assert_eq!(
            resolve_scan_root("~/code/proj").unwrap(),
            home.join("code/proj")
        );
        // 两侧空白容错
        assert_eq!(
            resolve_scan_root("  ~/code/proj  ").unwrap(),
            home.join("code/proj")
        );
        // 空输入 / 不存在 / 是文件不是目录 —— 都要中文可操作提示
        assert!(resolve_scan_root("   ").unwrap_err().contains("请输入"));
        assert!(resolve_scan_root("~/nope")
            .unwrap_err()
            .contains("路径不存在"));
        let f = home.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(resolve_scan_root("~/a.txt")
            .unwrap_err()
            .contains("不是文件夹"));
    }

    #[test]
    fn scan_root_reports_unset_home_instead_of_passing_through() {
        // HOME 与 USERPROFILE 都缺失时必须报错——绝不能把字面 `~/x` 当
        // 相对路径传进扫描（会扫到当前工作目录，静默给出错误结果）。
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = (HomeGuard::set(None), {
            let old = std::env::var_os("USERPROFILE");
            std::env::remove_var("USERPROFILE");
            move || {
                if let Some(v) = old {
                    std::env::set_var("USERPROFILE", v);
                }
            }
        });
        let err = resolve_scan_root("~/code").unwrap_err();
        assert!(err.contains("无法展开"), "got: {err}");
    }

    /// 进度事件 seq 必须与前端任务 id 逐一对齐（rc9/rc10 两次「数字恒 0」
    /// 的根因）。前端 `++seqRef` 首个任务 id=1；后端起点 0 + fetch_add(1)
    /// 取加后值 ⇒ 首个 seq=1。此测试锁死该不变量。
    #[test]
    fn progress_seq_starts_at_one_and_increments() {
        // 复刻 scan_and_plan 的取值逻辑：起点 0、fetch_add(1) 取加后值。
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let next = || SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        assert_eq!(
            next(),
            1,
            "first scan seq must be 1 (frontend task id starts at 1)"
        );
        assert_eq!(next(), 2);
        assert_eq!(next(), 3);
        assert_eq!(SEQ.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    /// 真实主目录下端到端展开 + 真实扫描。GUI 自动化被系统焦点限制挡住时，
    /// 这里用后端契约证明 `~/…` 真的能扫（修复前正是这条路径报
    /// root does not exist），而不是只在 mock HOME 下过单测。
    #[test]
    fn scan_root_works_with_real_home() {
        let _lock = ENV_LOCK.lock().unwrap();
        let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
            return;
        };
        let caches = home.join("Library").join("Caches");
        if !caches.exists() {
            return; // 非 macOS CI 环境跳过
        }
        let resolved = resolve_scan_root("~/Library/Caches").unwrap();
        assert_eq!(resolved, caches);
        let result = slimit_core::scan_with_progress(&resolved, &|_| {}).unwrap();
        assert_eq!(result.root, caches);
    }
}
