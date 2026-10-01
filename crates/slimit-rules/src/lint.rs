use crate::loader::{expected_prefix, ActionKind, Risk, Rule, API_VERSION};

/// 与 `rules/schema-v1.json` + `validate.py` 等价的 lint 检查。
/// 返回 Err(信息) 表示规则不可信，调用方必须拒绝加载。
pub fn lint(rule: &Rule) -> Result<(), String> {
    if rule.api_version != API_VERSION {
        return Err(format!("apiVersion must be {API_VERSION}"));
    }
    let id_ok = rule
        .id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !id_ok || rule.id.is_empty() {
        return Err(format!("id '{}' must be kebab-case [a-z0-9-]", rule.id));
    }
    match expected_prefix(&rule.os) {
        Some(prefix) if rule.id.starts_with(prefix) => {}
        Some(prefix) => {
            return Err(format!(
                "id '{}' missing prefix '{prefix}' for os {}",
                rule.id, rule.os
            ))
        }
        None => return Err(format!("unsupported os '{}'", rule.os)),
    }
    if rule.project.is_some() {
        lint_project(rule)?;
    } else if rule.paths.is_empty() {
        return Err("paths must not be empty".into());
    }
    if rule.risk == Risk::Red {
        if rule.red_flags.is_empty() {
            return Err("red rule requires red_flags".into());
        }
        if rule.action.kind == ActionKind::PurgeDir {
            return Err("red rule must not use purge-dir".into());
        }
    }
    if rule.action.kind == ActionKind::Command {
        if rule
            .action
            .command
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err("command rule requires command".into());
        }
        if rule
            .action
            .dry_run
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err("command rule requires dry_run (no preview, no entry)".into());
        }
    }
    Ok(())
}

use std::collections::HashSet;

/// project-artifact 规则的额外约束（设计见 docs/PROJECT-ARTIFACT-DESIGN.md §5）。
/// markers/rel_paths 只允许单段名——这是路径注入防线：命中锚在「目录名相等」
/// 上，规则无法把目标引到项目根之外。
fn lint_project(rule: &Rule) -> Result<(), String> {
    let proj = rule.project.as_ref().unwrap();
    if !rule.paths.is_empty() {
        return Err("project rule must have empty paths (mutually exclusive)".into());
    }
    if proj.markers.is_empty() || proj.rel_paths.is_empty() {
        return Err("project rule requires non-empty markers and rel_paths".into());
    }
    for name in proj.markers.iter().chain(proj.rel_paths.iter()) {
        let bad = name.is_empty()
            || name == "."
            || name == ".."
            || name
                .chars()
                .any(|c| c == '/' || c == '\\' || c == '*' || c == '?' || c == '[');
        if bad {
            return Err(format!(
                "project entry '{name}' must be a single path segment (no separators/glob)"
            ));
        }
    }
    if proj.max_age_days == Some(0) {
        return Err("project max_age_days must be >= 1".into());
    }
    if rule.action.kind == ActionKind::Command {
        return Err("project rule must not use command action".into());
    }
    Ok(())
}

/// 规则 id 全局唯一（在 [`crate::loader::load_rules`] 层做库级检查时调用）。
pub fn lint_unique(ids: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(format!("duplicate rule id '{id}'"));
        }
    }
    Ok(())
}
