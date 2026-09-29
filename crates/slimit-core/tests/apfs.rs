//! W4 APFS 探测：用本机 `diskutil -plist` 真实输出作为 fixture 验证解析。

use slimit_core::{parse_snapshots_plist, parse_volume_info_plist};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/").to_owned() + name)
        .expect("fixture exists")
}

#[test]
fn parse_snapshots_data_volume() {
    let snaps = parse_snapshots_plist(&fixture("snapshots_data.plist")).unwrap();
    assert!(!snaps.is_empty(), "本机 Data 卷有 Time Machine 快照");
    let tm: Vec<_> = snaps
        .iter()
        .filter(|s| s.name.starts_with("com.apple.TimeMachine."))
        .collect();
    assert_eq!(tm.len(), snaps.len());
    // Time Machine 本地快照默认是 purgeable 的
    assert!(tm.iter().all(|s| s.purgeable));
    for s in &snaps {
        assert!(!s.uuid.is_empty());
        assert!(s.xid > 0);
    }
}

#[test]
fn parse_snapshots_empty_ok() {
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict/></plist>"#;
    let snaps = parse_snapshots_plist(xml).unwrap();
    assert!(snaps.is_empty());
}

#[test]
fn parse_snapshots_rejects_garbage() {
    assert!(parse_snapshots_plist(b"not a plist").is_err());
}

#[test]
fn parse_volume_info_data() {
    let v = parse_volume_info_plist(&fixture("volume_data.plist")).unwrap();
    assert_eq!(v.volume_name.as_deref(), Some("Macintosh HD - Data"));
    assert!(v
        .device_identifier
        .as_deref()
        .unwrap_or("")
        .starts_with("disk"));
    assert!(v.apfs_container_free.unwrap_or(0) > 0);
    // APFS Data 卷的 FreeSpace 是容器托管口径，本机实测为 0，不对其断言
    assert!(v.apfs_container_size.unwrap_or(0) >= v.apfs_container_free.unwrap_or(0));
    // Data 卷本身不是快照挂载，不应有系统快照名
    assert!(v.system_snapshot_name.is_none());
}

#[test]
fn parse_volume_info_minimal_keys() {
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>VolumeName</key><string>Test</string>
</dict></plist>"#;
    let v = parse_volume_info_plist(xml).unwrap();
    assert_eq!(v.volume_name.as_deref(), Some("Test"));
    assert_eq!(v.free_space, None);
}
