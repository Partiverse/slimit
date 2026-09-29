use crate::types::{DirStat, FileEntry, ScanError, ScanResult};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "macos"))]
use std::time::Instant;

/// 扫描 `root`，返回真实占用统计。
///
/// 语义约定：
/// - 不跟随符号链接（symlink 本身计为一个条目，target 不入树）。
/// - 不跨文件系统边界（挂载点记为独立卷，另行扫描）。
/// - 硬链接按 `(dev, ino)` 去重：表观与真实大小都只在首次出现处计入聚合，
///   后续路径标 `shared = true`。
/// - 稀疏文件：`actual = blocks * 512`，`apparent = size`，两者分离呈现。
/// - 包含隐藏文件与 dotfile（清理场景不做 ignore 过滤）。
pub fn scan(root: &Path) -> Result<ScanResult, ScanError> {
    scan_with_progress(root, &|_| {})
}

/// 同 [`scan`]，遍历过程中回调 `progress(已发现条目数)`（相对根的累计值，
/// 总量未知——这正是进度而非百分比）。回调在工作线程调用，须自行保证
/// 线程安全且轻量（GUI 场景只做事件投递）。
pub fn scan_with_progress(
    root: &Path,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<ScanResult, ScanError> {
    if !root.exists() {
        return Err(ScanError::RootMissing(root.to_path_buf()));
    }

    #[cfg(target_os = "macos")]
    return crate::bulk::scan(root, Some(progress));

    #[cfg(not(target_os = "macos"))]
    return walk_ignore(root, Some(progress));
}

/// 非 macOS 回退：`ignore` 并行遍历 + 逐条 lstat。
/// 关闭全部标准过滤（hidden/gitignore 等），保证与 macOS 快路径同语义。
#[cfg(not(target_os = "macos"))]
fn walk_ignore(
    root: &Path,
    progress: Option<&(dyn Fn(u64) + Send + Sync)>,
) -> Result<ScanResult, ScanError> {
    use ignore::{WalkBuilder, WalkState};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    let files: Mutex<Vec<FileEntry>> = Mutex::new(Vec::new());
    let walk_errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let seen = AtomicU64::new(0);

    let started = Instant::now();
    WalkBuilder::new(root)
        .follow_links(false)
        .same_file_system(true)
        .standard_filters(false)
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
                            apparent: md.len(),
                            actual: md.blocks() * 512,
                            shared: false,
                        });
                    }
                    // 4096 条回调一次，控制事件频率。
                    if let Some(p) = progress {
                        let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
                        if n % 4096 == 0 {
                            p(files.lock().unwrap().len() as u64);
                        }
                    }
                    WalkState::Continue
                }
                Err(err) => {
                    walk_errors.lock().unwrap().push(err.to_string());
                    WalkState::Continue
                }
            })
        });

    if std::env::var_os("SLIMIT_DEBUG").is_some() {
        eprintln!("walk elapsed: {:?}", started.elapsed());
    }

    let files = files.into_inner().unwrap();
    let walk_errors = walk_errors.into_inner().unwrap();
    finish(root, files, walk_errors, started.elapsed())
}

/// 公共收尾：硬链接去重 + 目录聚合。
pub(crate) fn finish(
    root: &Path,
    mut files: Vec<FileEntry>,
    walk_errors: Vec<String>,
    elapsed: std::time::Duration,
) -> Result<ScanResult, ScanError> {
    let _ = elapsed;
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

    // 只含子目录（无直属文件）的中间目录与根也要有条目，否则聚合链断裂、
    // 这些目录的统计整体丢失（W2 前 du 对照发现的 bug）。
    // root 自身不向上走：否则会把扫描根之上的真实祖先目录（如扫
    // ~/Library/Caches/X 时暴露 ~/Library/Caches）以 0 统计插入 dirs，
    // 规则匹配层会把它们当作可执行目标（W7 手测抓到的正确性 bug）。
    let existing: Vec<PathBuf> = own.keys().cloned().collect();
    for path in existing {
        if path == root {
            continue;
        }
        let mut anc = path.as_path();
        while let Some(parent) = anc.parent() {
            if !own.contains_key(parent) {
                own.insert(parent.to_path_buf(), (0, 0, 0));
            }
            if parent == root {
                break;
            }
            anc = parent;
        }
    }

    // 兜底：任何游离在扫描根之上的条目（防御性，正常流程不会再产生）。
    own.retain(|path, _| path.starts_with(root));

    // 目录路径按深度降序，把自身值累加进父目录。
    let mut dir_paths: Vec<PathBuf> = own.keys().cloned().collect();
    dir_paths.sort_unstable_by_key(|p| std::cmp::Reverse(p.components().count()));

    for path in &dir_paths {
        if *path == root {
            continue;
        }
        let Some(parent) = path.parent() else {
            continue;
        };
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
        let link = res
            .files
            .iter()
            .find(|f| f.path == tmp.path().join("link"))
            .unwrap();
        assert!(
            link.apparent < 2048,
            "symlink entry should be tiny, not target size"
        );
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
        assert!(
            e.actual < e.apparent,
            "sparse file should have actual << apparent"
        );
    }

    #[test]
    fn missing_root_errors() {
        assert!(matches!(
            scan(Path::new("/definitely/not/here/slimit")),
            Err(ScanError::RootMissing(_))
        ));
    }

    #[test]
    fn aggregate_covers_fileless_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a").join("b");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("f.bin"), vec![0u8; 1000]).unwrap();

        let res = scan(tmp.path()).unwrap();
        let root_stat = res.dirs.iter().find(|d| d.path == tmp.path()).unwrap();
        assert_eq!(root_stat.apparent, 1000);
        assert_eq!(root_stat.file_count, 1);
        // 无直属文件的中间目录也要有聚合条目。
        let a = res
            .dirs
            .iter()
            .find(|d| d.path == tmp.path().join("a"))
            .unwrap();
        assert_eq!(a.apparent, 1000);
        let b = res.dirs.iter().find(|d| d.path == deep).unwrap();
        assert_eq!(b.apparent, 1000);
    }

    #[test]
    fn hidden_files_included() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".DS_Store"), b"x").unwrap();
        fs::write(tmp.path().join("normal.txt"), b"y").unwrap();

        let res = scan(tmp.path()).unwrap();
        assert_eq!(res.file_count, 2, "dotfiles must be scanned, not filtered");
    }

    #[test]
    fn regular_file_sizes_match_std_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("f.bin");
        fs::write(&f, vec![0u8; 5000]).unwrap();

        let res = scan(tmp.path()).unwrap();
        let e = res.files.iter().find(|x| x.path == f).unwrap();
        let md = fs::metadata(&f).unwrap();
        assert_eq!(e.apparent, md.len());
        assert_eq!(e.actual, md.blocks() * 512);
        assert_eq!(e.dev, md.dev());
        assert_eq!(e.ino, md.ino());
    }

    #[test]
    fn progress_callback_reports_monotonic() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        for i in 0..64 {
            fs::write(sub.join(format!("f{i:02}.bin")), vec![0u8; 16]).unwrap();
        }
        fs::write(tmp.path().join("root.bin"), vec![0u8; 16]).unwrap();

        let last = std::sync::Mutex::new(0u64);
        scan_with_progress(tmp.path(), &|n| {
            let mut l = last.lock().unwrap();
            assert!(*l <= n, "progress must be monotonic: {l} -> {n}");
            *l = n;
        })
        .unwrap();
        assert_eq!(
            *last.lock().unwrap(),
            65,
            "final callback must equal file count"
        );
    }

    #[test]
    fn dirs_never_escape_scan_root() {
        // W7 手测回归：root 自身有直属文件时，aggregate 的向上遍历曾把根
        // 之上的真实祖先目录（/tmp、/var、/ …）以 0 统计插入 dirs，导致
        // 规则匹配层产生扫描范围之外的可执行计划项。
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("direct.bin"), vec![0u8; 32]).unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("f.bin"), vec![0u8; 32]).unwrap();

        let res = scan(tmp.path()).unwrap();
        assert!(
            res.dirs.iter().all(|d| d.path.starts_with(tmp.path())),
            "dirs must stay under scan root, got: {:?}",
            res.dirs
        );
    }
}
