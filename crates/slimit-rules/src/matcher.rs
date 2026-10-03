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
    /// project 规则：目标目录 mtime 距今天数；路径规则恒为 None。
    pub age_days: Option<u64>,
    /// project 规则：年龄低于 max_age_days（或 mtime 不可得）⇒ true，
    /// 上层据此降为不可执行（只提示）。安全方向单调：拿不到年龄按需保护处理。
    pub below_min_age: bool,
}

/// 把规则的 `paths` 模板展开（`~` 展开）后在目录快照中查命中。
/// 模板含 glob 元字符（`*` `?` `[`）时按 globset 语义匹配每个快照路径
/// （profile 随机后缀等无法精确枚举的目标）；否则查目录快照精确前缀命中。
/// project 规则另走项目感知分支：目录名 ∈ rel_paths 且父目录存在 marker。
pub fn match_rules(rules: &[Rule], dirs: &[DirSnapshot]) -> Vec<Match> {
    let by_path: HashSet<&Path> = dirs.iter().map(|d| d.path.as_path()).collect();
    // 平台过滤（2026-10-03 P0 修复）：只匹配当前平台的规则。此前 linux 规则
    // 的 POSIX 路径（~/.gradle/caches 等）在 macOS 上同样存在，会在 mac 上
    // 以 linux 规则的名义命中并执行（隔离区 14 条中 7 条 linux-* 的实锤）。
    let current_prefix = crate::loader::current_os_prefix();
    let mut out = Vec::new();
    for rule in rules {
        if !rule.id.starts_with(current_prefix) {
            continue;
        }
        // project 分支：对「目录名命中」的少数候选做 marker 存在性检查（lstat）。
        // 快照不含文件明细，marker 检查必须回源文件系统；候选数少，成本可忽略。
        if let Some(proj) = &rule.project {
            for d in dirs {
                let Some(name) = d.path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !proj.rel_paths.iter().any(|r| r == name) {
                    continue;
                }
                let Some(root) = d.path.parent() else {
                    continue;
                };
                let has_marker = proj.markers.iter().any(|m| root.join(m).exists());
                // orphan=true 反向匹配：项目已删（无 marker）才算遗留产物。
                if has_marker == proj.orphan {
                    continue;
                }
                let age_days = std::fs::metadata(&d.path)
                    .ok()
                    .and_then(|md| md.modified().ok())
                    .and_then(days_since);
                // mtime 不可得时按需保护处理（below_min_age=true）：年龄无法
                // 担保就不担保；该选择在 authorize 重放下同样成立（单调安全）。
                let below_min_age = match (proj.max_age_days, age_days) {
                    (Some(max), Some(age)) => age < max as u64,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                out.push(Match {
                    rule_id: rule.id.clone(),
                    path: d.path.clone(),
                    actual_bytes: d.actual,
                    apparent_bytes: d.apparent,
                    age_days,
                    below_min_age,
                });
            }
            continue;
        }
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
                        let age_days = std::fs::metadata(&d.path)
                            .ok()
                            .and_then(|md| md.modified().ok())
                            .and_then(days_since);
                        out.push(Match {
                            rule_id: rule.id.clone(),
                            path: d.path.clone(),
                            actual_bytes: d.actual,
                            apparent_bytes: d.apparent,
                            age_days,
                            below_min_age: false,
                        });
                    }
                }
            } else if by_path.contains(path.as_path()) {
                if let Some(d) = dirs.iter().find(|d| d.path == path) {
                    // 年龄对所有规则都尽量给出（种子反馈「存在时间为什么不总是
                    // 显示」）：mtime 是通用元数据；仅 project 规则用它做
                    // below_min_age 守卫，路径规则只作展示证据。
                    let age_days = std::fs::metadata(&d.path)
                        .ok()
                        .and_then(|md| md.modified().ok())
                        .and_then(days_since);
                    out.push(Match {
                        rule_id: rule.id.clone(),
                        path,
                        actual_bytes: d.actual,
                        apparent_bytes: d.apparent,
                        age_days,
                        below_min_age: false,
                    });
                }
            }
        }
    }
    dedup(out)
}

/// 目录 mtime 距今整天数（构建产物目录的 mtime = 最近一次构建活动；
/// APFS atime 不可靠，不用）。
fn days_since(mtime: std::time::SystemTime) -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(mtime)
        .ok()
        .map(|d| d.as_secs() / 86400)
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
    // 中文输入法会打出全角 `～`（U+FF5E）；种子反馈「~ 路径全部无效」的
    // 疑点之一。统一归一为 ASCII `~` 再展开，其余场景不受影响。
    let normalized = if template.contains('～') {
        template.replace('～', "~")
    } else {
        template.to_string()
    };
    if normalized == "~" {
        return home_dir().map(PathBuf::from);
    }
    if let Some(rest) = normalized.strip_prefix("~/") {
        let home = home_dir()?;
        return Some(Path::new(&home).join(rest));
    }
    // Windows 惯用手写 `~\...`（扫描根由用户直接输入，规则库统一 `~/`）。
    if let Some(rest) = normalized.strip_prefix("~\\") {
        let home = home_dir()?;
        return Some(Path::new(&home).join(rest));
    }
    Some(PathBuf::from(normalized))
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
    fn project_rules_match_only_with_marker() {
        // project 规则：目录名 ∈ rel_paths 且父目录存在 marker 才命中；
        // 无 marker 的同名目录（如手工建的 ~/target）天然不命中。
        let dir = tempfile::tempdir().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(rules_dir.join("macos")).unwrap();
        std::fs::write(
            rules_dir.join("macos/test-project.yaml"),
            "apiVersion: slimit.rules/v1\nid: macos-project-test-target\nos: macos\npaths: []\nproject:\n  markers:\n    - Cargo.toml\n  rel_paths:\n    - target\nsemantics:\n  title: t\n  what: w\n  producer: p\n  consequence: c\nrisk: yellow\naction:\n  kind: purge-dir\nrefs:\n  - https://example.com\n",
        )
        .unwrap();
        let rules = load_rules(&rules_dir).unwrap();
        assert_eq!(rules.len(), 1);

        let proj = dir.path().join("proj");
        std::fs::create_dir_all(proj.join("target")).unwrap();
        std::fs::write(proj.join("Cargo.toml"), b"[package]\n").unwrap();
        let decoy = dir.path().join("decoy");
        std::fs::create_dir_all(decoy.join("target")).unwrap();

        let matches = match_rules(
            &rules,
            &[
                DirSnapshot::new(&proj.join("target"), 100, 80),
                DirSnapshot::new(&decoy.join("target"), 100, 80),
            ],
        );
        assert_eq!(matches.len(), 1, "decoy without marker must not match");
        assert_eq!(matches[0].path, proj.join("target"));
        assert_eq!(matches[0].actual_bytes, 80);
        assert_eq!(matches[0].age_days, Some(0), "fresh dir is 0 days old");
        assert!(!matches[0].below_min_age, "no max_age_days set ⇒ no guard");
    }

    #[test]
    fn project_age_guard_blocks_fresh_and_allows_old() {
        // max_age_days=14：新鲜 target 不得执行（below_min_age），mtime 拨回
        // 2020 年的可执行候选。mtime 口径 = 目录最近一次构建活动。
        let dir = tempfile::tempdir().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(rules_dir.join("macos")).unwrap();
        std::fs::write(
            rules_dir.join("macos/test-project.yaml"),
            "apiVersion: slimit.rules/v1\nid: macos-project-test-target\nos: macos\npaths: []\nproject:\n  markers:\n    - Cargo.toml\n  rel_paths:\n    - target\n  max_age_days: 14\nsemantics:\n  title: t\n  what: w\n  producer: p\n  consequence: c\nrisk: yellow\naction:\n  kind: purge-dir\nrefs:\n  - https://example.com\n",
        )
        .unwrap();
        let rules = load_rules(&rules_dir).unwrap();

        let fresh_root = dir.path().join("fresh");
        std::fs::create_dir_all(fresh_root.join("target")).unwrap();
        std::fs::write(fresh_root.join("Cargo.toml"), b"x").unwrap();

        let old_root = dir.path().join("old");
        std::fs::create_dir_all(old_root.join("target")).unwrap();
        std::fs::write(old_root.join("Cargo.toml"), b"x").unwrap();
        let old_target = old_root.join("target");
        // touch -t 在 macOS/Linux 均可用（CI rust 任务仅 macos-14）。
        let status = std::process::Command::new("touch")
            .args(["-t", "202001010000"])
            .arg(&old_target)
            .status()
            .unwrap();
        assert!(status.success(), "touch -t must work in test env");

        let matches = match_rules(
            &rules,
            &[
                DirSnapshot::new(&fresh_root.join("target"), 10, 8),
                DirSnapshot::new(&old_target, 10, 8),
            ],
        );
        assert_eq!(matches.len(), 2);
        let fresh = matches
            .iter()
            .find(|m| m.path == fresh_root.join("target"))
            .unwrap();
        let old = matches.iter().find(|m| m.path == old_target).unwrap();
        assert!(fresh.below_min_age, "fresh target must be guarded");
        assert!(
            !old.below_min_age,
            "old target must be executable-candidate"
        );
        assert!(old.age_days.unwrap() > 100);
    }

    #[test]
    fn project_orphan_matches_only_without_marker() {
        // 孤儿产物（种子反馈：项目删了 node_modules 还躺在磁盘上）：
        // 父目录没有 package.json 才命中；项目还在（有 marker）不命中。
        let dir = tempfile::tempdir().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(rules_dir.join("macos")).unwrap();
        std::fs::write(
            rules_dir.join("macos/test-orphan.yaml"),
            "apiVersion: slimit.rules/v1\nid: macos-project-test-orphan\nos: macos\npaths: []\nproject:\n  markers:\n    - package.json\n  rel_paths:\n    - node_modules\n  max_age_days: 30\n  orphan: true\nsemantics:\n  title: t\n  what: w\n  producer: p\n  consequence: c\nrisk: yellow\naction:\n  kind: purge-dir\nrefs:\n  - https://example.com\n",
        )
        .unwrap();
        let rules = load_rules(&rules_dir).unwrap();
        assert_eq!(rules.len(), 1);

        let orphan_root = dir.path().join("deleted-proj");
        std::fs::create_dir_all(orphan_root.join("node_modules")).unwrap();
        let live_root = dir.path().join("live-proj");
        std::fs::create_dir_all(live_root.join("node_modules")).unwrap();
        std::fs::write(live_root.join("package.json"), b"{}").unwrap();

        let matches = match_rules(
            &rules,
            &[
                DirSnapshot::new(&orphan_root.join("node_modules"), 100, 80),
                DirSnapshot::new(&live_root.join("node_modules"), 100, 80),
            ],
        );
        assert_eq!(matches.len(), 1, "live project must not match orphan rule");
        assert_eq!(matches[0].path, orphan_root.join("node_modules"));
        assert!(
            matches[0].below_min_age,
            "fresh orphan is still age-guarded"
        );
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

    #[test]
    fn glob_paths_match_each_profile_dir() {
        // 注：match_rules 会过滤非当前平台规则（id 前缀须为 macos-），
        // glob 机制本身与平台无关，用 macos- 前缀测试。
        // Firefox Windows 风格：profile 目录带随机后缀，规则用 glob 命中
        // 每个已存在的 profile 子目录。依赖 HOME 展开 `~`，须持 env 锁。
        // 注：曾因"疑在 macOS 偶发挂起"被 #[ignore]，后排查证实为假阳性
        // （排查脚本依赖的 `timeout` 命令在本机不存在，静默失败被误判为
        // 挂起）；25 轮真实超时验证（含并行 + 全 workspace）均 0.00s 通过。
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(vec![("HOME", Some("/Users/test".into()))]);
        let dir = tempfile::tempdir().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(rules_dir.join("macos")).unwrap();
        std::fs::write(
            rules_dir.join("macos/test-glob.yaml"),
            "apiVersion: slimit.rules/v1\nid: macos-test-glob\nos: macos\npaths:\n  - \"~/slimit-profiles/*/cache2\"\nsemantics:\n  title: t\n  what: w\n  producer: p\n  consequence: c\nrisk: green\naction:\n  kind: purge-dir\nrefs:\n  - https://example.com\n",
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
        assert!(matches.iter().all(|m| m.rule_id == "macos-test-glob"));
    }
}
