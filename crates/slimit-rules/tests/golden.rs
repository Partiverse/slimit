//! Golden fixture 集成测试：加载仓库 `rules/` 目录全部规则，
//! 校验不变量后与 `tests/fixtures/golden-rules.json` 精确对比。
//!
//! 更新 fixture：`SLIMIT_UPDATE_GOLDEN=1 cargo test -p slimit-rules`

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use slimit_rules::{load_rules, ActionKind, Risk};

/// 仓库根目录：`CARGO_MANIFEST_DIR` = `<repo>/crates/slimit-rules`。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("manifest dir should have two parents")
        .to_path_buf()
}

#[test]
fn golden_rules() {
    let rules = load_rules(&repo_root().join("rules")).expect("load_rules");

    // 1) 规则总数下限：防止规则库被意外清空或漏打包（不是增长指标，
    //    未来规则增减时按事实人工调整；2026-09-29 实测 27 条）。
    assert!(
        rules.len() >= 27,
        "expected >= 27 rules, got {}",
        rules.len()
    );

    // 2) id 全局唯一（load_rules 内部也查重，这里独立复核）。
    let mut seen: HashSet<&str> = HashSet::new();
    for rule in &rules {
        assert!(seen.insert(rule.id.as_str()), "duplicate rule id '{}'", rule.id);
    }

    // 3) risk=red 的规则只允许 advise（删除类动作必须给出提示而非直接执行）。
    for rule in &rules {
        if rule.risk == Risk::Red {
            assert_eq!(
                rule.action.kind,
                ActionKind::Advise,
                "red rule '{}' must use advise, got {:?}",
                rule.id,
                rule.action.kind
            );
        }
    }

    // 4) command 规则必须有非空 dry_run（无预览的命令不允许执行）。
    for rule in &rules {
        if rule.action.kind == ActionKind::Command {
            let dry_run = rule.action.dry_run.as_deref().unwrap_or("").trim();
            assert!(
                !dry_run.is_empty(),
                "command rule '{}' requires non-empty dry_run",
                rule.id
            );
        }
    }

    // 5) golden fixture：按 loader 顺序序列化为 pretty JSON 后精确对比。
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden-rules.json");
    let actual = serde_json::to_string_pretty(&rules).expect("serialize rules") + "\n";

    if std::env::var("SLIMIT_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(fixture_path.parent().unwrap()).expect("create fixtures dir");
        std::fs::write(&fixture_path, actual).expect("write golden fixture");
        return;
    }

    let expected = std::fs::read_to_string(&fixture_path).unwrap_or_else(|e| {
        panic!(
            "cannot read golden fixture {} (create it with SLIMIT_UPDATE_GOLDEN=1 cargo test -p slimit-rules): {e}",
            fixture_path.display()
        )
    });
    assert!(
        expected == actual,
        "golden fixture mismatch ({}):\n{}",
        fixture_path.display(),
        first_diff(&expected, &actual)
    );
}

/// 找出两份文本的首个差异行，返回 `fixture:` / `actual:` 对照。
fn first_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    for i in 0..expected_lines.len().max(actual_lines.len()) {
        let (e, a) = (
            expected_lines.get(i).copied(),
            actual_lines.get(i).copied(),
        );
        if e == a {
            continue;
        }
        return format!(
            "first difference at line {}:\n  fixture: {}\n  actual:  {}",
            i + 1,
            e.unwrap_or("<EOF>"),
            a.unwrap_or("<EOF>")
        );
    }
    "no per-line difference found".into()
}
