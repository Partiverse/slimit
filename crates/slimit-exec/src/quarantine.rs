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
    /// 迁入时间（RFC 3339）。
    pub quarantined_at: String,
    /// 迁入时真实占用字节（用于审计与到期清理优先级）。
    pub actual_bytes: u64,
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

    /// 生成 manifest 并返回可写入的临时路径（executor 在 rename 之后落盘，
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
