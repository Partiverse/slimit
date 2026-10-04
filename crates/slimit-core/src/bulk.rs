//! macOS 快路径扫描：`getattrlistbulk` 批量目录枚举。
//!
//! 动机（docs/BENCH.md W2）：逐文件 lstat 在冷缓存下是主要瓶颈（同机
//! `du -x` 对照 913s / ~416k 条目，属物理上限级）；`getattrlistbulk`
//! 一次 syscall 返回整个目录子项的属性集，syscall 数降低 1–2 个数量级。
//!
//! 语义与 `ignore` 回退路径一致：
//! - 不跟随符号链接（root 为 symlink 时不深入，仅记一个条目）；
//! - 不跨文件系统边界（子项 `devid != root_dev` 一律跳过）；
//! - 包含隐藏文件（`ignore` 默认标准过滤会跳过 `.DS_Store` 等，对清理
//!   场景是正确性 bug，快路径天然不含过滤）。
//!
//! 记录布局（sys/attr.h）：固定属性按位序打包、无对齐填充，变长 NAME
//! 在记录尾部，其 u32 偏移相对记录起始。

use crate::types::{FileEntry, ScanError, ScanResult};
use std::collections::VecDeque;
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Instant;

// sys/attr.h 位图常量（getattrlistbulk 要求 RETURNED_ATTRS 恒为首个返回属性）。
const ATTR_CMN_NAME: u32 = 0x0000_0001;
const ATTR_CMN_DEVID: u32 = 0x0000_0002;
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
const ATTR_CMN_FILEID: u32 = 0x0200_0000;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
const ATTR_FILE_DATALENGTH: u32 = 0x0000_0200;

/// 实测记录布局（M-series macOS 26，`examples/bulk_probe.rs` 逐属性定位）：
///
/// ```text
/// +00 u32  记录总长（8 字节对齐）
/// +04 u32  返回的 common 位图（含 RETURNED_ATTRS）
/// +08/+12  保留（vol/dir 位图位）
/// +16 u32  返回的 file 位图（目录为 0）
/// +20      保留（fork 位图位）
/// +24 u32  名字偏移（相对记录头 24 字节处）
/// +28 u32  名字字节数（含 NUL）
/// +32..    固定属性值：common 按位序、后接 file 按位序（无对齐填充）
/// 尾部     NUL 结尾的 UTF-8 名字
/// ```
///
/// 值区起始 = +32（头 24 字节 + 返回位图自身 8 字节）。
// vnode 类型值（sys/vnode.h vtype_t）。
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

const BUF_SIZE: usize = 1 << 20;

/// 崩溃防线：记录布局假设若不成立（属性缺失/截断），整目录走逐条回退，
/// 保证结果正确只是慢。单测会对照 std::fs::metadata 验证布局解析。
const REQUIRED_COMMON: u32 =
    ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_DEVID | ATTR_CMN_OBJTYPE | ATTR_CMN_FILEID;
const REQUIRED_FILE: u32 = ATTR_FILE_ALLOCSIZE | ATTR_FILE_DATALENGTH;

#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

impl AttrList {
    fn new() -> Self {
        Self {
            bitmapcount: 5, // ATTR_BIT_MAP_COUNT：attrgroup 字段个数
            reserved: 0,
            commonattr: REQUIRED_COMMON,
            volattr: 0,
            dirattr: 0,
            fileattr: REQUIRED_FILE,
            forkattr: 0,
        }
    }
}

struct Counters {
    dirs: AtomicU64,
    bulk_calls: AtomicU64,
    fallback_dirs: AtomicU64,
    /// 已发现的文件/符号链接条目数（进度事件用）。
    files_done: AtomicU64,
}

impl Counters {
    fn new() -> Self {
        Self {
            dirs: AtomicU64::new(0),
            bulk_calls: AtomicU64::new(0),
            fallback_dirs: AtomicU64::new(0),
            files_done: AtomicU64::new(0),
        }
    }
}

struct Shared<'a> {
    queue: Mutex<QueueState>,
    cv: Condvar,
    files: Mutex<Vec<FileEntry>>,
    errors: Mutex<Vec<String>>,
    counters: Counters,
    progress: Option<&'a (dyn Fn(crate::types::ScanProgress) + Send + Sync)>,
}

struct QueueState {
    pending: VecDeque<PathBuf>,
    /// 已发现未处理完的目录数；归零即扫描结束。
    in_flight: usize,
}

struct Local {
    files: Vec<FileEntry>,
    errors: Vec<String>,
    children: Vec<PathBuf>,
}

pub(crate) fn scan(
    root: &Path,
    progress: Option<&(dyn Fn(crate::types::ScanProgress) + Send + Sync)>,
) -> Result<ScanResult, ScanError> {
    let started = Instant::now();
    let root_md =
        std::fs::symlink_metadata(root).map_err(|_| ScanError::RootMissing(root.to_path_buf()))?;
    let root_dev = root_md.dev();

    let mut files = Vec::new();
    let mut errors = Vec::new();

    if !root_md.is_dir() {
        // 与旧 walkdir 语义一致：root 是文件或 symlink 时只产出一个条目。
        if root_md.is_file() || root_md.is_symlink() {
            files.push(single_entry(root, &root_md));
        }
        return crate::scan::finish(root, files, errors, started.elapsed());
    }

    let shared = Shared {
        queue: Mutex::new(QueueState {
            pending: VecDeque::from([root.to_path_buf()]),
            in_flight: 1,
        }),
        cv: Condvar::new(),
        files: Mutex::new(Vec::new()),
        errors: Mutex::new(Vec::new()),
        counters: Counters::new(),
        progress,
    };

    let workers = std::env::var("SLIMIT_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &usize| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| worker(root_dev, &shared));
        }
    });

    files = shared.files.into_inner().unwrap();
    errors = shared.errors.into_inner().unwrap();

    // 完整性二次校验（2026-10-03 P0 缓解）：getattrlistbulk 在 APFS 大目录上
    // 存在间歇性丢失目录子树的系统级缺陷（真机实测：walker 处理 475,346 目录、
    // 快照仅 441,370；丢失子树无任何错误返回）。用 read_dir 全量走查补录缺失
    // 文件——read_dir 单遍无属性读取，代价约为 bulk 扫描的 15–20%。
    // SLIMIT_TREE_VERIFY=0 关闭（默认开启，正确性优先）。
    if std::env::var_os("SLIMIT_TREE_VERIFY")
        .map(|v| v == "0")
        .unwrap_or(false)
    {
        // 显式关闭：跳过校验
    } else {
        verify_and_fill(root, root_dev, &mut files, &mut errors);
    }

    if std::env::var_os("SLIMIT_DEBUG").is_some() {
        let c = &shared.counters;
        eprintln!(
            "bulk walk elapsed: {:?}  dirs: {}  bulk_calls: {}  fallback_dirs: {}  threads: {workers}",
            started.elapsed(),
            c.dirs.load(Ordering::Relaxed),
            c.bulk_calls.load(Ordering::Relaxed),
            c.fallback_dirs.load(Ordering::Relaxed),
        );
        for e in errors.iter().take(3) {
            eprintln!("debug: {e}");
        }
    }

    crate::scan::finish(root, files, errors, started.elapsed())
}

fn worker(root_dev: u64, shared: &Shared) {
    let mut local = Local {
        files: Vec::new(),
        errors: Vec::new(),
        children: Vec::new(),
    };
    // 每 worker 复用一个 bulk 缓冲，避免逐目录分配+清零 1 MiB。
    let mut buf = vec![0u8; BUF_SIZE];

    loop {
        let dir = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if let Some(d) = q.pending.pop_back() {
                    break d;
                }
                if q.in_flight == 0 {
                    // 退出前移交剩余缓冲，否则尾批结果全部丢失。
                    shared.files.lock().unwrap().append(&mut local.files);
                    shared.errors.lock().unwrap().append(&mut local.errors);
                    shared.cv.notify_all();
                    return;
                }
                q = shared.cv.wait(q).unwrap();
            }
        };

        let before = local.files.len();
        process_dir(&dir, root_dev, &shared.counters, &mut local, &mut buf);
        let done = shared
            .counters
            .files_done
            .fetch_add((local.files.len() - before) as u64, Ordering::Relaxed)
            + (local.files.len() - before) as u64;
        if let Some(p) = shared.progress {
            p(crate::types::ScanProgress {
                files_done: done,
                current_dir: dir,
            });
        }

        let children = std::mem::take(&mut local.children);
        let mut q = shared.queue.lock().unwrap();
        // 锁内一次性更新：先减本目录，再加子目录，in_flight 不会瞬间归零误判结束。
        q.in_flight = q.in_flight.saturating_sub(1) + children.len();
        q.pending.extend(children);
        if q.in_flight == 0 {
            shared.cv.notify_all();
        }
        // 局部缓冲按目录批次移交全局，锁持有时间极短。
        if local.files.len() >= 4096 {
            shared.files.lock().unwrap().append(&mut local.files);
        }
        if local.errors.len() >= 256 {
            shared.errors.lock().unwrap().append(&mut local.errors);
        }
        drop(q);
        shared.cv.notify_all();
    }
}

fn process_dir(
    dir: &Path,
    root_dev: u64,
    counters: &Counters,
    local: &mut Local,
    buf: &mut Vec<u8>,
) {
    counters.dirs.fetch_add(1, Ordering::Relaxed);
    let Ok(cpath) = CString::new(dir.as_os_str().as_bytes()) else {
        local
            .errors
            .push(format!("invalid path: {}", dir.display()));
        return;
    };
    // SAFETY: cpath 是合法 NUL 结尾字符串；fd 在所有路径上恰好关闭一次。
    let fd = unsafe {
        libc::open(
            cpath.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY,
        )
    };
    if fd < 0 {
        local.errors.push(format!(
            "open {}: {errno_str}",
            dir.display(),
            errno_str = errno_str()
        ));
        return;
    }

    let result = bulk_scan_dir(fd, dir, root_dev, local, counters, buf);
    if let Err(err) = result {
        counters.fallback_dirs.fetch_add(1, Ordering::Relaxed);
        // 记录首个失败原因，便于诊断布局/权限问题（有界防爆量）。
        if local.errors.len() < 64 {
            local.errors.push(format!(
                "bulk {}: errno {err} ({errno_str})",
                dir.display(),
                errno_str = errno_str()
            ));
        }
        fallback_scan_dir(dir, local);
    }

    // SAFETY: fd 由上方 open 成功返回。
    unsafe { libc::close(fd) };
}

fn bulk_scan_dir(
    fd: i32,
    dir: &Path,
    root_dev: u64,
    local: &mut Local,
    counters: &Counters,
    buf: &mut Vec<u8>,
) -> Result<(), i32> {
    let mut attrlist = AttrList::new();
    let dir_str = dir.as_os_str();

    loop {
        // SAFETY: attrlist/buf 均为合法可写内存，大小如实传递。
        let n = unsafe {
            libc::getattrlistbulk(
                fd,
                &mut attrlist as *mut AttrList as *mut std::ffi::c_void,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                buf.len(),
                0,
            )
        };
        if n < 0 {
            return Err(last_errno());
        }
        counters.bulk_calls.fetch_add(1, Ordering::Relaxed);
        if n == 0 {
            return Ok(());
        }

        let mut off = 0usize;
        for _ in 0..n {
            let rec = &buf[off..];
            let len = ru32(rec, 0) as usize;
            if len < 32 || len > rec.len() {
                return Err(libc::EINVAL);
            }
            let returned = ru32(rec, 4);
            if returned & REQUIRED_COMMON != REQUIRED_COMMON {
                return Err(libc::EINVAL);
            }
            let returned_file = ru32(rec, 16);
            let name_off = 24 + ru32(rec, 24) as usize;
            let name_len = ru32(rec, 28) as usize;
            let devid = ru32(rec, 32) as u64;
            let objtype = ru32(rec, 36);
            let ino = ru64(rec, 40);
            // apparent 取 DATALENGTH（st_size 语义），actual 取 ALLOCSIZE
            // （st_blocks * 512 语义）。
            let (apparent, actual) = if objtype == VREG {
                if returned_file & REQUIRED_FILE != REQUIRED_FILE || len < 64 {
                    return Err(libc::EINVAL);
                }
                (ru64(rec, 56), ru64(rec, 48))
            } else if objtype == VLNK && returned_file & ATTR_FILE_DATALENGTH != 0 && len >= 64 {
                // symlink 不跟随，但 lstat 语义保留其自身长度。
                (ru64(rec, 56), 0)
            } else {
                (0, 0)
            };
            if name_off < 32 || name_off >= len || name_len == 0 || name_off + name_len > len {
                return Err(libc::EINVAL);
            }
            // 名字以 NUL 结尾，UTF-8。
            let name = OsStr::from_bytes(&rec[name_off..name_off + name_len - 1]);
            let path = join(dir_str, name);

            match objtype {
                VDIR => {
                    if devid == root_dev {
                        local.children.push(path);
                    }
                }
                VLNK => {
                    // symlink 不深入；自身计为条目（大小 0）。
                    local.files.push(FileEntry {
                        path,
                        dev: devid,
                        ino,
                        apparent: 0,
                        actual: 0,
                        shared: false,
                    });
                }
                _ => {
                    if devid == root_dev {
                        local.files.push(FileEntry {
                            path,
                            dev: devid,
                            ino,
                            apparent,
                            actual,
                            shared: false,
                        });
                    }
                }
            }
            off += len;
        }
    }
}

/// `getattrlistbulk` 失败（网络卷、特殊文件系统等）时的逐条回退，
/// 语义与旧 `ignore` 路径一致（含隐藏文件、metadata 为 lstat 不跟随）。
fn fallback_scan_dir(dir: &Path, local: &mut Local) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            local.errors.push(format!("readdir {}: {e}", dir.display()));
            return;
        }
    };
    for entry in rd {
        let Ok(entry) = entry else { continue };
        let Ok(md) = entry.metadata() else {
            local.errors.push(format!(
                "stat {}: {errno_str}",
                entry.path().display(),
                errno_str = errno_str()
            ));
            continue;
        };
        if md.is_dir() {
            local.children.push(entry.path());
        } else {
            local.files.push(FileEntry {
                path: entry.path(),
                dev: md.dev(),
                ino: md.ino(),
                apparent: md.len(),
                actual: md.blocks() * 512,
                shared: false,
            });
        }
    }
}

/// 完整性二次校验：read_dir 全量走查，补录 bulk 枚举丢失的文件。
/// 以「已记录路径的哈希集合」为基准，磁盘上存在但未记录的文件按 lstat
/// 语义补录（FileEntry）；空目录不补（无空间意义，与既有语义一致：
/// dirs 由文件 parent 链推导）。
fn verify_and_fill(
    root: &Path,
    root_dev: u64,
    files: &mut Vec<FileEntry>,
    errors: &mut Vec<String>,
) {
    let started = Instant::now();
    use std::collections::HashSet;
    use std::hash::BuildHasher;
    // 单个 builder 克隆（同一组 key）：recorded 与 walk 两处哈希必须一致，
    // 否则同路径不同哈希 → 去重失效 → 全量重复补录。
    let builder = std::collections::hash_map::RandomState::new();
    let hash = |p: &Path| -> u64 { builder.hash_one(p) };
    let mut recorded: HashSet<u64> = HashSet::with_capacity(files.len() * 2);
    for f in files.iter() {
        recorded.insert(hash(&f.path));
    }

    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut added = 0usize;
    let mut verify_errors = 0usize;
    while let Some(dir) = stack.pop() {
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                // 无 FDA 下 ~/Library 深处的 EPERM 属预期，只计前几条。
                if verify_errors < 8 {
                    errors.push(format!("verify readdir {}: {e}", dir.display()));
                }
                verify_errors += 1;
                continue;
            }
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(md) = entry.metadata() else {
                continue;
            };
            // 不跨文件系统边界（与 bulk 主路径语义一致）。
            if md.dev() != root_dev {
                continue;
            }
            if md.is_dir() {
                stack.push(path);
                continue;
            }
            // 文件（含 symlink）：未记录才补录。
            if recorded.insert(hash(&path)) {
                files.push(FileEntry {
                    path,
                    dev: md.dev(),
                    ino: md.ino(),
                    apparent: md.len(),
                    actual: md.blocks() * 512,
                    shared: false,
                });
                added += 1;
            }
        }
    }
    let _ = added;
    if std::env::var_os("SLIMIT_DEBUG").is_some() {
        eprintln!(
            "verify_and_fill elapsed: {:?}  added: {added}",
            started.elapsed()
        );
    }
}

fn single_entry(path: &Path, md: &std::fs::Metadata) -> FileEntry {
    FileEntry {
        path: path.to_path_buf(),
        dev: md.dev(),
        ino: md.ino(),
        apparent: md.len(),
        actual: md.blocks() * 512,
        shared: false,
    }
}

fn ru32(b: &[u8], o: usize) -> u32 {
    u32::from_ne_bytes(b[o..o + 4].try_into().unwrap())
}

fn ru64(b: &[u8], o: usize) -> u64 {
    u64::from_ne_bytes(b[o..o + 8].try_into().unwrap())
}

fn join(dir: &OsStr, name: &OsStr) -> PathBuf {
    let mut s = OsString::with_capacity(dir.len() + name.len() + 1);
    s.push(dir);
    s.push("/");
    s.push(name);
    PathBuf::from(s)
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn errno_str() -> String {
    std::io::Error::last_os_error().to_string()
}
