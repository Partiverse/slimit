# SlimIt MVP 技术规格（macOS）

> 版本：v0.1 · 日期：2026-09-28 · 上游文档：[PRODUCT_PLAN.md](./PRODUCT_PLAN.md)
> MVP 范围：macOS 单平台，8 周，旗舰场景 = "解剖 System Data" 完整闭环（扫描 → 语义解释 → 计划预演 → 隔离区清理 → 可恢复）。

---

## 1. 总体架构

Rust workspace，核心与 UI 分离：

```
slimit/
├── crates/
│   ├── slimit-core/      # 扫描器：并行遍历、真实占用计算、平台探测
│   ├── slimit-rules/     # 规则引擎：加载/校验/匹配 YAML 规则库
│   ├── slimit-exec/      # 执行器：清理计划、隔离区、恢复、审计日志
│   ├── slimit-ai/        # 解释器 trait + 本地模型实现 + 降级策略
│   └── slimit-tauri/     # Tauri 2 壳 + 前端（TS/React）
├── rules/                # 规则库（YAML，独立版本化，支持热更新）
│   ├── schema-v1.json    # JSON Schema，CI 校验
│   └── macos/*.yaml
└── docs/
```

依赖方向：`tauri → exec → rules → core`；`ai` 独立，被 tauri 调用。所有跨 crate 数据结构定义在 `slimit-core`（或后续抽 `slimit-types`）。

## 2. slimit-core 扫描器规格

### 2.1 遍历
- 并行 walk（`ignore`/`jwalk` 或自研 rayon 层），遵循 symlink 不跟随、单文件系统边界（`-x` 语义，跨挂载点记为独立卷）。
- 每节点收集：path、`st_dev/st_ino/st_nlink/st_size/st_blocks/st_mtime`、是否目录/符号链接。

### 2.2 真实占用（对标竞品的"统计幻觉"修正，本产品的地基）
| 现象 | 处理 |
|---|---|
| 硬链接 | `(st_dev, st_ino)` 去重：多路径共享 inode 只在首次出现处计块数，其余路径标 `shared` |
| 稀疏文件 | `actual = st_blocks * 512`，`apparent = st_size`；差值大时 UI 显示双值（Docker.raw 场景） |
| APFS 快照 | `diskutil apfs listSnapshots -plist` 列出快照；快照占用无法逐个精确归因（块共享），UI 标注"估算"：`tmutil thinlocalsnapshots / 0 1` 干跑思路 + purgeable 卷属性读数 |
| purgeable | 无公开精确 API。MVP 用 `NSURLVolumeIsEjectableKey`/卷能力属性 + 快照列表推断，UI 文案明确"估算值" |
| 已删仍持有 | MVP 不做（Linux 场景为主），排 v1.1 |

### 2.3 性能目标（CI 基准固定机器 M-series + 内置 SSD）
- 100 万文件冷扫描 < 60s；热缓存 < 15s。
- 内存峰值 < 500MB（流式聚合，不整树驻留；树结构按需从聚合节点下钻重建）。
- 基准方法遵循 `dev-benchmark-methodology` skill（届时调用）。

## 3. slimit-rules 规则引擎规格

### 3.1 规则库形态
- YAML 文件，一文件一规则，`rules/macos/*.yaml`；`schema-v1.json` 为唯一契约，CI 强制校验（见 §6）。
- 随应用内置 + 启动时检查热更新（签名包，v1.1 实现；MVP 仅内置）。

### 3.2 匹配与执行分类
- `paths` 支持 `~` 展开、环境变量、`*` glob；每条规则产出 0..n 个命中目标。
- `action.kind`（MVP 三种）：
  - `purge-dir`：删目录内容（`delete_contents_only: true` 强制保留目录本身）。走隔离区。
  - `command`：执行官方命令。必须同时给 `dry_run` 命令；执行前先跑 dry_run 解析可回收量。系统级命令（tmutil/DISM 类）标 `irreversible: true`，不可进隔离区，UI 黄条 + 二次确认。
  - `advise`：不执行，仅展示解释与官方操作路径。`risk: red` 规则强制 `advise` 或 `irreversible`。
- 路径未命中任何规则 → 归入"未识别"，默认按 `advise` 处理（默认安全原则）。

## 4. slimit-exec 执行器规格

### 4.1 两阶段执行
1. **Plan**：`Vec<PlanItem>`，每项含：规则 id、目标路径、预计回收字节（真实占用口径）、风险等级、后果说明、可否恢复。
2. **Apply**：逐项执行，产生 `AuditEntry`（JSONL 追加写，含时间戳、规则 id、路径、字节、结果、隔离区 id）。

### 4.2 隔离区
- 位置：`~/Library/Application Support/SlimIt/quarantine/<ulid>/`。
- `purge-dir` 实现 = `rename()` 到隔离区（同卷原子、零拷贝）+ `manifest.json`（原始路径、时间、规则、文件清单摘要）。跨卷 fallback：copy + delete，UI 提示耗时。
- 恢复 = 按 manifest `rename()` 回原路径；原路径已存在则生成带后缀路径，绝不覆盖。
- 保留期默认 14 天，到期的后台任务仅删除隔离区内副本。注意：隔离区本身占空间，UI 在体检中显示其体积。
- `command` 类不可隔离：执行结果依赖官方命令语义，审计日志记录 dry_run 与实跑输出。

### 4.3 权限模型
- MVP 全程用户态；`sudo` 场景（/Library/Caches 部分子目录）MVP 仅提示，不内置提权（v1.1 评估 privileged helper，需 Apple 公证配合）。

## 5. slimit-ai 解释器规格

- 输入：`ExplanationRequest { path, size_actual, mtime, owner_bundle (从 .app/Info.plist 反查), sibling_summary, nearest_rule_hits }`。
- 输出（严格 JSON，schema 校验，失败重试 1 次后降级）：
  ```json
  { "what": "...", "producer": "...", "consequence": "...",
    "suggested_risk": "green|yellow|red", "confidence": 0.0-1.0 }
  ```
- **红线：AI 输出只进 UI 提示层，永不写入规则匹配结果、永不影响执行器授权。** 执行授权只来自规则库。
- 实现：MVP 先上"启发式降级版"（无模型：基于 owner bundle、路径分词、文件类型分布的模板化解释），模型版（3B GGUF，candle 或 llama-server 本地 sidecar）W6 接入；云端 API 为 Pro+ 预留接口，显式开关。

## 6. 质量与 CI

- `rules/` CI：JSON Schema 校验 + lint（id 唯一、os 前缀一致、red 必填 red_flags、command 必带 dry_run、refs 非空）+ golden 测试（每条规则配 fixture 路径树，断言匹配与计划输出）。
- 执行器测试：临时目录 fixture，跑 plan→apply→restore→purge 全周期，断言字节守恒与无越界删除（隔离区外不得出现 rename 目标）。
- 平台 CI：macOS runner 跑全量；扫描器基准回归（阈值劣化 >15% 阻断）。
- 发布：Developer ID 签名 + notarization；可复现构建（`cargo dist` 或等效）。

## 7. MVP 周计划

| 周 | 交付 |
|---|---|
| W1–2 | workspace 骨架；core 扫描器（并行 + inode 去重 + 稀疏双值）达到性能目标 |
| W3 | rules 引擎 + schema + CI 校验；内置 30 条 macOS 规则（附录 A 精选） |
| W4 | exec 隔离区 + 审计 + 计划预演 API；exec 测试全绿 |
| W5 | purgeable/快照探测；Tauri UI：扫描结果语义地图 + System Data 解剖视图 |
| W6 | AI 解释（降级版→模型版）；UI 串联 plan→apply→restore |
| W7 | 内测包（官网直发）；性能回归；规则库扩到 80+ 条 |
| W8 | notarization、落地页、发布 v0.1 |

## 8. 明确不做（MVP）

- Windows/Linux（后续版本）；实时监测守护；卸载器功能；重复文件查找；sudo 提权清理；云同步；规则热更新通道。
