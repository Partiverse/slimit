use slimit_rules::matcher::{DirSnapshot, Match};
use slimit_rules::{ActionKind, Risk, Rule};
use std::collections::HashSet;

/// 清理计划单项。确认前不产生任何副作用。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
        let Some(rule) = rules.iter().find(|r| r.id == m.rule_id) else {
            continue;
        };
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
pub fn plan_from_snapshots(rules: &[Rule], dirs: &[DirSnapshot]) -> Vec<PlanItem> {
    let matches = slimit_rules::match_rules(rules, dirs);
    plan(rules, &matches)
}

/// 服务端重校验：执行授权只来自规则库（SPEC §5 红线）。`apply_plan` 收到的
/// [`PlanItem`] 来自 IPC——`executable` 是客户端自证。本函数按规则库重新
/// 推导可执行性（复用 [`plan_from_snapshots`] 的 canonical 逻辑）：伪造、
/// 未知 rule_id 或 red 规则命中的项一律降级为不可执行。
///
/// 对正常流程（scan_and_plan 产出的计划原样传回）幂等——重校验全部通过。
pub fn authorize_items(items: Vec<PlanItem>, rules: &[Rule]) -> Vec<PlanItem> {
    let dirs: Vec<DirSnapshot> = items
        .iter()
        .map(|i| DirSnapshot::new(&i.path, i.estimated_bytes, i.estimated_bytes))
        .collect();
    let authorized: HashSet<(String, std::path::PathBuf)> = plan_from_snapshots(rules, &dirs)
        .into_iter()
        .filter(|p| p.executable)
        .map(|p| (p.rule_id, p.path))
        .collect();
    items
        .into_iter()
        .map(|mut i| {
            if i.executable && !authorized.contains(&(i.rule_id.clone(), i.path.clone())) {
                i.executable = false;
            }
            i
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn rule(id: &str, risk: Risk, kind: ActionKind, path: &Path) -> Rule {
        serde_json::from_value(serde_json::json!({
            "apiVersion": "slimit.rules/v1",
            "id": id,
            "os": "macos",
            "paths": [path.to_string_lossy()],
            "risk": match risk { Risk::Green => "green", Risk::Yellow => "yellow", Risk::Red => "red" },
            "action": { "kind": match kind { ActionKind::PurgeDir => "purge-dir", ActionKind::Command => "command", ActionKind::Advise => "advise" } },
            "refs": ["https://example.com"]
        }))
        .unwrap()
    }

    #[test]
    fn authorize_items_keeps_rule_authorized_and_downgrades_forged() {
        // 红线（SPEC §5）：执行授权只来自规则库。apply_plan 收到的 PlanItem
        // 来自 IPC——`executable` 是客户端自证，路径不在规则库授权范围内时
        // 必须降级为不可执行，且不产生任何副作用。
        let tmp = tempfile::tempdir().unwrap();
        let legit = tmp.path().join("caches");
        std::fs::create_dir_all(&legit).unwrap();
        let rules = vec![rule(
            "macos-test-cache",
            Risk::Green,
            ActionKind::PurgeDir,
            &legit,
        )];

        let legit_item = PlanItem {
            rule_id: "macos-test-cache".into(),
            path: legit.clone(),
            estimated_bytes: 10,
            risk: Risk::Green,
            executable: true,
        };
        // 伪造项：executable=true 但路径不在规则库 paths 内。
        let forged_path = tmp.path().join("documents");
        std::fs::create_dir_all(&forged_path).unwrap();
        let forged = PlanItem {
            rule_id: "macos-test-cache".into(),
            path: forged_path.clone(),
            estimated_bytes: 100,
            risk: Risk::Green,
            executable: true,
        };

        let out = authorize_items(vec![legit_item, forged], &rules);
        assert!(out[0].executable, "rule-authorized item stays executable");
        assert!(!out[1].executable, "forged path must be downgraded");
        assert!(legit.exists() && forged_path.exists(), "no side effects");
    }

    #[test]
    fn authorize_items_downgrades_unknown_rule_id_and_red() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("caches");
        std::fs::create_dir_all(&dir).unwrap();
        let rules = vec![rule(
            "macos-red-advise",
            Risk::Red,
            ActionKind::Advise,
            &dir,
        )];

        // 未知 rule_id：规则库无从授权，必须降级。
        let unknown = PlanItem {
            rule_id: "not-in-library".into(),
            path: dir.clone(),
            estimated_bytes: 10,
            risk: Risk::Green,
            executable: true,
        };
        // red 规则永不执行（plan 的红线经 authorize_items 重新落地）。
        let red = PlanItem {
            rule_id: "macos-red-advise".into(),
            path: dir.clone(),
            estimated_bytes: 10,
            risk: Risk::Red,
            executable: true,
        };

        let out = authorize_items(vec![unknown, red], &rules);
        assert!(!out[0].executable, "unknown rule_id must be downgraded");
        assert!(!out[1].executable, "red rule must never be executable");
    }
}
