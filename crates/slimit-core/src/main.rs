use slimit_core::scan;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let json = args.iter().any(|a| a == "--json");
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
            println!("\n{:<18} {:>12} {:>12}  path", "actual", "apparent", "files");
            for d in dirs.iter().take(top) {
                println!(
                    "{:>16} B {:>10} B {:>8}  {}",
                    d.actual, d.apparent, d.file_count, d.path.display()
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
