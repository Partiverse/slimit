use crate::loader::Rule;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 目录占用快照。由上层从 `slimit_core::ScanResult::dirs` 转换（rules 不依赖 core，
/// 依赖方向见 SPEC §1：tauri → exec → rules → core）。
#[derive(Debug, Clone, PartialEq)]
pub struct DirSnapshot {
    pub path: PathBuf,
    pub apparent: u64,
    pub actual: u64,
}

impl DirSnapshot {
    pub fn new(path: &Path, apparent: u64, actual: u64) -> Self {
        Self { path: path.to_path_buf(), apparent, actual }
    }
}

/// 规则命中的目标：规则 + 实际存在的路径 + 该路径真实占用。
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub rule_id: String,
    pub path: PathBuf,
    pub actual_bytes: u64,
    pub apparent_bytes: u64,
}

/// 把规则的 `paths` 模板展开（`~` 展开）后在目录快照中查前缀命中。
/// MVP 仅支持目录前缀匹配；glob 语义在 W3 扩展（globset 已在依赖中）。
pub fn match_rules(rules: &[Rule], dirs: &[DirSnapshot]) -> Vec<Match> {
    let by_path: HashSet<&Path> = dirs.iter().map(|d| d.path.as_path()).collect();
    let mut out = Vec::new();
    for rule in rules {
        for template in &rule.paths {
            let Some(path) = expand_tilde(template) else { continue };
            if by_path.contains(path.as_path()) {
                if let Some(d) = dirs.iter().find(|d| d.path == path) {
                    out.push(Match {
                        rule_id: rule.id.clone(),
                        path,
                        actual_bytes: d.actual,
                        apparent_bytes: d.apparent,
                    });
                }
            }
        }
    }
    dedup(out)
}

/// `~` 与 `~/` 展开；其余原样返回（相对路径相对 cwd）。
pub fn expand_tilde(template: &str) -> Option<PathBuf> {
    if template == "~" {
        return std::env::var_os("HOME").map(PathBuf::from);
    }
    if let Some(rest) = template.strip_prefix("~/") {
        let home = std::env::var_os("HOME")?;
        return Some(Path::new(&home).join(rest));
    }
    Some(PathBuf::from(template))
}

/// 同一 rule+path 只保留一条。
fn dedup(matches: Vec<Match>) -> Vec<Match> {
    let mut seen = HashSet::new();
    matches
        .into_iter()
        .filter(|m| seen.insert((m.rule_id.clone(), m.path.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load_rules;

    #[test]
    fn tilde_expansion() {
        std::env::set_var("HOME", "/Users/test");
        assert_eq!(expand_tilde("~/Library/Caches"), Some(PathBuf::from("/Users/test/Library/Caches")));
        assert_eq!(expand_tilde("/abs/path"), Some(PathBuf::from("/abs/path")));
    }

    #[test]
    fn matches_existing_dirs_only() {
        let dir = tempfile::tempdir().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(rules_dir.join("macos")).unwrap();
        std::fs::write(
            rules_dir.join("macos/test-cache.yaml"),
            "apiVersion: slimit.rules/v1\nid: macos-test-cache\nos: macos\npaths:\n  - \"~/slimit-matcher-test\"\nsemantics:\n  title: t\n  what: w\n  producer: p\n  consequence: c\nrisk: green\naction:\n  kind: purge-dir\nrefs:\n  - https://example.com\n",
        )
        .unwrap();
        let rules = load_rules(&rules_dir).unwrap();
        assert_eq!(rules.len(), 1);

        let existing = DirSnapshot::new(Path::new("/Users/test/slimit-matcher-test"), 100, 80);
        let matches = match_rules(&rules, &[existing]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].actual_bytes, 80);
    }
}
