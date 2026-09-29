use crate::audit::AuditLog;
use crate::plan::PlanItem;
use crate::quarantine::{Manifest, Quarantine};

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("target missing: {0}")]
    TargetMissing(std::path::PathBuf),
    #[error("item not executable (risk={risk}): {path}")]
    NotExecutable {
        risk: String,
        path: std::path::PathBuf,
    },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("restore: original gone and manifest unreadable: {0}")]
    Unrestorable(String),
}

#[derive(Debug, serde::Serialize)]
pub struct ApplyReport {
    pub item: PlanItem,
    /// 隔离条目 id（成功时）。
    pub quarantine_id: Option<String>,
    pub error: Option<String>,
}

/// 逐项执行计划：仅执行 `executable` 项。`purge-dir` = 原子 rename 进隔离区
/// + manifest 落盘 + 审计日志。任何单项失败不中断其余项（报告逐项记录）。
///
/// 安全不变量：
/// - rename 目标永远在隔离区内；隔离区外的任何路径绝不被写入。
/// - `delete_contents_only` 的"保留目录本身"语义由 manifest+restore 保证：
///   迁走的是整个目录，恢复时原样回来，等价于"内容清空后目录保留"在
///   用户视角不可区分（目录 inode 变化——若上游发现应用依赖目录 inode，
///   W4 换成逐文件迁移方案）。
pub fn apply(
    items: &[PlanItem],
    quarantine: &Quarantine,
    audit: &mut AuditLog,
) -> Vec<ApplyReport> {
    let mut reports = Vec::new();
    for item in items {
        if !item.executable {
            reports.push(ApplyReport {
                item: item.clone(),
                quarantine_id: None,
                error: Some(format!("not executable (risk={:?})", item.risk)),
            });
            continue;
        }
        match purge_into_quarantine(item, quarantine) {
            Ok(id) => {
                audit.record(&serde_json::json!({
                    "event": "quarantine",
                    "rule_id": item.rule_id,
                    "path": item.path,
                    "bytes": item.estimated_bytes,
                    "quarantine_id": id,
                }));
                reports.push(ApplyReport {
                    item: item.clone(),
                    quarantine_id: Some(id),
                    error: None,
                });
            }
            Err(e) => {
                audit.record(&serde_json::json!({
                    "event": "quarantine-failed",
                    "rule_id": item.rule_id,
                    "path": item.path,
                    "error": e.to_string(),
                }));
                reports.push(ApplyReport {
                    item: item.clone(),
                    quarantine_id: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    reports
}

fn purge_into_quarantine(item: &PlanItem, q: &Quarantine) -> Result<String, ApplyError> {
    if !item.path.exists() {
        return Err(ApplyError::TargetMissing(item.path.clone()));
    }
    let (id, manifest) = q.build_manifest(&item.path, &item.rule_id, item.estimated_bytes);
    let dest = q.entry_dir(&id);
    std::fs::create_dir_all(&dest)?;

    // 同卷 rename 原子迁移；跨卷（EXDEV）MVP 直接报错，UI 提示（v1.1 做 copy+delete fallback）。
    match std::fs::rename(&item.path, dest.join("payload")) {
        Ok(()) => {}
        Err(e) if e.raw_os_error() == Some(18 /* EXDEV */) => {
            return Err(ApplyError::Io(std::io::Error::new(
                std::io::ErrorKind::CrossesDevices,
                "cross-volume move not supported in MVP; target left untouched",
            )));
        }
        Err(e) => return Err(e.into()),
    }

    let text = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(q.manifest_path(&id), text)?;
    Ok(id)
}

/// 按 manifest 完整恢复。原路径已存在时生成带后缀的新路径，绝不覆盖。
pub fn restore(quarantine: &Quarantine, id: &str) -> Result<std::path::PathBuf, ApplyError> {
    let manifest: Manifest = quarantine.read_manifest(id)?;
    let payload = quarantine.entry_dir(id).join("payload");
    if !payload.exists() {
        return Err(ApplyError::Unrestorable(format!(
            "payload missing for {id}"
        )));
    }
    let original = &manifest.original_path;
    let dest = if original.exists() {
        // 原路径被占：追加后缀，绝不覆盖用户数据。
        let mut n = 1;
        loop {
            let candidate = original.with_extension(format!(
                "{}.slimit-restored-{n}",
                original
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default()
            ));
            if !candidate.exists() {
                break candidate;
            }
            n += 1;
        }
    } else {
        original.clone()
    };

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&payload, &dest)?;
    // 清空隔离条目（manifest 一并移除）。
    std::fs::remove_dir_all(quarantine.entry_dir(id))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditLog;
    use slimit_rules::matcher::{DirSnapshot, Match};
    use slimit_rules::{ActionKind, Risk, Rule};
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

    fn make_item(id: &str, risk: Risk, kind: ActionKind, path: &Path, bytes: u64) -> PlanItem {
        let r = rule(id, risk, kind, path);
        let m = Match {
            rule_id: id.to_string(),
            path: path.to_path_buf(),
            actual_bytes: bytes,
            apparent_bytes: bytes,
        };
        crate::plan::plan(&[r], &[m]).remove(0)
    }

    #[test]
    fn plan_never_executes_red() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("archives");
        std::fs::create_dir_all(&target).unwrap();
        let item = make_item(
            "macos-xcode-archives",
            Risk::Red,
            ActionKind::Advise,
            &target,
            100,
        );
        assert!(!item.executable);
    }

    #[test]
    fn apply_quarantine_then_restore_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("caches");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("f.bin"), vec![0u8; 512]).unwrap();

        let item = make_item(
            "macos-test-cache",
            Risk::Green,
            ActionKind::PurgeDir,
            &target,
            512,
        );
        let q = Quarantine::new(&tmp.path().join("slimit"));
        let mut audit = AuditLog::new(&tmp.path().join("slimit")).unwrap();

        let reports = apply(&[item], &q, &mut audit);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].error.is_none());
        let qid = reports[0].quarantine_id.clone().unwrap();
        assert!(!target.exists(), "target should be moved into quarantine");

        let restored = restore(&q, &qid).unwrap();
        assert_eq!(restored, target);
        assert!(target.join("f.bin").exists(), "content fully restored");
        assert!(!q.entry_dir(&qid).exists(), "quarantine entry cleaned");
    }

    #[test]
    fn restore_never_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("caches");
        std::fs::create_dir_all(&target).unwrap();

        let item = make_item(
            "macos-test-cache",
            Risk::Green,
            ActionKind::PurgeDir,
            &target,
            0,
        );
        let q = Quarantine::new(&tmp.path().join("slimit"));
        let mut audit = AuditLog::new(&tmp.path().join("slimit")).unwrap();
        let qid = apply(&[item], &q, &mut audit)[0]
            .quarantine_id
            .clone()
            .unwrap();

        // 原路径重建（模拟应用又生成了目录）。
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("new"), b"new").unwrap();

        let restored = restore(&q, &qid).unwrap();
        assert_ne!(restored, target, "must not overwrite");
        assert!(restored.to_string_lossy().contains("slimit-restored"));
        assert_eq!(std::fs::read(target.join("new")).unwrap(), b"new");
    }

    #[test]
    fn missing_target_reported_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let ghost = tmp.path().join("ghost");
        let item = make_item(
            "macos-test-cache",
            Risk::Green,
            ActionKind::PurgeDir,
            &ghost,
            0,
        );
        let q = Quarantine::new(&tmp.path().join("slimit"));
        let mut audit = AuditLog::new(&tmp.path().join("slimit")).unwrap();
        let reports = apply(&[item], &q, &mut audit);
        assert!(reports[0].error.is_some());
        assert!(reports[0].quarantine_id.is_none());
    }

    #[test]
    fn snapshot_conversion_and_plan() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("caches");
        std::fs::create_dir_all(&target).unwrap();
        let snap = DirSnapshot::new(&target, 1000, 800);
        let r = rule(
            "macos-test-cache",
            Risk::Green,
            ActionKind::PurgeDir,
            &target,
        );
        let items = crate::plan::plan_from_snapshots(&[r], &[snap]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].estimated_bytes, 800);
        assert!(items[0].executable);
    }
}
