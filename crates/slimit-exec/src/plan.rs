use slimit_rules::matcher::{DirSnapshot, Match};
use slimit_rules::{ActionKind, Risk, Rule};
use std::collections::HashSet;

/// 净回收持久度（RECLAIM-STRATEGY §3 的 A/B/C 分类，UI 排序与新手档的依据）。
/// 纯分类信息，不参与执行授权——红线由 risk + action.kind 决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Durability {
    /// A 类：一次性大额（项目构建产物等），回收后不复发。
    OneShot,
    /// B 类：周期性再生（缓存/日志等），清完会回来。
    Regenerating,
    /// C 类：用户数据，只提示不代删。
    UserData,
}

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
    /// project 规则：目标目录 mtime 距今天数；路径规则为 None。
    #[serde(default)]
    pub age_days: Option<u64>,
    /// project 规则年龄低于 max_age_days ⇒ true ⇒ 不可执行（只提示）。
    #[serde(default)]
    pub below_min_age: bool,
    /// 净回收持久度分类（UI 排序/新手档用；旧前端缺省为 regenerating）。
    #[serde(default = "default_durability")]
    pub durability: Durability,
}

fn default_durability() -> Durability {
    Durability::Regenerating
}

/// 按规则属性推导持久度分类：red ⇒ C 类用户数据；project 规则 ⇒ A 类
/// 一次性大额；其余路径规则 ⇒ B 类会再生。
fn durability_of(rule: &Rule) -> Durability {
    if rule.risk == Risk::Red {
        Durability::UserData
    } else if rule.project.is_some() {
        Durability::OneShot
    } else {
        Durability::Regenerating
    }
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
            // project 规则年龄不足（或年龄不可得）⇒ 只提示不执行（安全方向单调）。
            (Risk::Green | Risk::Yellow, ActionKind::PurgeDir) => !m.below_min_age,
            (Risk::Green | Risk::Yellow, ActionKind::Command | ActionKind::Advise) => false,
        };
        items.push(PlanItem {
            rule_id: m.rule_id.clone(),
            path: m.path.clone(),
            estimated_bytes: m.actual_bytes,
            risk: rule.risk,
            executable,
            age_days: m.age_days,
            below_min_age: m.below_min_age,
            durability: durability_of(rule),
        });
    }
    // 排序：不可执行的守卫项沉底，其余按净收益（A 类优先于 B 类，同档按字节）。
    items.sort_by(|a, b| {
        a.below_min_age
            .cmp(&b.below_min_age)
            .then_with(|| tier_rank(a.durability).cmp(&tier_rank(b.durability)))
            .then_with(|| b.estimated_bytes.cmp(&a.estimated_bytes))
    });
    items
}

/// 排序权重：A 类(0) < B 类(1) < C 类(2)。C 类恒不可执行，已被
/// below_min_age 之外的 executable 规则挡在执行之外，仅影响展示顺序。
fn tier_rank(d: Durability) -> u8 {
    match d {
        Durability::OneShot => 0,
        Durability::Regenerating => 1,
        Durability::UserData => 2,
    }
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
    fn project_rule_age_guard_controls_executable_and_authorize_replays() {
        // project 规则：年龄守卫决定可执行性；authorize_items 重放（IPC 信任
        // 边界）必须与首算一致——旧 target 保留可执行，伪造项照常降级。
        let tmp = tempfile::tempdir().unwrap();

        let old_root = tmp.path().join("oldproj");
        std::fs::create_dir_all(old_root.join("target")).unwrap();
        std::fs::write(old_root.join("Cargo.toml"), b"x").unwrap();
        let old_target = old_root.join("target");
        let status = std::process::Command::new("touch")
            .args(["-t", "202001010000"])
            .arg(&old_target)
            .status()
            .unwrap();
        assert!(status.success());

        let fresh_root = tmp.path().join("freshproj");
        std::fs::create_dir_all(fresh_root.join("target")).unwrap();
        std::fs::write(fresh_root.join("Cargo.toml"), b"x").unwrap();

        let rule: Rule = serde_json::from_value(serde_json::json!({
            "apiVersion": "slimit.rules/v1",
            "id": "macos-project-cargo-target",
            "os": "macos",
            "paths": [],
            "project": { "markers": ["Cargo.toml"], "rel_paths": ["target"], "max_age_days": 14 },
            "risk": "yellow",
            "action": { "kind": "purge-dir" },
            "refs": ["https://example.com"]
        }))
        .unwrap();

        let items = plan_from_snapshots(
            &[rule.clone()],
            &[
                DirSnapshot::new(&old_target, 100, 80),
                DirSnapshot::new(&fresh_root.join("target"), 100, 80),
            ],
        );
        assert_eq!(items.len(), 2);
        let old = items.iter().find(|i| i.path == old_target).unwrap();
        let fresh = items
            .iter()
            .find(|i| i.path == fresh_root.join("target"))
            .unwrap();
        assert!(old.executable, "old target must be executable");
        assert!(old.age_days.unwrap() > 100);
        assert!(
            !fresh.executable,
            "fresh target must be guarded (below_min_age)"
        );
        assert!(fresh.below_min_age);

        // IPC 信任边界：前端原样传回计划时，重放全部通过（幂等）；
        // 同字节数时排序稳定，old 在前、fresh 在后。
        let out = authorize_items(items, &[rule]);
        assert!(
            out[0].executable,
            "old target stays executable after replay"
        );
        assert_eq!(
            out.iter().filter(|i| i.executable).count(),
            1,
            "only the old target stays executable after replay"
        );
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
            age_days: None,
            below_min_age: false,
            durability: crate::plan::Durability::Regenerating,
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
            age_days: None,
            below_min_age: false,
            durability: crate::plan::Durability::Regenerating,
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
            age_days: None,
            below_min_age: false,
            durability: crate::plan::Durability::Regenerating,
        };
        // red 规则永不执行（plan 的红线经 authorize_items 重新落地）。
        let red = PlanItem {
            rule_id: "macos-red-advise".into(),
            path: dir.clone(),
            estimated_bytes: 10,
            risk: Risk::Red,
            executable: true,
            age_days: None,
            below_min_age: false,
            durability: crate::plan::Durability::Regenerating,
        };

        let out = authorize_items(vec![unknown, red], &rules);
        assert!(!out[0].executable, "unknown rule_id must be downgraded");
        assert!(!out[1].executable, "red rule must never be executable");
    }

    #[test]
    fn durability_classifies_and_orders_above_b_tier() {
        // A/B/C 分类（RECLAIM-STRATEGY §3）：project 规则=A 一次性大额，
        // red=C 用户数据，路径规则=B 会再生。排序把 A 类提到 B 类之前——
        // 一次 80GB 比每周 2GB 安慰剂更值钱。
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("proj");
        let cache = tmp.path().join("caches");
        std::fs::create_dir_all(proj.join("target")).unwrap();
        std::fs::write(proj.join("Cargo.toml"), b"x").unwrap();
        std::fs::create_dir_all(&cache).unwrap();

        let project_rule: Rule = serde_json::from_value(serde_json::json!({
            "apiVersion": "slimit.rules/v1",
            "id": "macos-project-cargo-target",
            "os": "macos",
            "paths": [],
            "project": { "markers": ["Cargo.toml"], "rel_paths": ["target"] },
            "risk": "yellow",
            "action": { "kind": "purge-dir" },
            "refs": ["https://example.com"]
        }))
        .unwrap();
        // 路径规则体积更大——排序仍须把 A 类放前面。
        let path_rule = rule(
            "macos-test-cache",
            Risk::Green,
            ActionKind::PurgeDir,
            &cache,
        );

        let items = plan_from_snapshots(
            &[project_rule, path_rule],
            &[
                DirSnapshot::new(&proj.join("target"), 10_000, 8_000),
                DirSnapshot::new(&cache, 50_000, 40_000),
            ],
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, proj.join("target"), "A class ranks first");
        assert_eq!(items[0].durability, Durability::OneShot);
        assert_eq!(items[1].durability, Durability::Regenerating);

        // red 规则恒为 C 类。
        let red_rule = rule("macos-red-test", Risk::Red, ActionKind::Advise, &cache);
        let red_items = plan_from_snapshots(&[red_rule], &[DirSnapshot::new(&cache, 1, 1)]);
        assert_eq!(red_items[0].durability, Durability::UserData);
        assert!(!red_items[0].executable);
    }
}
