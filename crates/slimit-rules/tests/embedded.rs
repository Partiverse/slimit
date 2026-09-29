//! 嵌入规则快照与目录加载的一致性测试：GUI 用 `embedded_rules()`，CLI 用
//! `load_rules()`，两者必须产出同一规则库，否则两个入口行为分叉。

use std::collections::BTreeMap;

use slimit_rules::{embedded_rules, load_rules};
use std::path::Path;

#[test]
fn embedded_matches_directory_load() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("manifest dir should have two parents")
        .to_path_buf();
    let from_dir = load_rules(&repo_root.join("rules")).expect("load_rules");
    let embedded = embedded_rules().expect("embedded_rules");

    let by_id = |rules: &[slimit_rules::Rule]| -> BTreeMap<String, String> {
        rules
            .iter()
            .map(|r| (r.id.clone(), format!("{:?}/{:?}", r.risk, r.action.kind)))
            .collect()
    };
    assert_eq!(by_id(&from_dir), by_id(&embedded));
    assert!(!embedded.is_empty());
}
