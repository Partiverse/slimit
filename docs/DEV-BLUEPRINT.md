# Slimit 长任务开发蓝图（v0.1 收尾 → v0.2 → 远期）

> 制定：2026-10-04 · 基于：rc12（SHA d0d399cc）+ docs/V2-ROADMAP.md（已评审定稿）+ 深查新实锤（iCloud 阻塞 P0 / getattrlistbulk 丢子树 / EINTR 丢条目）
> 用法：每个 Phase 就是一个 workflow 的一棒（子代理拓扑见各节），按序推进；集成模式（commit 到 main，不逐批发 rc）。

## Phase A — v0.1 正确性收尾（P0 还债，先行）

| # | 任务 | 规模 | 验收 |
|---|---|---|---|
| A1 | **枚举超时+跳过+告警机制**：目录枚举 syscall 加超时看门狗（iCloud/文件提供器阻塞的根治），超时目录记告警跳过不挂死 | M | 造 iCloud 阻塞夹具，扫描在时限内完成且告警列出被跳目录 |
| A2 | **open EINTR 重试**：bulk.rs open<0 即放弃子树 → EINTR 重试（PEP 475 语义），其余错误走 fallback | S | OneDrive EINTR 夹具子树全额补录（verify_and_fill 已兜底，双保险） |
| A3 | **通知同步缺陷排查**：LaunchAgent 无头扫描后 osascript 通知不可确认（log show 无踪迹） | S | 真机观察到通知或定位根因（通知权限/身份） |
| A4 | **空目录语义文档化**：dirs 由文件 parent 链推导，空目录不进快照——写入 HANDOFF/BETA 已知限制，消除「识别不出来」误解 | S | 文档落位 |

Phase A 完成即发 rc13（v0.1 正确性定稿版）。

## Phase B — v0.2 主线（按 docs/V2-ROADMAP.md 定稿排序）

| # | 任务 | 规模 | 依赖 |
|---|---|---|---|
| B1 | **空间透镜可视化**（sunburst/treemap，清理页入口） | M | 扫描数据已在（top_dirs/top_files） |
| B2 | **重复文件检测**（SHA-256，隐私边界设计先行——打破「只读元数据」现状需 SECURITY 评审） | L | SECURITY 评审 |
| B3 | **本地模型 AI 解释**（~3B，llama.cpp/MLX，离线差异化主打） | L | 模型选型调研 |
| B4 | **Windows walker**（USN Journal，docs/WINDOWS-RESEARCH.md 选型已定）+ Windows 规则镜像启用 | L | Windows 环境 |
| B5 | **Linux walker**（fanotify/inotify）+ Linux 规则镜像启用 | M | Linux 环境 |
| B6 | **定时扫描 L3**（green 预授权，设计稿 docs/L3-AUTOCLEAN-DESIGN.md；SECURITY 不变量第 7 条需用户确认） | M | 用户确认 |

## Phase C — 打磨与远期

- 交互打磨（用户痛点 top-N，输入待收集）
- 文件类型聚合视图（S）
- 自动更新器评估（v0.1 明确不做，重新评估时机）
- notarization（用户已暂停，公开发布/收费前必须）

## 工作流执行方式

每个 Phase 走一个 workflow：子代理拓扑 = 每任务一个实现代理 + 一个真机验证代理 + 一个集成代理（收尾合并 HANDOFF、commit push、不发版除非用户指令）。蓝图文档即工作流的输入上下文。
