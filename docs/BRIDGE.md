# Tauri 桥接协议（W6）

Rust 侧命令定义于 `crates/slimit-tauri/src/lib.rs`，前端类型化封装于 `crates/slimit-tauri/ui/src/bridge.ts`。**命令名/参数名/字段名即契约，两侧必须同步修改。**

| 命令 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `scan_and_plan` | `root: string`, `top?: number`（默认 20） | `ScanPlanResponse { summary, plan }` | 扫描 + 嵌入规则匹配一次完成；进度经 `scan-progress` 事件推送 |
| `explain` | `req: ExplanationRequest` | `Explanation` | AI 提示层（启发式降级实现）；**永不影响执行授权** |
| `apply_plan` | `items: PlanItem[]` | `ApplyReport[]` | 仅 `executable` 项迁入隔离区（app data 目录），逐项报告 |
| `restore_item` | `id: string` | `string`（恢复路径） | 按 manifest 完整恢复，绝不覆盖已存在路径 |
| `list_quarantine` | — | `Manifest[]` | 按迁入时间升序；manifest 损坏条目跳过 |
| `volume_summary_cmd` | `mount: string` | `VolumeSummary` | `diskutil info -plist` 摘要 |
| `list_snapshots` | `volume: string` | `SnapshotInfo[]` | `diskutil apfs listSnapshots -plist` |

错误统一走 `Result<T, String>`。

## 事件

| 事件 | 负载 | 说明 |
|---|---|---|
| `scan-progress` | `{ seq: number, files_done: number }` | 遍历中周期回调；`files_done` 为已发现条目累计值（总量未知，是进度而非百分比）；`seq` 用于丢弃过期扫描的事件 |

## 数据结构

core（`slimit-core`）、执行（`slimit-exec`）、AI（`slimit-ai`）的 serde 结构：

```ts
interface DirStat { path: string; apparent: number; actual: number; file_count: number }
interface ScanSummary { root: string; file_count: number; actual: number; apparent: number; top_dirs: DirStat[] }
interface PlanItem { rule_id: string; path: string; estimated_bytes: number; risk: 'green'|'yellow'|'red'; executable: boolean }
interface ScanPlanResponse { summary: ScanSummary; plan: PlanItem[] }
interface ExplanationRequest { path: string; actual_bytes: number; apparent_bytes: number; owner_bundle: string|null; nearest_rule_hits: string[] }
interface Explanation { what: string; producer: string; consequence: string; suggested_risk: 'green'|'yellow'|'red'; confidence: number }
interface ApplyReport { item: PlanItem; quarantine_id: string|null; error: string|null }
interface Manifest { id: string; original_path: string; rule_id: string; quarantined_at: string; actual_bytes: number }
interface SnapshotInfo { name: string; uuid: string; xid: number; purgeable: boolean; limiting_container_shrink: boolean }
interface VolumeSummary { volume_name: string | null; device_identifier: string | null; free_space: number | null;
  capacity_in_use: number | null; apfs_container_size: number | null; apfs_container_free: number | null;
  system_snapshot_name: string | null }
```

设计约定：

- 前端只接收 `ScanSummary` 聚合与 `PlanItem` 命中列表，永不传百万级 `files` 明细。
- 规则库编译期嵌入（`slimit-rules::embedded_rules`，build.rs 快照），GUI 免运行时规则文件路径；与 `load_rules()` 目录加载同管线、同结果（`tests/embedded.rs` 保证一致）。
- 安全红线（SPEC §5）：`risk: red` 与 `advise/command` 项 `executable=false`，前端不可勾选、`apply_plan` 再拒一次（双层保险）；AI 解释只进 UI 提示层；清理动作只做"迁入隔离区"，可完整恢复，AI 永无删除权。
- 隔离区位于 `<app_data_dir>/quarantine/`（identifier `dev.partiverse.slimit`），审计日志 `<app_data_dir>/audit/audit.jsonl`。
- purgeable 数值无公开来源，不估算；只透出快照标志。APFS Data 卷容量展示用 `apfs_container_free`。

## 已知限制

- 扫描为阻塞命令（tauri 同步命令在独立线程执行，不卡 UI 事件循环），但输入框/按钮仍建议扫描期间禁用。
- 长路径输入需绝对路径，前端不做 `~` 展开。
- 跨卷目标（EXDEV）apply 直接报错不执行，UI 提示（v1.1 做 copy+delete fallback）。
