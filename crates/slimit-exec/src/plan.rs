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

/// 条目授权来源。`rule` = 规则库命中（红线①主通道）；`user-manual` =
/// 用户对具体路径的显式手动选择（2026-10-02 应种子反馈新增：删除权在用户，
/// 不在 AI。服务端仍强制：真实存在 + 绝对路径 + 系统路径保护名单 +
/// 隔离区 + 审计 origin 标注；AI 解释永不产生 manual 项）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanOrigin {
    #[default]
    Rule,
    UserManual,
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
    /// 授权来源：规则命中或用户手动选择。
    #[serde(default)]
    pub origin: PlanOrigin,
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
            origin: PlanOrigin::Rule,
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

/// 服务端重校验：执行授权只来自规则库**或用户对具体路径的显式手动选择**
/// （SPEC §5 红线，2026-10-02 语义扩展见 SECURITY.md）。`apply_plan` 收到的
/// [`PlanItem`] 来自 IPC——`executable` 是客户端自证。
///
/// - `origin=rule` 的项：按规则库重新推导可执行性（复用 canonical
///   `match_rules`+`plan` 逻辑），伪造、未知 rule_id 或 red 规则命中的项
///   一律降级为不可执行。
/// - `origin=user-manual` 的项：AI/规则均未参与，授权来自用户本人。服务端
///   重置 risk=Yellow、executable=true，但必须通过 [`is_protected_path`]
///   系统路径保护名单——未过名单的降级为不可执行。手动项同样只做隔离区
///   迁移，可完整恢复。
///
/// 对正常流程（scan_and_plan 产出的计划原样传回）幂等——重校验全部通过。
pub fn authorize_items(items: Vec<PlanItem>, rules: &[Rule]) -> Vec<PlanItem> {
    let rule_items: Vec<PlanItem> = items
        .iter()
        .filter(|i| i.origin == PlanOrigin::Rule)
        .cloned()
        .collect();
    let dirs: Vec<DirSnapshot> = rule_items
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
        .map(|mut i| match i.origin {
            PlanOrigin::UserManual => {
                // 手动项：服务端重写所有客户端自证字段（risk/executable/
                // durability/rule_id），只保留 path；保护名单未过或路径
                // 不存在/相对 ⇒ 降级为不可执行（risk=Red 标记拒绝原因）。
                i.rule_id = "user-manual".into();
                i.durability = Durability::OneShot;
                i.below_min_age = false;
                i.age_days = None;
                if is_protected_path(&i.path) || !i.path.is_absolute() || !i.path.exists() {
                    i.executable = false;
                    i.risk = Risk::Red;
                    return i;
                }
                i.executable = true;
                i.risk = Risk::Yellow;
                i
            }
            PlanOrigin::Rule => {
                if i.executable && !authorized.contains(&(i.rule_id.clone(), i.path.clone())) {
                    i.executable = false;
                }
                i
            }
        })
        .collect()
}

/// 手动清理的系统路径保护名单（2026-10-02）。用户自选 ≠ 可以删系统：
/// 两层防线——
/// - 前缀保护：整个子树都不可手动清理（系统核心目录）；
/// - 自身保护：该路径本身不可作为清理目标，但其子路径可以
///   （如 ~/Downloads 整体挡住，~/Downloads/foo.dmg 允许）。
///
/// 注意：/private 前缀覆盖 macOS 的 /etc /var /tmp（均为其符号链接目标）。
pub fn is_protected_path(path: &std::path::Path) -> bool {
    // canonicalize 归一冗余成分与 symlink（防 `~/Library/../..` 与 /etc→
    // /private/etc 绕过）；失败按保护处理，安全方向单调。
    let Ok(canon) = path.canonicalize() else {
        return true;
    };
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    is_protected_resolved(&canon, home.as_deref())
}

/// [`is_protected_path`] 的纯函数核（对已归一化路径做名单判断）。
/// 独立出来是为了可测：macOS 上所有测试可写路径都在 /private 下，无法
/// 用真实 FS 造出「保护名单外的存在路径」。
fn is_protected_resolved(canon: &std::path::Path, home: Option<&std::path::Path>) -> bool {
    const SYSTEM_PREFIXES: &[&str] = &[
        "/System", "/private", "/usr", "/bin", "/sbin", "/etc", "/var", "/dev",
    ];
    if SYSTEM_PREFIXES
        .iter()
        .any(|s| canon == std::path::Path::new(s) || canon.starts_with(s))
    {
        return true;
    }
    let mut self_protected: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("/"),
        "/Volumes".into(),
        "/Applications".into(),
        "/Library".into(),
        "/opt".into(),
    ];
    if let Some(h) = home {
        self_protected.push(h.to_path_buf());
        for d in [
            "Desktop",
            "Documents",
            "Downloads",
            "Music",
            "Movies",
            "Pictures",
            "Public",
            "Library",
            "Applications",
        ] {
            self_protected.push(h.join(d));
        }
    }
    self_protected.iter().any(|s| canon == *s)
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
    fn manual_items_authorized_with_server_side_overrides() {
        // 手动清理（2026-10-02）：用户显式选择的路径由用户授权——服务端重写
        // 全部自证字段（risk/executable/rule_id/durability），只保留 path。
        // macOS 测试环境的可写临时目录都在 /private 下（受系统前缀保护），
        // 「放行」语义由 is_protected_resolved 纯函数测试覆盖；本测试断言
        // 重写逻辑对保护命中项同样发生（rule_id 归一），只是不可执行。
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("big-installer.dmg");
        std::fs::write(&victim, vec![0u8; 1024]).unwrap();
        assert!(
            is_protected_path(&victim),
            "test env sanity: tempdir lives under /private prefix"
        );

        let manual = PlanItem {
            rule_id: "whatever-forged".into(),
            path: victim.clone(),
            estimated_bytes: 1024,
            risk: Risk::Red,
            executable: false,
            age_days: None,
            below_min_age: false,
            durability: Durability::UserData,
            origin: PlanOrigin::UserManual,
        };
        let out = authorize_items(vec![manual], &[]);
        assert_eq!(out[0].origin, PlanOrigin::UserManual);
        assert_eq!(out[0].rule_id, "user-manual", "server rewrites rule_id");
        assert!(
            !out[0].executable && out[0].risk == Risk::Red,
            "protected (in CI env) manual items are downgraded"
        );
    }

    #[test]
    fn is_protected_resolved_blocks_system_trees_and_top_level_self() {
        // 纯函数核：前缀保护（系统目录整棵子树，含 /etc→/private/etc 的
        // 真实挂载点）；自身保护（顶层容器本身挡住、子路径放行）。
        let home = std::path::Path::new("/Users/tester");
        let yes = |p: &str| {
            assert!(
                is_protected_resolved(std::path::Path::new(p), Some(home)),
                "{p} must be protected"
            );
        };
        let no = |p: &str| {
            assert!(
                !is_protected_resolved(std::path::Path::new(p), Some(home)),
                "{p} must be allowed"
            );
        };
        yes("/System/Library/X");
        yes("/usr/bin");
        yes("/private/etc");
        yes("/private/var/folders/xx/T/thing");
        yes("/Applications");
        yes("/Volumes");
        yes("/");
        yes("/Users/tester");
        yes("/Users/tester/Downloads");
        yes("/Users/tester/Library");
        no("/Applications/Foo.app");
        no("/Users/tester/Downloads/setup.dmg");
        no("/Users/tester/code/proj/target");
        no("/Users/tester/.trae-cn");
    }

    #[test]
    fn manual_items_downgraded_on_missing_or_relative_paths() {
        // 不存在 / 相对路径：手动授权的前提是「用户对真实存在路径的选择」。
        let ghost = PlanItem {
            rule_id: "user-manual".into(),
            path: std::path::PathBuf::from("/definitely/not/here/slimit"),
            estimated_bytes: 0,
            risk: Risk::Yellow,
            executable: true,
            age_days: None,
            below_min_age: false,
            durability: Durability::OneShot,
            origin: PlanOrigin::UserManual,
        };
        let relative = PlanItem {
            rule_id: "user-manual".into(),
            path: std::path::PathBuf::from("relative/dir"),
            estimated_bytes: 0,
            risk: Risk::Yellow,
            executable: true,
            age_days: None,
            below_min_age: false,
            durability: Durability::OneShot,
            origin: PlanOrigin::UserManual,
        };
        let out = authorize_items(vec![ghost, relative], &[]);
        assert!(!out[0].executable && !out[1].executable);
        assert_eq!(out[0].risk, Risk::Red, "downgraded items read as red");
    }

    #[test]
    fn is_protected_path_wrapper_semantics() {
        // 包装层：canonicalize 失败（不存在）按保护处理，安全方向单调；
        // 真实系统树受前缀保护。名单细节（含子路径放行）由
        // is_protected_resolved 纯函数测试覆盖。
        assert!(is_protected_path(std::path::Path::new(
            "/definitely/not/here"
        )));
        assert!(is_protected_path(std::path::Path::new("/System/Library")));
        assert!(is_protected_path(std::path::Path::new("/usr/bin")));
        assert!(is_protected_path(std::path::Path::new("/Applications")));
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
            origin: crate::plan::PlanOrigin::Rule,
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
            origin: crate::plan::PlanOrigin::Rule,
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
            origin: crate::plan::PlanOrigin::Rule,
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
            origin: crate::plan::PlanOrigin::Rule,
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
