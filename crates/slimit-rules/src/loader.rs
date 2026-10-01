use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const API_VERSION: &str = "slimit.rules/v1";
const PREFIXES: &[(&str, &str)] = &[
    ("macos", "macos-"),
    ("windows", "win-"),
    ("linux", "linux-"),
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub id: String,
    pub os: String,
    #[serde(default)]
    pub scope: Option<String>,
    pub paths: Vec<String>,
    /// 项目感知匹配（project-artifact 扩展）。与 `paths` 互斥（lint 强制）。
    #[serde(default)]
    pub project: Option<ProjectCfg>,
    #[serde(default)]
    pub match_: Option<MatchCfg>,
    #[serde(default)]
    pub semantics: Option<Semantics>,
    pub risk: Risk,
    #[serde(default)]
    pub red_flags: Vec<String>,
    pub action: Action,
    #[serde(default)]
    pub estimate: Option<Estimate>,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MatchCfg {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// 项目感知匹配配置（设计见 docs/PROJECT-ARTIFACT-DESIGN.md）：
/// 命中「父目录存在 markers 之一、自身目录名 ∈ rel_paths」的目录。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectCfg {
    /// 项目根标记文件（单段文件名），如 `Cargo.toml`。
    pub markers: Vec<String>,
    /// 项目根下的产物目录名（单段），如 `target`。
    pub rel_paths: Vec<String>,
    /// 目录 mtime 距今天数低于该值 ⇒ below_min_age（不可执行，只提示）。
    #[serde(default)]
    pub max_age_days: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Semantics {
    pub title: String,
    #[serde(default)]
    pub what: Option<String>,
    #[serde(default)]
    pub producer: Option<String>,
    #[serde(default)]
    pub safe_to_delete_because: Option<String>,
    #[serde(default)]
    pub consequence: Option<String>,
    #[serde(default)]
    pub regenerate: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Green,
    Yellow,
    Red,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub kind: ActionKind,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub dry_run: Option<String>,
    #[serde(default)]
    pub delete_contents_only: Option<bool>,
    #[serde(default)]
    pub irreversible: Option<bool>,
    #[serde(default)]
    pub requires_root: Option<bool>,
    #[serde(default)]
    pub min_age_days: Option<u32>,
    #[serde(default)]
    pub keep_newest: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    PurgeDir,
    Command,
    Advise,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Estimate {
    #[serde(default)]
    pub typical_size: Option<String>,
    #[serde(default)]
    pub recovery: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("read {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("parse {0}: {1}")]
    Yaml(PathBuf, #[source] serde_yaml::Error),
    #[error("{0}: {1}")]
    Invalid(PathBuf, String),
}

/// 加载规则目录下全部 YAML 规则（顶层或 os 子目录，`_` 前缀跳过）。
/// 同时执行 lint 与 id 查重——规则库是可信输入，任何失败即整体报错。
pub fn load_rules(rules_dir: &Path) -> Result<Vec<Rule>, LoadError> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(rules_dir)
        .map_err(|e| LoadError::Io(rules_dir.to_path_buf(), e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .flat_map(|p| {
            if p.is_dir() {
                match std::fs::read_dir(&p) {
                    Ok(rd) => rd
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .collect::<Vec<_>>(),
                    Err(_) => Vec::new(),
                }
            } else {
                vec![p]
            }
        })
        .filter(|p| {
            p.extension()
                .map(|x| x == "yaml" || x == "yml")
                .unwrap_or(false)
        })
        .filter(|p| {
            !p.file_name()
                .map(|n| n.to_string_lossy().starts_with('_'))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();

    let mut rules = Vec::new();
    for path in entries {
        let text = std::fs::read_to_string(&path).map_err(|e| LoadError::Io(path.clone(), e))?;
        let rule: Rule =
            serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml(path.clone(), e))?;
        crate::lint::lint(&rule).map_err(|msg| LoadError::Invalid(path.clone(), msg))?;
        rules.push(rule);
    }

    let ids: Vec<String> = rules.iter().map(|r| r.id.clone()).collect();
    crate::lint::lint_unique(&ids)
        .map_err(|msg| LoadError::Invalid(rules_dir.to_path_buf(), msg))?;

    Ok(rules)
}

/// 编译期嵌入的规则（build.rs 快照，见 `crate::embedded_rules_gen`）。
/// 与 [`load_rules`] 相同的 lint + 查重管线；规则库是可信输入，任何
/// 失败即整体报错。GUI/单二进制场景免运行时文件路径。
pub fn embedded_rules() -> Result<Vec<Rule>, LoadError> {
    let mut rules = Vec::new();
    for (name, text) in crate::embedded_rules_gen::EMBEDDED_RULES {
        let name = PathBuf::from(name);
        let rule: Rule =
            serde_yaml::from_str(text).map_err(|e| LoadError::Yaml(name.clone(), e))?;
        crate::lint::lint(&rule).map_err(|msg| LoadError::Invalid(name.clone(), msg))?;
        rules.push(rule);
    }

    let ids: Vec<String> = rules.iter().map(|r| r.id.clone()).collect();
    crate::lint::lint_unique(&ids)
        .map_err(|msg| LoadError::Invalid(PathBuf::from("embedded"), msg))?;

    Ok(rules)
}

pub fn expected_prefix(os: &str) -> Option<&'static str> {
    PREFIXES.iter().find(|(o, _)| *o == os).map(|(_, p)| *p)
}
