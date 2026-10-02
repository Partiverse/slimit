// 与 slimit-tauri/src/lib.rs 的 tauri::command 一一对应的类型化桥接层。
// 命令名/参数名/字段名即协议：改 Rust 侧时必须同步这里。
import { invoke } from "@tauri-apps/api/core";

export interface DirStat {
  path: string;
  apparent: number;
  actual: number;
  file_count: number;
}

export interface ScanSummary {
  root: string;
  file_count: number;
  actual: number;
  apparent: number;
  top_dirs: DirStat[];
  /** 大文件列表（手动清理 .dmg/.pkg 等安装包场景）。 */
  top_files?: FileStat[];
}

export interface FileStat {
  path: string;
  apparent: number;
  actual: number;
}

export interface PlanItem {
  rule_id: string;
  path: string;
  estimated_bytes: number;
  risk: "green" | "yellow" | "red";
  executable: boolean;
  /** project 规则：目标目录 mtime 距今天数；路径规则为 null。 */
  age_days: number | null;
  /** project 规则年龄未达阈值（或 mtime 不可得）⇒ 只提示，executable=false。 */
  below_min_age: boolean;
  /** 净回收持久度：one-shot=一次性大额 / regenerating=会再生 / user-data=用户数据。 */
  durability?: "one-shot" | "regenerating" | "user-data";
  /** 授权来源：rule=规则命中；user-manual=用户手动选择（服务端强制隔离区+保护名单）。 */
  origin?: "rule" | "user-manual";
}

export interface ScanPlanResponse {
  summary: ScanSummary;
  plan: PlanItem[];
}

export interface ExplanationRequest {
  path: string;
  actual_bytes: number;
  apparent_bytes: number;
  owner_bundle: string | null;
  nearest_rule_hits: string[];
  /** project 规则：目标目录 mtime 距今天数；路径规则为 null。 */
  age_days?: number | null;
  /** project 规则年龄未达阈值 ⇒ 解释会明示「暂不建议清理」。 */
  below_min_age?: boolean;
}

export interface Explanation {
  what: string;
  producer: string;
  consequence: string;
  suggested_risk: "green" | "yellow" | "red";
  confidence: number;
  /** 解释来源：rules=规则库 / cloud=云端 AI / heuristic=本地启发式 */
  source: string;
}

/** 云端 AI 设置（OpenAI 兼容端点；enabled=false 时全部离线）。 */
export interface AiSettings {
  enabled: boolean;
  base_url: string;
  api_key: string;
  model: string;
}

export interface ApplyReport {
  item: PlanItem;
  quarantine_id: string | null;
  error: string | null;
}

export interface Manifest {
  id: string;
  original_path: string;
  rule_id: string;
  quarantined_at: string;
  actual_bytes: number;
}

export interface SnapshotInfo {
  name: string;
  uuid: string;
  xid: number;
  purgeable: boolean;
  limiting_container_shrink: boolean;
}

export interface VolumeSummary {
  volume_name: string | null;
  device_identifier: string | null;
  free_space: number | null;
  capacity_in_use: number | null;
  apfs_container_size: number | null;
  apfs_container_free: number | null;
  system_snapshot_name: string | null;
}

export function scanAndPlan(root: string, top = 20): Promise<ScanPlanResponse> {
  return invoke("scan_and_plan", { root, top });
}

export function getSettings(): Promise<AiSettings> {
  return invoke("get_settings");
}

export function setSettings(settings: AiSettings): Promise<void> {
  return invoke("set_settings", { settings });
}

export function explain(req: ExplanationRequest): Promise<Explanation> {
  return invoke("explain", { req });
}

export function applyPlan(items: PlanItem[]): Promise<ApplyReport[]> {
  return invoke("apply_plan", { items });
}

export function restoreItem(id: string): Promise<string> {
  return invoke("restore_item", { id });
}

export function listQuarantine(): Promise<Manifest[]> {
  return invoke("list_quarantine");
}

/** 清理迁入超 14 天（DEFAULT_RETENTION_DAYS）的隔离条目，返回被清理项。
 *  不可逆：UI 必须二次确认后调用。 */
export function purgeExpiredQuarantine(): Promise<Manifest[]> {
  return invoke("purge_expired_quarantine");
}

export function listSnapshots(volume: string): Promise<SnapshotInfo[]> {
  return invoke("list_snapshots", { volume });
}

export function volumeSummary(mount: string): Promise<VolumeSummary> {
  return invoke("volume_summary_cmd", { mount });
}

/** `scan-progress` 事件负载（lib.rs 的 scan_and_plan emit）。 */
export interface ScanProgress {
  seq: number;
  files_done: number;
  current_dir: string;
}

/** 规则库条目（list_rules 返回）。 */
export interface RuleInfo {
  id: string;
  os: "macos" | "linux" | "windows";
  scope: string;
  paths: string[];
  risk: "green" | "yellow" | "red";
  title: string;
  what: string;
  producer: string;
  consequence: string;
  safe_to_delete_because: string;
  regenerate: string;
  typical_size: string;
  recovery: string;
  refs: string[];
}

export function listRules(): Promise<RuleInfo[]> {
  return invoke("list_rules");
}

/** 测试云端 AI 连通性（/models 轻量探测，不耗 token）。返回人话结果或错误。 */
export function testAi(): Promise<string> {
  return invoke("test_ai");
}

/** 打开「完全磁盘访问」系统设置面板（权限引导）。 */
export function openFdaSettings(): Promise<void> {
  return invoke("open_fda_settings");
}

/** 手动清理探测：返回路径存在性与真实占用（目录=聚合全部内容）。 */
export interface ManualProbe {
  path: string;
  is_dir: boolean;
  actual_bytes: number;
  apparent_bytes: number;
}

export function probeManual(path: string): Promise<ManualProbe> {
  return invoke("probe_manual", { path });
}

/** 检测「完全磁盘访问」授权状态（探测 FDA 保护目录可读性）。 */
export function checkFda(): Promise<boolean> {
  return invoke("check_fda");
}

/** 立即彻底删除单个隔离条目（不可逆；UI 必须二次确认后调用）。 */
export function purgeQuarantineItem(id: string): Promise<Manifest> {
  return invoke("purge_quarantine_item", { id });
}
