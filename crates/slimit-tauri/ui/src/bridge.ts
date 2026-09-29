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
}

export interface PlanItem {
  rule_id: string;
  path: string;
  estimated_bytes: number;
  risk: "green" | "yellow" | "red";
  executable: boolean;
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
}

export interface Explanation {
  what: string;
  producer: string;
  consequence: string;
  suggested_risk: "green" | "yellow" | "red";
  confidence: number;
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
}
