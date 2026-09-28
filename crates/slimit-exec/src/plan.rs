use slimit_rules::matcher::{Match, DirSnapshot};
use slimit_rules::{ActionKind, Risk, Rule};

/// 清理计划单项。确认前不产生任何副作用。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlanItem {
    pub rule_id: String,
    pub path: std::path::PathBuf,
    /// 预计回收字节（真实占用口径）。
    pub estimated_bytes: u64,
    pub risk: Risk,
    /// 可执行的动作：green/yellow 的 purge-dir 进隔离区；command/advise 不由本 crate 执行。
    pub executable: bool,
}

/// 从规则命中生成执行计划。
///
/// 红线（不可协商）：
/// - `risk: red` 永不生成可执行项（仅提示）。
/// - `advise` 永不执行。
/// - 未识别路径根本不进入本函数（默认安全在上层保证）。
pub fn plan(rules: &[Rule], matches: &[Match]) -> Vec<PlanItem> {
    let mut items = Vec::new();
    for m in matches {
        let Some(rule) = rules.iter().find(|r| r.id == m.rule_id) else { continue };
        let executable = match (rule.risk, rule.action.kind) {
            (Risk::Red, _) => false,
            (Risk::Green | Risk::Yellow, ActionKind::PurgeDir) => true,
            (Risk::Green | Risk::Yellow, ActionKind::Command | ActionKind::Advise) => false,
        };
        items.push(PlanItem {
            rule_id: m.rule_id.clone(),
            path: m.path.clone(),
            estimated_bytes: m.actual_bytes,
            risk: rule.risk,
            executable,
        });
    }
    items.sort_by(|a, b| b.estimated_bytes.cmp(&a.estimated_bytes));
    items
}

/// 便捷转换：目录快照 + 规则 → 计划（matcher → planner 的一步封装）。
pub fn plan_from_snapshots(
    rules: &[Rule],
    dirs: &[DirSnapshot],
) -> Vec<PlanItem> {
    let matches = slimit_rules::match_rules(rules, dirs);
    plan(rules, &matches)
}
