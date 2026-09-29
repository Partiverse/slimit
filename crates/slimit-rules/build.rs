//! 生成仓库 `rules/` 目录的编译期嵌入快照（`include_str!` 引用原文件，
//! 不复制内容，改动即重编）。GUI 打包/单二进制场景免运行时文件路径。
//!
//! 与 `loader::load_rules` 同语义：递归收集 yaml/yml，跳过 `_` 前缀
//! （模板/共享片段）。

use std::path::{Path, PathBuf};

fn main() {
    let manifest: PathBuf = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR")
        .into();
    let rules_dir = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("crate sits two levels below repo root")
        .join("rules");
    println!("cargo:rerun-if-changed={}", rules_dir.display());
    let rules_dir = rules_dir
        .canonicalize()
        .expect("rules/ directory must exist at repo root");

    let mut files = Vec::new();
    collect(&rules_dir, &mut files);
    files.sort();

    let mut out = String::from("// 由 build.rs 生成，勿手改：仓库 rules/ 的编译期快照。\n");
    out.push_str("pub const EMBEDDED_RULES: &[(&str, &str)] = &[\n");
    for path in files {
        // 键：rules 目录内的逻辑相对名（报错信息用）；值：绝对路径引用
        // 原文件（规范化的绝对路径不含 ..，且每次构建按本机重新生成）。
        let logical = path
            .strip_prefix(&rules_dir)
            .expect("collected path under rules dir")
            .to_string_lossy()
            .replace('\\', "/");
        let abs = path.canonicalize().expect("canonicalize rule file");
        out.push_str(&format!(
            "    ({logical:?}, include_str!({:?})),\n",
            abs.to_string_lossy()
        ));
    }
    out.push_str("];\n");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    std::fs::write(Path::new(&out_dir).join("embedded_rules.rs"), out)
        .expect("write embedded_rules.rs");
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .map(|x| x == "yaml" || x == "yml")
            .unwrap_or(false)
            && !path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with('_'))
                .unwrap_or(false)
        {
            out.push(path);
        }
    }
}
