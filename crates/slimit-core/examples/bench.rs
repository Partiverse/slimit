//! 性能回归基准（docs/BENCH.md 记录口径）。
//!
//! 用法：`cargo run --release -p slimit-core --example bench -- <dir> [runs]`
//! 或：  `cargo run --release -p slimit-core --example bench -- --synth [runs]`
//!
//! `--synth` 模式在临时目录生成确定性合成树（固定布局与内容，跨日可比），
//! 是长期回归基线的标准用法；`<dir>` 模式测真实目录（树会漂移，只做参考）。
use std::time::Instant;

/// 确定性合成树：200 个顶层目录 × 每目录 25 个子目录 × 每子目录 10 个
/// 文件（200 B）= 50k 文件 / 5k+ 目录，规模小到秒级跑完、又足以暴露
/// walker 层的性能变化。内容固定 → 文件数/字节数跨日可比。
fn synth_tree() -> std::io::Result<std::path::PathBuf> {
    let base = std::env::temp_dir().join("slimit-bench-synth");
    if base.exists() {
        std::fs::remove_dir_all(&base)?;
    }
    std::fs::create_dir_all(&base)?;
    let content = vec![b'x'; 200];
    for i in 0..200u32 {
        let dir = base.join(format!("d{i:03}"));
        std::fs::create_dir_all(&dir)?;
        for j in 0..25u32 {
            let sub = dir.join(format!("s{j:03}"));
            std::fs::create_dir_all(&sub)?;
            for k in 0..10u32 {
                std::fs::write(sub.join(format!("f{k:03}.bin")), &content)?;
            }
        }
    }
    Ok(base)
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let synth = args.first().map(|a| a == "--synth").unwrap_or(false);
    if synth {
        args.remove(0);
    }
    let dir = if synth {
        match synth_tree() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("synth tree creation failed: {e}");
                std::process::exit(1);
            }
        }
        .to_string_lossy()
        .to_string()
    } else {
        match args.pop() {
            Some(d) => d,
            None => {
                eprintln!("usage: bench <dir> [runs] | bench --synth [runs]");
                std::process::exit(2);
            }
        }
    };
    let runs: usize = args
        .last()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if synth { 3 } else { 2 });

    println!(
        "benchmarking {dir} × {runs} runs (release build{})",
        if synth { ", synth tree" } else { "" }
    );
    for i in 1..=runs {
        let started = Instant::now();
        let res = match slimit_core::scan(std::path::Path::new(&dir)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("scan failed: {e}");
                std::process::exit(1);
            }
        };
        let elapsed = started.elapsed();
        let mpps = res.file_count as f64 / elapsed.as_secs_f64() / 1e6;
        println!(
            "run {i}: {:>10} files | {:>8.2}s | {:.2} M files/s | {:.1} MiB actual",
            res.file_count,
            elapsed.as_secs_f64(),
            mpps,
            res.dirs
                .iter()
                .find(|d| d.path == res.root)
                .map(|d| d.actual as f64 / (1024.0 * 1024.0))
                .unwrap_or(0.0),
        );
    }
}
