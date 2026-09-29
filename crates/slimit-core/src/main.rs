use slimit_core::{list_snapshots, scan, volume_summary};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("snapshots") => cmd_snapshots(&args[2..]),
        Some("volume") => cmd_volume(&args[2..]),
        Some("-h") | Some("--help") | Some("help") => help(),
        _ => cmd_scan(&args[1..]),
    }
}

fn help() {
    eprintln!("usage:");
    eprintln!("  slimit <root> [--json] [--top N]   扫描目录（真实占用/表观大小）");
    eprintln!("  slimit snapshots <volume> [--json] 列出 APFS 快照");
    eprintln!("  slimit volume <mount> [--json]     卷容量摘要");
}

fn json_flag(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

fn cmd_snapshots(args: &[String]) {
    let Some(volume) = args.iter().find(|a| !a.starts_with('-')) else {
        help();
        std::process::exit(2);
    };
    match list_snapshots(volume) {
        Ok(snaps) => {
            if json_flag(args) {
                println!("{}", serde_json::to_string_pretty(&snaps).unwrap());
                return;
            }
            println!("{}: {} snapshots", volume, snaps.len());
            for s in &snaps {
                println!(
                    "  {}  purgeable={} limiting-shrink={} xid={}",
                    s.name, s.purgeable, s.limiting_container_shrink, s.xid
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_volume(args: &[String]) {
    let Some(mount) = args.iter().find(|a| !a.starts_with('-')) else {
        help();
        std::process::exit(2);
    };
    match volume_summary(mount) {
        Ok(v) => {
            if json_flag(args) {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
                return;
            }
            println!(
                "volume: {} ({})",
                v.volume_name.as_deref().unwrap_or("?"),
                v.device_identifier.as_deref().unwrap_or("?")
            );
            if let Some(free) = v.free_space {
                println!("  free:            {:.2} GiB", free as f64 / 1073741824.0);
            }
            if let (Some(size), Some(free)) = (v.apfs_container_size, v.apfs_container_free) {
                println!(
                    "  APFS container:  {:.2} / {:.2} GiB used",
                    (size - free) as f64 / 1073741824.0,
                    size as f64 / 1073741824.0
                );
            }
            if let Some(name) = &v.system_snapshot_name {
                println!("  system snapshot: {name}");
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_scan(args: &[String]) {
    let root = args
        .first()
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let json = json_flag(args);
    let top: usize = args
        .iter()
        .position(|a| a == "--top")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    match scan(&root) {
        Ok(res) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap());
                return;
            }
            println!(
                "root: {}  files: {}  actual: {:.2} MiB  apparent: {:.2} MiB",
                res.root.display(),
                res.file_count,
                total(&res, true) as f64 / 1048576.0,
                total(&res, false) as f64 / 1048576.0
            );
            let mut dirs = res.dirs.clone();
            dirs.sort_unstable_by(|a, b| b.actual.cmp(&a.actual));
            println!(
                "\n{:<18} {:>12} {:>12}  path",
                "actual", "apparent", "files"
            );
            for d in dirs.iter().take(top) {
                println!(
                    "{:>16} B {:>10} B {:>8}  {}",
                    d.actual,
                    d.apparent,
                    d.file_count,
                    d.path.display()
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn total(res: &slimit_core::ScanResult, actual: bool) -> u64 {
    res.dirs
        .iter()
        .find(|d| d.path == res.root)
        .map(|d| if actual { d.actual } else { d.apparent })
        .unwrap_or(0)
}
