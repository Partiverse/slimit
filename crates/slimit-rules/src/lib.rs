//! SlimIt 规则引擎。
//!
//! 契约：`rules/schema-v1.json`（slimit.rules/v1）。本 crate 负责
//! 1) 加载规则目录（`_` 前缀跳过）；2) 与 schema 等价的 lint 检查；
//! 3) 把规则 path 模板展开为具体路径并与扫描结果匹配。

pub mod lint;
pub mod loader;
pub mod matcher;

pub use loader::{load_rules, Action, ActionKind, Risk, Rule};
pub use matcher::match_rules;
