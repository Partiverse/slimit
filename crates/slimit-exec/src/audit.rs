use std::io::Write;
use std::path::{Path, PathBuf};

/// JSONL 追加写审计日志。每次 apply/restore 追加一行，绝不改写历史。
pub struct AuditLog {
    path: PathBuf,
    file: std::fs::File,
}

impl AuditLog {
    pub fn new(base: &Path) -> std::io::Result<Self> {
        let dir = base.join("audit");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("audit.jsonl");
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { path, file })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record(&mut self, event: &serde_json::Value) {
        let mut line = event.clone();
        if let serde_json::Value::Object(map) = &mut line {
            map.insert(
                "ts_unix_secs".into(),
                serde_json::json!(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)),
            );
        }
        let _ = writeln!(self.file, "{line}");
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = AuditLog::new(dir.path()).unwrap();
        log.record(&serde_json::json!({ "event": "quarantine", "path": "/x" }));
        log.record(&serde_json::json!({ "event": "restore", "path": "/x" }));
        let text = std::fs::read_to_string(log.path()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        for l in lines {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            assert!(v.get("ts_unix_secs").is_some());
        }
    }
}
