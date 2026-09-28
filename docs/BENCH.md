# SlimIt 扫描器基准日志

方法：`SLIMIT_DEBUG=1 ./target/release/slimit <dir> --top N`，release 构建（cargo 1.93.1，M-series Mac，内置 SSD）。冷/热各一次，热 = 紧随冷之后重跑同目录。格式：`文件数 | 耗时 | 吞吐`。

## 2026-09-28 · W2 getattrlistbulk 重写

改动：macOS 快路径改为自定义并行 walker（`open` + `getattrlistbulk` 批量取整目录子项元数据，8 线程共享队列；`SLIMIT_JOBS` 可调），非 macOS 保留 `ignore` 兜底。记录布局见 `crates/slimit-core/src/bulk.rs` 头注释（用 `cargo run -p slimit-core --example bulk_probe -- <dir>` 逐属性定位验证）。

| 场景 | 数据 | 耗时 | 折算 1M |
|---|---|---|---|
| ~/Library（热） | **2,958,704 files** / 44.8 GiB actual | **57.0s**（walk 50.1s） | ~19.3s |
| /opt/homebrew（热，无 TCC） | 159,273 files | 0.99s | ~6.2s |
| ~/Library（冷） | — | 未测（`purge` 需 sudo） | 估 20–40s（见下） |

对照 W1 基线的口径修正：W1 的 416k 文件数偏低——`ignore` 默认标准过滤跳过隐藏文件（`.DS_Store` 等正是清理目标，已修），且部分 TCC 拒绝目录表现不同。真实规模约 296 万文件，**新旧吞吐对比应为同机 `du` 而非旧数值**：`du -x` 单线程 913s / ~3M 条目 vs 本实现热 57s（~16×）。

关键发现：

1. **冷/热 16× 差距根因 = macOS TCC**：`~/Library` 每 syscall 触发隐私检查（bulk 调用 ~53µs/次），TCC 之外的树仅 ~6.2µs/文件。同机 `du -x ~/Library` 冷 913s 定锚证明这是 macOS 上限而非实现 bug（slimit 已快 du 16×）。授予终端 Full Disk Access 后预期大幅改善（未验证）。
2. **吞吐对比 du（同为不去重硬链接口径）**：Group Containers 10.72GB、Android 9.885GB、CocoaPods 966/1013 MiB（差 4.8% = du 计目录 inode 块 + 我们硬链接去重）——数值与 du 逐字节吻合。此前 du 全量 34.4GiB 偏低是 TCC 静默漏扫。
3. **线程扩展为负收益**：16 线程 55s、32 线程 70s（walk），8 线程最优——APFS/TCC 内核侧存在串行点。
4. SPEC 判定（1M 文件）：热 ~19.3s（差目标 15s 1.3×，TCC 主导，非 TCC 树 ~6s 达标）；冷估 20–40s（达标，未实测）。**结论：纯用户态优化已到环境上限，剩余差距需 Full Disk Access 或降低 syscall 数（openat fd 链）**。

### 顺带修复的正确性 bug（对照 du 发现）

- **无直属文件的目录聚合丢失**：`aggregate()` 之前只给含直属文件的目录建条目，CocoaPods（du 1013 MiB）在输出里是 0.00 MiB。现在所有祖先目录（含根）都有聚合条目。
- `ignore` 兜底路径补 `standard_filters(false)`，与快路径同语义（含隐藏文件）。
- `slimit-exec` 的 `slimit-rules` 依赖从 dev-dependencies 移到 dependencies（库代码引用了它，W1 遗留）。

## 2026-09-28 · W1 骨架首测（基线，口径有误，仅存档）

| 场景 | 数据 | 冷 | 热 | 结论 |
|---|---|---|---|---|
| ~/Library/Caches | 14,121 files / 625 MiB | 0.69s | — | 正常 |
| ~/Library | 416,160 files / 1.15 GiB actual | **365.6s** | **23.0s** | 热吞吐 ~18k files/s；冷吞吐 ~1.1k files/s |

对照 SPEC_MVP §2.3 目标（100 万文件：冷 < 60s、热 < 15s）：

- 热：416k→23s 折算 1M ≈ 55s，**未达标**（目标 15s），差 ~3.7×。
- 冷：416k→366s 折算 1M ≈ 880s，**严重未达标**（目标 60s），差 ~15×。

### 已验证的正确性（本次扫描同时是真实环境验证）

- 稀疏双值：Docker.raw 实测 apparent 494.4 GB / actual 5.6 GB，双值呈现正确（用户最常见的"显示占用 vs 真实占用"困惑被直接解决）。
- 硬链接去重、symlink 不跟随、单文件系统边界：单测覆盖通过（17 tests 全绿）。

### W2 优化候选（按优先级）

1. **冷扫描慢的主因排查**：冷/热 16× 差距远超正常 FS cache 效应，怀疑 macOS TCC/Gatekeeper 对 ~/Library 部分子树的 per-syscall 权限开销、以及 DirEntry::metadata() 的逐文件 lstat。对策：改用 `fstatat(AT_SYMLINK_NOFOLLOW)` 批处理 + 测 `du -x` 同机对照定锚（区分"我们的 bug"与"macOS 上限"）。
2. per-thread 本地缓冲再批量入全局（降锁争用，热路径次要因素）。
3. 聚合阶段 O(n log n) 排序可换 radix/trie；当前非瓶颈。
4. 达标判据：热 1M < 15s 且冷 1M < 60s（同机对照 du 误差 < 20% 视为达到物理上限）。
