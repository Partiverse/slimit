use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 隔离区：`<base>/quarantine/<id>/`，每个条目一个目录 + manifest.json。
#[derive(Debug, Clone)]
pub struct Quarantine {
    base: PathBuf,
}

/// manifest 记录恢复所需的全部信息。删条目前提是 manifest 可信。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    /// 原始路径（rename 回去的唯一依据）。
    pub original_path: PathBuf,
    pub rule_id: String,
    /// 迁入时间（MVP：unix 秒串，见 `now_rfc3339`；用于审计与到期清理）。
    pub quarantined_at: String,
    /// 迁入时真实占用字节（用于审计与到期清理优先级）。
    pub actual_bytes: u64,
}

/// 隔离条目默认保留期（天）：红线③「删除一律进隔离区（14 天可恢复）」
/// 的到期清理口径。
pub const DEFAULT_RETENTION_DAYS: u64 = 14;

/// 解析 `quarantined_at` 为 unix 秒（当前格式 `"{secs} (unix-seconds)"`，
/// 兼容纯数字串）。不可解析返回 None——年龄未知按保守处理，永不删除。
fn parse_unix_secs(s: &str) -> Option<u64> {
    let digits = s.split_whitespace().next()?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl Quarantine {
    pub fn new(base: &Path) -> Self {
        Self {
            base: base.join("quarantine"),
        }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// 全部隔离条目的父目录：`<base>/quarantine/`（base 已含 quarantine 段）。
    fn entry_root(&self) -> PathBuf {
        self.base.clone()
    }

    pub fn entry_dir(&self, id: &str) -> PathBuf {
        self.entry_root().join(id)
    }

    pub fn manifest_path(&self, id: &str) -> PathBuf {
        self.entry_dir(id).join("manifest.json")
    }

    pub fn read_manifest(&self, id: &str) -> std::io::Result<Manifest> {
        let text = std::fs::read_to_string(self.manifest_path(id))?;
        serde_json::from_str(&text).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("bad manifest: {e}"),
            )
        })
    }

    /// 生成 manifest 并返回条目 id 与 manifest（executor 在 rename 之后落盘，
    /// 避免"先写 manifest 后 rename 失败"留下孤儿 manifest）。
    pub(crate) fn build_manifest(
        &self,
        original_path: &Path,
        rule_id: &str,
        actual_bytes: u64,
    ) -> (String, Manifest) {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let manifest = Manifest {
            id: id.clone(),
            original_path: original_path.to_path_buf(),
            rule_id: rule_id.to_string(),
            quarantined_at: now_rfc3339(),
            actual_bytes,
        };
        (id, manifest)
    }

    /// 列出全部隔离条目 id（按迁入时间升序）。manifest 缺失或损坏的条目
    /// 跳过不报错：隔离区读取永不阻断主流程。
    pub fn list(&self) -> std::io::Result<Vec<Manifest>> {
        let mut out = Vec::new();
        if !self.entry_root().exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(self.entry_root())? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().to_string();
            if let Ok(m) = self.read_manifest(&id) {
                out.push(m);
            }
        }
        out.sort_by(|a, b| a.quarantined_at.cmp(&b.quarantined_at));
        Ok(out)
    }
    /// 清理迁入超过 `max_age_days` 的隔离条目（payload + manifest 一并
    /// 移除），返回被清理的 manifest。这是不可逆删除——调用方负责二次
    /// 确认与审计。语义：
    /// - `quarantined_at` 不可解析或 manifest 不可读的条目跳过不删
    ///   （年龄未知按保守处理）。
    /// - 年龄恰好达到 `max_age_days` 即视为过期（`>=`）。
    /// - 单项删除失败立即返回 Err（已清理项不回滚，失败项留在隔离区，
    ///   下次调用重试）。
    pub fn purge_expired(&self, max_age_days: u64) -> std::io::Result<Vec<Manifest>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let max_age_secs = max_age_days.saturating_mul(86_400);
        let mut purged = Vec::new();
        for m in self.list()? {
            let Some(t) = parse_unix_secs(&m.quarantined_at) else {
                continue;
            };
            if now.saturating_sub(t) >= max_age_secs {
                std::fs::remove_dir_all(self.entry_dir(&m.id))?;
                purged.push(m);
            }
        }
        Ok(purged)
    }

    /// 立即彻底删除单个隔离条目（payload + manifest，不可逆）。
    ///
    /// UI 必须二次确认后调用；审计由调用方记录（与 purge-expired 同模式）。
    /// id 先过 UUID 校验（防路径穿越，与 restore 同防线）。
    pub fn purge_entry(&self, id: &str) -> std::io::Result<Manifest> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid quarantine id: {id}"),
            ));
        }
        let manifest = self.read_manifest(id)?;
        std::fs::remove_dir_all(self.entry_dir(id))?;
        Ok(manifest)
    }
}

fn now_rfc3339() -> String {
    // MVP：无 chrono 依赖，用系统时间秒拼 UTC。格式精度足够审计用途。
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs} (unix-seconds)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let q = Quarantine::new(dir.path());
        let (id, m) = q.build_manifest(Path::new("/tmp/x"), "macos-test", 123);
        assert!(!id.is_empty());
        assert_eq!(m.original_path, Path::new("/tmp/x"));
    }

    #[test]
    fn parse_unix_secs_formats() {
        assert_eq!(super::parse_unix_secs("123 (unix-seconds)"), Some(123));
        assert_eq!(super::parse_unix_secs("456"), Some(456));
        assert_eq!(super::parse_unix_secs("abc"), None);
        assert_eq!(super::parse_unix_secs(""), None);
    }

    #[test]
    fn purge_expired_removes_old_keeps_new() {
        let dir = tempfile::tempdir().unwrap();
        let q = Quarantine::new(dir.path());

        // 旧条目：quarantined_at = epoch（必然超过 14 天）。
        let (old_id, mut old_m) = q.build_manifest(Path::new("/tmp/old"), "macos-test", 100);
        old_m.quarantined_at = "0 (unix-seconds)".into();
        std::fs::create_dir_all(q.entry_dir(&old_id)).unwrap();
        std::fs::write(
            q.manifest_path(&old_id),
            serde_json::to_string(&old_m).unwrap(),
        )
        .unwrap();
        std::fs::write(q.entry_dir(&old_id).join("payload"), b"old").unwrap();

        // 新条目：当前时间（未到期）。
        let (new_id, new_m) = q.build_manifest(Path::new("/tmp/new"), "macos-test", 5);
        std::fs::create_dir_all(q.entry_dir(&new_id)).unwrap();
        std::fs::write(
            q.manifest_path(&new_id),
            serde_json::to_string(&new_m).unwrap(),
        )
        .unwrap();
        std::fs::write(q.entry_dir(&new_id).join("payload"), b"new").unwrap();

        let purged = q.purge_expired(14).unwrap();
        assert_eq!(purged.len(), 1);
        assert_eq!(purged[0].original_path, Path::new("/tmp/old"));
        assert!(
            !q.entry_dir(&old_id).exists(),
            "expired entry must be removed"
        );
        assert!(
            q.entry_dir(&new_id).join("payload").exists(),
            "fresh entry must survive"
        );
        assert_eq!(q.list().unwrap().len(), 1);
    }

    #[test]
    fn purge_expired_skips_unknown_age() {
        let dir = tempfile::tempdir().unwrap();
        let q = Quarantine::new(dir.path());
        let (id, mut m) = q.build_manifest(Path::new("/tmp/x"), "macos-test", 1);
        m.quarantined_at = "not-a-date".into();
        std::fs::create_dir_all(q.entry_dir(&id)).unwrap();
        std::fs::write(q.manifest_path(&id), serde_json::to_string(&m).unwrap()).unwrap();

        let purged = q.purge_expired(14).unwrap();
        assert!(
            purged.is_empty(),
            "unparseable timestamp must never be purged"
        );
        assert!(q.entry_dir(&id).exists());
    }

    #[test]
    fn list_matches_layout_single_quarantine_level() {
        // 布局契约（结构体注释）：`<base>/quarantine/<id>/`，仅一层
        // quarantine。list 必须与 entry_dir 指向同一棵树。
        let dir = tempfile::tempdir().unwrap();
        let q = Quarantine::new(dir.path());
        let (id, mut m) = q.build_manifest(Path::new("/tmp/a"), "macos-test", 1);
        std::fs::create_dir_all(q.entry_dir(&id)).unwrap();
        // executor 的写入路径：manifest.id 与目录名一致。
        m.id = id.clone();
        std::fs::write(q.manifest_path(&id), serde_json::to_string(&m).unwrap()).unwrap();

        let items = q.list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].original_path, Path::new("/tmp/a"));
        assert!(q.entry_dir(&items[0].id).join("manifest.json").exists());
        // 空隔离区/目录不存在也要返回 Ok(空)。
        let empty = Quarantine::new(&dir.path().join("elsewhere"));
        assert!(empty.list().unwrap().is_empty());
    }
}
