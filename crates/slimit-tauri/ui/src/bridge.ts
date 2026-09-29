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

export function scanDir(root: string, top = 20): Promise<ScanSummary> {
  return invoke("scan_dir", { root, top });
}

export function listSnapshots(volume: string): Promise<SnapshotInfo[]> {
  return invoke("list_snapshots", { volume });
}

export function volumeSummary(mount: string): Promise<VolumeSummary> {
  return invoke("volume_summary_cmd", { mount });
}
