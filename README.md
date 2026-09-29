# SlimIt

跨 macOS / Windows / Linux 的存储瘦身工具。核心差异化：**AI 语义化解释 + 机制级安全**——解释每个目录是什么、谁产生的、删了会怎样，并以隔离区可回滚方式清理。

**状态**：MVP W6 完成（2026-09-29）——W2 扫描器 `getattrlistbulk` 批量枚举（[docs/BENCH.md](./docs/BENCH.md)：~/Library 实测 296 万文件热 57s，同机 `du` 913s，快 ~16×）；W3 规则库 33 条 macOS 规则（green 19 / yellow 8 / red 3，validate + golden fixture 全绿）；W4 APFS 探测接入 core：`slimit snapshots <volume>` / `slimit volume <mount>`；W5 Tauri 2 + React 前端骨架；W6 GUI 串联完整清理链（[docs/BRIDGE.md](./docs/BRIDGE.md)）：扫描进度事件流、规则编译期嵌入、plan → apply（隔离区）→ restore、AI 语义解释接入 UI 提示层。仓库：https://github.com/Partiverse/slimit（私有）。

## 文档索引

| 文档 | 内容 |
|---|---|
| [docs/PRODUCT_PLAN.md](./docs/PRODUCT_PLAN.md) | 产品方案（已批准）：市场、竞品、五层架构、商业模式、风险、路线图、三平台目标清单（附录 A） |
| [docs/SPEC_MVP.md](./docs/SPEC_MVP.md) | MVP 技术规格：crate 划分、扫描器/规则引擎/执行器/AI 规格、CI、8 周计划 |
| [docs/BENCH.md](./docs/BENCH.md) | 扫描器基准日志与回归基线（合成树 50k 基准） |
| [docs/BRIDGE.md](./docs/BRIDGE.md) | Tauri 桥接协议：命令/事件/数据结构契约 |
| [docs/SECURITY.md](./docs/SECURITY.md) | 安全状态：扫描结果、覆盖缺口、设计级安全不变量 |
| [docs/BETA.md](./docs/BETA.md) | 内测分发：安装指引（未签名应用通行）、已知限制、回报渠道 |
| [rules/README.md](./rules/README.md) | 规则库编写规范与执行语义 |
| [rules/schema-v1.json](./rules/schema-v1.json) | 规则 schema（`slimit.rules/v1`），CI 强制校验 |
| [rules/macos/](./rules/macos/) | macOS 规则（已建 3 条样例：homebrew-cache / xcode-deriveddata / xcode-archives） |

## 不可妥协红线

1. AI 输出永不授权删除；执行授权只来自规则库。
2. `risk: red` 目标永不代删，只提示。
3. 删除一律进隔离区（14 天可恢复）；系统级不可逆命令必须标 `irreversible` 并二次确认。
4. 未识别路径默认按 `advise` 处理（默认安全）。

## 路线

macOS MVP（8 周）→ Windows（+2 月）→ Linux 开源（+2 月）。定价：免费 / ¥98 买断 / ¥68 每年订阅。
