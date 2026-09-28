use crate::types::{DirStat, FileEntry, ScanError, ScanResult};
use ignore::{WalkBuilder, WalkState};
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// 扫描 `root`，返回真实占用统计。
///
/// 语义约定：
/// - 不跟随符号链接（symlink 本身计为一个条目，target 不入树）。
/// - 不跨文件系统边界（挂载点记为独立卷，另行扫描）。
/// - 硬链接按 `(dev, ino)` 去重：表观与真实大小都只在首次出现处计入聚合，
///   后续路径标 `shared = true`。
/// - 稀疏文件：`actual = blocks * 512`，`apparent = size`，两者分离呈现。
pub fn scan(root: &Path) -> Result<ScanResult, ScanError> {
    if !root.exists() {
        return Err(ScanError::RootMissing(root.to_path_buf()));
    }

    let files: Mutex<Vec<FileEntry>> = Mutex::new(Vec::new());
    let walk_errors: Mutex<Vec<String>> = Mutex::new(Vec::new());

    let started = Instant::now();
    WalkBuilder::new(root)
        .follow_links(false)
        .same_file_system(true)
        .build_parallel()
        .run(|| {
            let files = &files;
            let walk_errors = &walk_errors;
            Box::new(move |entry| match entry {
                Ok(e) => {
                    // 目录本身不产生 FileEntry，交给聚合阶段。
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        return WalkState::Continue;
                    }
                    if let Ok(md) = e.metadata() {
                        // follow_links(false) 时符号链接的 metadata 是 lstat。
                        files.lock().unwrap().push(FileEntry {
                            path: e.path().to_path_buf(),
                            dev: md.dev(),
                            ino: md.ino(),
                            nlink: md.nlink(),
                            apparent: md.len(),
                            actual: md.blocks() * 512,
                            shared: false,
                        });
                    }
                    WalkState::Continue
                }
                Err(err) => {
                    walk_errors.lock().unwrap().push(err.to_string());
                    WalkState::Continue
                }
            })
        });
    let walk_elapsed = started.elapsed();

    let mut files = files.into_inner().unwrap();
    let walk_errors = walk_errors.into_inner().unwrap();
    if !walk_errors.is_empty() && files.is_empty() {
        return Err(ScanError::Walk(walk_errors.join("; ")));
    }

    // 硬链接去重：并行遍历下同一 inode 可能以任意顺序出现，需全局标记。
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    for f in &mut files {
        f.shared = !seen.insert((f.dev, f.ino));
    }

    let dirs = aggregate(root, &files);

    if std::env::var_os("SLIMIT_DEBUG").is_some() {
        eprintln!("walk elapsed: {walk_elapsed:?}");
    }

    Ok(ScanResult {
        root: root.to_path_buf(),
        file_count: files.len() as u64,
        files,
        dirs,
    })
}

/// 自底向上聚合目录大小（硬链接已在 `files` 中去重，此处仅求和）。
fn aggregate(root: &Path, files: &[FileEntry]) -> Vec<DirStat> {
    // dir -> (apparent, actual, file_count)，仅计入该目录自身（不含子目录）。
    let mut own: HashMap<PathBuf, (u64, u64, u64)> = HashMap::new();
    for f in files {
        if f.shared {
            continue;
        }
        if let Some(parent) = f.path.parent() {
            let e = own.entry(parent.to_path_buf()).or_insert((0, 0, 0));
            e.0 += f.apparent;
            e.1 += f.actual;
            e.2 += 1;
        }
    }

    // 目录路径按深度降序，把自身值累加进父目录。
    let mut dir_paths: Vec<PathBuf> = own.keys().cloned().collect();
    dir_paths.sort_unstable_by_key(|p| std::cmp::Reverse(p.components().count()));

    for path in &dir_paths {
        if *path == root {
            continue;
        }
        let Some(parent) = path.parent() else { continue };
        // 先取出 child 避免同 map 同时可变/不可变借用。
        let child = match own.get(path) {
            Some(c) => (c.0, c.1, c.2),
            None => continue,
        };
        if let Some(p) = own.get_mut(parent) {
            p.0 += child.0;
            p.1 += child.1;
            p.2 += child.2;
        }
    }

    let mut dirs: Vec<DirStat> = own
        .into_iter()
        .map(|(path, (apparent, actual, file_count))| DirStat {
            path,
            apparent,
            actual,
            file_count,
        })
        .collect();
    dirs.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn sizes_and_hardlink_dedup() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.bin");
        let b = tmp.path().join("b.bin");
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(&a, vec![0u8; 4096]).unwrap();
        fs::hard_link(&a, &b).unwrap();
        fs::write(sub.join("c.txt"), vec![0u8; 1024]).unwrap();

        let res = scan(tmp.path()).unwrap();

        assert_eq!(res.file_count, 3);
        let root_stat = res.dirs.iter().find(|d| d.path == tmp.path()).unwrap();
        // 4096 只计一次（硬链接去重）+ 1024。
        assert_eq!(root_stat.apparent, 4096 + 1024);
        assert!(root_stat.actual >= 4096 + 1024);

        let shared_b = res.files.iter().find(|f| f.path == b).unwrap();
        assert!(shared_b.shared);
        let a_entry = res.files.iter().find(|f| f.path == a).unwrap();
        assert!(!a_entry.shared);
        assert_eq!(a_entry.apparent, 4096);
    }

    #[test]
    fn symlinks_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("big.bin"), vec![0u8; 2048]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, tmp.path().join("link")).unwrap();

        let res = scan(tmp.path()).unwrap();
        // 条目 = real/big.bin + link 自身（lstat）；link 指向的目录内容不入树。
        assert_eq!(res.file_count, 2);
        let mut paths: Vec<_> = res.files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![tmp.path().join("link"), real.join("big.bin")],
            "symlink target contents must not be walked"
        );
        let link = res.files.iter().find(|f| f.path == tmp.path().join("link")).unwrap();
        assert!(link.apparent < 2048, "symlink entry should be tiny, not target size");
    }

    #[test]
    fn sparse_reported_dual() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("sparse.bin");
        let file = fs::File::create(&f).unwrap();
        file.set_len(1 << 20).unwrap(); // 1 MiB 表观，真实块接近 0

        let res = scan(tmp.path()).unwrap();
        let e = res.files.iter().find(|x| x.path == f).unwrap();
        assert_eq!(e.apparent, 1 << 20);
        let md = fs::metadata(&f).unwrap();
        assert_eq!(e.actual, md.blocks() * 512);
        assert!(e.actual < e.apparent, "sparse file should have actual << apparent");
    }

    #[test]
    fn missing_root_errors() {
        assert!(matches!(
            scan(Path::new("/definitely/not/here/slimit")),
            Err(ScanError::RootMissing(_))
        ));
    }
}
