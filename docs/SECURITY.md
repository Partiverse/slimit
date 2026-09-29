# SlimIt 安全状态

> 本文档汇总自动化扫描结果与设计级安全不变量。**自动化扫描的"无发现"不等于项目安全**——见下文覆盖缺口。

## 自动化扫描（Mimosa deep）

| 项 | 值 |
|---|---|
| 日期 | 2026-09-29 |
| 深度 | deep（static_only_no_runtime_execution） |
| Run status | **inconclusive**（completeness: partial） |
| Findings | 0（0 business-logic candidate） |
| 依赖扫描 | 551 个包（Cargo.lock 全量），离线 advisory 比对 0 命中 |
| Seal | `sha256:ac2f0299…27e473653` |
| 扫描工件 | `~/.mimosa/security-scans/project-63bda5b17c307006…/scan-2026-09-29T04-27-50.692Z-8f1d155b3f55/` |

**覆盖缺口（扫描器自报，照录）**：

- 调用图部分不完整：部分调用为动态派发或超出分析规模，跨文件可达性可能不完整。
- 源文件仅选取 30 个进入深度分析（threat model 阶段 entryPoints/principals/authorizationSurfaces 均为 0 观测，说明入口识别未覆盖 Tauri command 层）。

**结论**：本次扫描给出的是"在已审范围内未发现问题"，而非"项目安全"。W8 发布前应重跑并确认 completeness 达到 complete。

## 设计级安全不变量（机制保证，不依赖扫描结论）

SlimIt 是一个有删除能力的工具，安全模型是产品的核心承诺，以下不变量由代码结构强制（SPEC_MVP §5）：

1. **AI 永无删除权**：`slimit-ai` 的 `Explanation` 只进 UI 提示层；执行授权唯一来源是 `slimit-rules` 规则库（`PlanItem.executable` 判定），AI 输出在数据流上不可达执行器。
2. **red 永不执行**：`plan()` 对 `risk: red` 一律 `executable=false`（executor.rs）；`lint` 拒绝 red+purge-dir 组合进库；UI 勾选与 `apply_plan` 二次拒绝（双层保险）。
3. **删除 = 迁入隔离区**：唯一删除动作是同卷原子 rename 进 `<app_data>/quarantine/` + manifest 落盘，任何条目可完整 restore（restore 绝不覆盖已存在路径，自动加后缀）；审计 JSONL 追加写。
4. **command 必带 dry_run**：规则库内 command 规则无 dry_run 不入库（lint 强制）；MVP 中 command 类规则不自动执行。
5. **规则库可信输入**：编译期嵌入 + 加载时逐条 lint（kebab-case id、red 必填 red_flags、refs 非空）+ id 全局唯一；嵌入版与目录版一致性有测试锁定。
6. **密钥与隐私**：扫描只读文件元数据（getattrlistbulk 的 dev/ino/size/blocks），不读取文件内容；无网络上传路径（云端解释是 W8+ 的显式开关项）。

## 待办

- [ ] W8 发布前重跑 Mimosa 并确认 completeness: complete（若仍 partial，人工审计 Tauri command 层与 executor 路径作为替代证据）。
- [ ] Tauri capability 最小化复查（当前默认 capability，仅 core:default；确认无需额外 IPC 权限）。
