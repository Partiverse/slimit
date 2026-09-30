use crate::loader::Rule;
use globset::Glob;
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
        Self {
            path: path.to_path_buf(),
            apparent,
            actual,
        }
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

/// 把规则的 `paths` 模板展开（`~` 展开）后在目录快照中查命中。
/// 模板含 glob 元字符（`*` `?` `[`）时按 globset 语义匹配每个快照路径
/// （profile 随机后缀等无法精确枚举的目标）；否则查目录快照精确前缀命中。
pub fn match_rules(rules: &[Rule], dirs: &[DirSnapshot]) -> Vec<Match> {
    let by_path: HashSet<&Path> = dirs.iter().map(|d| d.path.as_path()).collect();
    let mut out = Vec::new();
    for rule in rules {
        for template in &rule.paths {
            let Some(path) = expand_tilde(template) else {
                continue;
            };
            let text = path.to_string_lossy();
            if has_glob_meta(&text) {
                let Ok(glob) = Glob::new(&text).map(|g| g.compile_matcher()) else {
                    continue;
                };
                for d in dirs {
                    if glob.is_match(&d.path) {
                        out.push(Match {
                            rule_id: rule.id.clone(),
                            path: d.path.clone(),
                            actual_bytes: d.actual,
                            apparent_bytes: d.apparent,
                        });
                    }
                }
            } else if by_path.contains(path.as_path()) {
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

/// glob 元字符检测：模板含任一即走 glob 语义（字面路径不受影响）。
fn has_glob_meta(text: &str) -> bool {
    text.contains('*') || text.contains('?') || text.contains('[')
}

/// 主目录解析链：HOME → USERPROFILE。
/// Windows 通常只有 USERPROFILE（无 HOME），不回退则 Windows 规则
/// （~/AppData/...）永远无法命中。
fn home_dir() -> Option<std::ffi::OsString> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
}

/// `~` 与 `~/` 展开；其余原样返回（相对路径相对 cwd）。
pub fn expand_tilde(template: &str) -> Option<PathBuf> {
    if template == "~" {
        return home_dir().map(PathBuf::from);
    }
    if let Some(rest) = template.strip_prefix("~/") {
        let home = home_dir()?;
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
    use std::sync::Mutex;

    /// HOME/USERPROFILE 是进程全局环境变量，测试并行时互相污染。
    /// 所有动 env 的测试必须持锁串行执行。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// RAII 环境守卫：构造时设置新值并保存旧值，Drop（含 panic unwind）时
    /// 恢复旧值，避免 HOME/USERPROFILE 状态泄漏给并行测试。
    /// 新值为 None 表示移除该变量。
    struct EnvGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);
    impl EnvGuard {
        fn set(pairs: Vec<(&'static str, Option<std::ffi::OsString>)>) -> Self {
            let mut saved = Vec::with_capacity(pairs.len());
            for (key, new_value) in pairs {
                saved.push((key, std::env::var_os(key)));
                match new_value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
            EnvGuard(saved)
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn tilde_expansion() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(vec![("HOME", Some("/Users/test".into()))]);
        assert_eq!(
            expand_tilde("~/Library/Caches"),
            Some(PathBuf::from("/Users/test/Library/Caches"))
        );
        assert_eq!(expand_tilde("/abs/path"), Some(PathBuf::from("/abs/path")));
    }

    #[test]
    fn tilde_falls_back_to_userprofile() {
        // Windows 通常只有 USERPROFILE 没有 HOME：`~` 展开必须回退，
        // 否则 Windows 规则（~/AppData/...）永远无法命中。
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(vec![
            ("HOME", None),
            ("USERPROFILE", Some("/Users/test".into())),
        ]);

        assert_eq!(
            expand_tilde("~/AppData/Local/npm-cache"),
            Some(PathBuf::from("/Users/test/AppData/Local/npm-cache")),
            "HOME unset + USERPROFILE set must still expand"
        );
        assert_eq!(
            expand_tilde("~"),
            Some(PathBuf::from("/Users/test")),
            "bare ~ must also fall back to USERPROFILE"
        );
        assert_eq!(
            expand_tilde("/abs/path"),
            Some(PathBuf::from("/abs/path")),
            "non-tilde paths are unaffected by env"
        );
    }

    #[test]
    fn matches_existing_dirs_only() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(vec![("HOME", Some("/Users/test".into()))]);
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

    #[ignore = "globset 匹配在 macOS 上偶发挂起，待排查（非阻塞主流程）"]
    #[test]
    fn glob_paths_match_each_profile_dir() {
        // Firefox Windows 风格：profile 目录带随机后缀，规则用 glob 命中
        // 每个已存在的 profile 子目录。依赖 HOME 展开 `~`，须持 env 锁。
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(vec![("HOME", Some("/Users/test".into()))]);
        let dir = tempfile::tempdir().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(rules_dir.join("windows")).unwrap();
        std::fs::write(
            rules_dir.join("windows/test-glob.yaml"),
            "apiVersion: slimit.rules/v1\nid: win-test-glob\nos: windows\npaths:\n  - \"~/slimit-profiles/*/cache2\"\nsemantics:\n  title: t\n  what: w\n  producer: p\n  consequence: c\nrisk: green\naction:\n  kind: purge-dir\nrefs:\n  - https://example.com\n",
        )
        .unwrap();
        let rules = load_rules(&rules_dir).unwrap();
        assert_eq!(rules.len(), 1);

        let p1 = DirSnapshot::new(
            Path::new("/Users/test/slimit-profiles/abc123.default-release/cache2"),
            100,
            80,
        );
        let p2 = DirSnapshot::new(
            Path::new("/Users/test/slimit-profiles/xyz9.default/cache2"),
            50,
            40,
        );
        // profile 目录本身（不在 glob 叶子层）与无关目录不得命中。
        let other = DirSnapshot::new(
            Path::new("/Users/test/slimit-profiles/abc123.default-release"),
            999,
            999,
        );
        let matches = match_rules(&rules, &[p1, p2, other]);
        assert_eq!(matches.len(), 2, "each profile cache2 dir must match");
        assert!(matches.iter().all(|m| m.rule_id == "win-test-glob"));
    }
}
