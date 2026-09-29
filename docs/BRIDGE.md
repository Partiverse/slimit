# Tauri 桥接协议（W5）

Rust 侧命令定义于 `crates/slimit-tauri/src/lib.rs`，前端类型化封装于 `crates/slimit-tauri/ui/src/bridge.ts`。**命令名/参数名/字段名即契约，两侧必须同步修改。**

| 命令 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `scan_dir` | `root: string`, `top?: number`（默认 20） | `ScanSummary` | 同步阻塞扫描；前端扫描期间禁用按钮 |
| `volume_summary_cmd` | `mount: string` | `VolumeSummary` | `diskutil info -plist` 摘要 |
| `list_snapshots` | `volume: string` | `SnapshotInfo[]` | `diskutil apfs listSnapshots -plist` |

错误统一走 `Result<T, String>`（`ScanError` 的 `Display`）。

## 数据结构（定义在 slimit-core，serde 默认命名）

```ts
interface DirStat { path: string; apparent: number; actual: number; file_count: number }
interface ScanSummary { root: string; file_count: number; actual: number; apparent: number; top_dirs: DirStat[] }
interface SnapshotInfo { name: string; uuid: string; xid: number; purgeable: boolean; limiting_container_shrink: boolean }
interface VolumeSummary { volume_name: string | null; device_identifier: string | null; free_space: number | null;
  capacity_in_use: number | null; apfs_container_size: number | null; apfs_container_free: number | null;
  system_snapshot_name: string | null }
```

设计约定：

- 前端只接收 `ScanSummary` 聚合，永不传百万级 `files` 明细（`ScanResult::summarize`）。
- purgeable 数值无公开来源，不估算；只透出快照 `purgeable` / `limiting_container_shrink` 标志。
- APFS Data 卷 `free_space` 恒为 0（容器托管口径），容量展示用 `apfs_container_free`。

## 已知限制（W5 骨架）

- 同步阻塞命令，无进度事件流（W6 接入）。
- 长路径输入需绝对路径，前端不做 `~` 展开。
