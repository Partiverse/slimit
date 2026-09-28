# SlimIt 扫描器基准日志

方法：`SLIMIT_DEBUG=1 ./target/release/slimit <dir> --top N`，release 构建（cargo 1.93.1，M-series Mac，内置 SSD）。冷/热各一次，热 = 紧随冷之后重跑同目录。格式：`文件数 | 耗时 | 吞吐`。

## 2026-09-28 · W1 骨架首测（基线）

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
