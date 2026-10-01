//! SlimIt 执行器。
//!
//! 两阶段：[`plan`] 生成 [`PlanItem`] 列表（不落盘），用户确认后 [`apply`]
//! 执行。`purge-dir` 类目标通过 `rename` 迁入隔离区（同卷原子），写
//! manifest 后可完整 [`restore`]；`command` 类 MVP 不执行（UI 层跑官方命令）。

pub mod audit;
pub mod executor;
pub mod plan;
pub mod quarantine;

pub use audit::AuditLog;
pub use executor::{apply, restore, ApplyError, ApplyReport};
pub use plan::{authorize_items, plan, plan_from_snapshots, PlanItem};
pub use quarantine::{Manifest, Quarantine, DEFAULT_RETENTION_DAYS};
