# SlimIt 安全状态

> 本文档汇总自动化扫描结果与设计级安全不变量。**自动化扫描的"无发现"不等于项目安全**——见下文覆盖缺口。

## 自动化扫描

### cargo-audit（主审计，PASS）

| 项 | 值 |
|---|---|
| 日期 | 2026-10-01 |
| 工具 | cargo-audit 0.17 / advisory-db 2026-09-29 |
| Run status | **PASS**（exit 0） |
| Findings | 0 漏洞（460 crate 依赖） |
| 非漏洞警告 | 3 个（proc-macro-error unmaintained、glib unsound、yoke-derive yanked）——按可达性 triage 均不进入 macOS 发布路径，详见下方"依赖审计" |

### cargo-deny（参考，不阻塞）

| 项 | 值 |
|---|---|
| Run status | **FAIL**（advisories + licenses） |
| 原因 | 默认档比 cargo-audit 严格（3 个非漏洞警告触发 advisories FAIL）；licenses 允许清单过窄（MIT/BSD-3-Clause/Unlicense/0BSD/Apache-2.0 双许可"not explicitly allowed"） |
| 处置 | 主审计以 cargo-audit 为准；cargo-deny 的 licenses FAIL 为配置问题，非漏洞，不阻塞发布 |

### 依赖审计（3 个非漏洞警告 triage）

| 警告 | 依赖链 | 可达性 | 评估 |
|---|---|---|---|
| `proc-macro-error` unmaintained | glib-macros → glib → gtk → tauri | 仅 Linux/gtk target（macOS 不编译），构建期 | 无害 |
| `glib 0.18.5` unsound | gtk → tauri（Linux 后端） | 仅 Linux，不在 macOS 发布路径 | 无害 |
| `yoke-derive` yanked | yoke → zerovec → icu → idna → url → reqwest | 运行时可达，但 yanked=下架非漏洞 | 无害，留意 `cargo update` |

**结论**：0 可达 critical/high，cargo-audit PASS 成立。

## 人工审计（2026-10-01，替代 Mimosa 的 partial completeness）

Mimosa 扫描（2026-09-29）completeness: partial，入口识别未覆盖 Tauri command 层。本次人工审计覆盖全部 11 个 `#[tauri::command]` 入口与 executor 路径，发现 **2 个真实问题，已修复**：

### 已修复

1. **`restore` id 路径穿越**（`crates/slimit-exec/src/executor.rs`）
   - **问题**：`Quarantine::entry_dir(id)` 的 `Path::join` 遇绝对路径或 `../` 会逃逸隔离区根，`restore` 的 id 来自 IPC 外部输入，随后 `read_manifest` 与 `remove_dir_all` 都作用在逃逸路径上。
   - **修复**：`is_valid_entry_id` 校验 id 为 32 位 hex（UUID simple 格式），非 UUID id 一律返回 `ApplyError::InvalidId`，隔离区外零写入。
   - **测试**：`restore_rejects_ids_that_escape_quarantine` 行为性红→绿，验证 `../victim` 与绝对路径 id 均被拒绝，逃逸目标未被触碰。

2. **`apply` 信任前端 `executable`**（`crates/slimit-exec/src/plan.rs` + `crates/slimit-tauri/src/lib.rs`）
   - **问题**：`apply_plan` 收到的 `PlanItem` 来自 IPC——`executable` 是客户端自证，SPEC §5 红线"执行授权只来自规则库"靠约定不靠代码。
   - **修复**：新增 `authorize_items(items, rules)` 按规则库重新推导可执行性（复用 canonical `match_rules`+`plan` 逻辑），伪造、未知 rule_id 或 red 规则命中的项一律降级为不可执行。`apply_plan` 在调用 `apply` 前先 `authorize_items`。
   - **测试**：`authorize_items_keeps_rule_authorized_and_downgrades_forged` + `authorize_items_downgrades_unknown_rule_id_and_red` 双绿，验证正常流程幂等、伪造项降级、无副作用。

### 已确认安全（审计通过）

- **AI 输出只进 UI 提示层**：`explain` 命令的云端 AI 失败静默回落，永不阻塞、永不影响执行授权。
- **red 永不执行**：`plan()` 对 `risk: red` 一律 `executable=false`；`apply_plan` 的 `authorize_items` 二次拒绝。
- **删除 = 迁入隔离区**：唯一删除动作是同卷原子 rename 进 `<app_data>/quarantine/` + manifest 落盘；`restore` 绝不覆盖已存在路径（自动加后缀）；审计 JSONL 追加写。
- **command 必带 dry_run**：规则库内 command 规则无 dry_run 不入库（lint 强制）；MVP 中 command 类规则不自动执行。
- **规则库可信输入**：编译期嵌入 + 加载时逐条 lint（kebab-case id、red 必填 red_flags、refs 非空）+ id 全局唯一；嵌入版与目录版一致性有测试锁定。
- **密钥与隐私**：扫描只读文件元数据（dev/ino/size/blocks），不读取文件内容；无网络上传路径（云端解释是 W8+ 的显式开关项）。
- **审计日志**：`AuditLog` 追加写 JSONL，不可改写历史；`apply`/`restore`/`purge-expired` 均记录。
- **隔离区根**：`<app_data_dir>/quarantine/`，id 校验后零逃逸。

## 设计级安全不变量（机制保证，不依赖扫描结论）

SlimIt 是一个有删除能力的工具，安全模型是产品的核心承诺，以下不变量由代码结构强制（SPEC_MVP §5）：

1. **AI 永无删除权**：`slimit-ai` 的 `Explanation` 只进 UI 提示层；执行授权唯一来源是 `slimit-rules` 规则库（`PlanItem.executable` 判定），AI 输出在数据流上不可达执行器。
2. **red 永不执行**：`plan()` 对 `risk: red` 一律 `executable=false`（executor.rs）；`lint` 拒绝 red+purge-dir 组合进库；UI 勾选与 `apply_plan` 二次拒绝（双层保险）。
3. **删除 = 迁入隔离区**：唯一删除动作是同卷原子 rename 进 `<app_data>/quarantine/` + manifest 落盘，任何条目可完整 restore（restore 绝不覆盖已存在路径，自动加后缀）；审计 JSONL 追加写。
4. **command 必带 dry_run**：规则库内 command 规则无 dry_run 不入库（lint 强制）；MVP 中 command 类规则不自动执行。
5. **规则库可信输入**：编译期嵌入 + 加载时逐条 lint（kebab-case id、red 必填 red_flags、refs 非空）+ id 全局唯一；嵌入版与目录版一致性有测试锁定。
6. **密钥与隐私**：扫描只读文件元数据（getattrlistbulk 的 dev/ino/size/blocks），不读取文件内容；无网络上传路径（云端解释是 W8+ 的显式开关项）。
7. **IPC 输入零信任**：`apply_plan` 的 `PlanItem` 经 `authorize_items` 按规则库重校验；`restore_item` 的 id 经 UUID 格式校验；隔离区外零写入。

## 待办

- [x] W8 发布前重跑 Mimosa 并确认 completeness: complete（若仍 partial，人工审计 Tauri command 层与 executor 路径作为替代证据）——**本次人工审计已覆盖全部 11 个 command 入口与 executor 路径，发现 2 个问题已修复**。
- [ ] Tauri capability 最小化复查（当前默认 capability，仅 core:default；确认无需额外 IPC 权限）。
- [ ] 干净机验收（第二台 Mac，需用户执行）。
- [ ] notarization（需 Apple Developer Program 注册，$99/年；公开发布/收费前必须补）。
