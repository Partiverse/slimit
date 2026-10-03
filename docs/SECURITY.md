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

3. **`restore` 未写审计日志**（`crates/slimit-exec/src/executor.rs`）——GUI 全链路手测抓到（2026-10-01，project-artifact 场景）
   - **问题**：`audit.rs` 的契约是"每次 apply/restore 追加一行"，但 `restore()` 从未接收也从未写入 `AuditLog`；本节此前一度声称 `restore` 已记录，属文档与实现不符（已更正）。后果：隔离区迁出不可追溯，违反红线④「删除必须可追溯」的对称要求。
   - **修复**：`restore(quarantine, id, audit)` 收 `&mut AuditLog`，成功 rename + 清理隔离条目后追加 `restore` 事件（含 id / 实际恢复路径 / original_path / rule / bytes / `renamed` 标记是否因原路径被占而改名）；tauri `restore_item` 同步构造 AuditLog。与 `apply` 同层保证，不依赖调用方自觉。
   - **测试**：`apply_quarantine_then_restore_roundtrip` 内断言 `restore` 事件字段齐备（id/path/renamed）；另两条 restore 测试同步改签名。

### 已确认安全（审计通过）

- **AI 输出只进 UI 提示层**：`explain` 命令的云端 AI 失败静默回落，永不阻塞、永不影响执行授权。
- **red 永不执行**：`plan()` 对 `risk: red` 一律 `executable=false`；`apply_plan` 的 `authorize_items` 二次拒绝。
- **project 规则年龄守卫**：`below_min_age`（年龄不足或 mtime 不可得）⇒ `executable=false`；`authorize_items` 重放同逻辑。拿不到年龄证据时按需保护处理，所有不确定方向单调朝安全。
- **删除 = 迁入隔离区**：唯一删除动作是同卷原子 rename 进 `<app_data>/quarantine/` + manifest 落盘；`restore` 绝不覆盖已存在路径（自动加后缀）；审计 JSONL 追加写。
- **command 必带 dry_run**：规则库内 command 规则无 dry_run 不入库（lint 强制）；MVP 中 command 类规则不自动执行。
- **规则库可信输入**：编译期嵌入 + 加载时逐条 lint（kebab-case id、red 必填 red_flags、refs 非空、project 规则单段名防路径注入）+ id 全局唯一；嵌入版与目录版一致性有测试锁定。
- **密钥与隐私**：扫描只读文件元数据（dev/ino/size/blocks），不读取文件内容；无网络上传路径（云端解释是 W8+ 的显式开关项）。
- **审计日志**：`AuditLog` 追加写 JSONL，不可改写历史；`apply`/`restore`/`purge-expired` 三类事件均在 executor/command 层实装（非仅约定）。
- **隔离区根**：`<app_data_dir>/quarantine/`，id 校验后零逃逸。

## 设计级安全不变量（机制保证，不依赖扫描结论）

SlimIt 是一个有删除能力的工具，安全模型是产品的核心承诺，以下不变量由代码结构强制（SPEC_MVP §5）：

1. **AI 永无删除权**：`slimit-ai` 的 `Explanation` 只进 UI 提示层；AI 输出在数据流上不可达执行器。执行授权来源有二（2026-10-02 扩展）：① `slimit-rules` 规则库命中（`PlanItem.executable` 判定）；② **用户对具体路径的显式手动选择**（`PlanOrigin::UserManual`，应种子反馈「没命中规则就不能删吗」新增）。手动模式的授权主体是用户本人而非 AI——AI 解释永不产生 manual 项，数据流上不可达。
2. **red 永不执行**：`plan()` 对 `risk: red` 一律 `executable=false`（executor.rs）；`lint` 拒绝 red+purge-dir 组合进库；UI 勾选与 `apply_plan` 二次拒绝（双层保险）。
3. **删除 = 迁入隔离区**：唯一删除动作是同卷原子 rename 进 `<app_data>/quarantine/` + manifest 落盘，任何条目可完整 restore（restore 绝不覆盖已存在路径，自动加后缀）；审计 JSONL 追加写（quarantine/restore/purge 事件均含 `origin` 标注 rule 与 user-manual）。
4. **command 必带 dry_run**：规则库内 command 规则无 dry_run 不入库（lint 强制）；MVP 中 command 类规则不自动执行。
5. **规则库可信输入**：编译期嵌入 + 加载时逐条 lint（kebab-case id、red 必填 red_flags、refs 非空、project 规则单段名防路径注入、`orphan` 反向匹配同受单段名限制）+ id 全局唯一；嵌入版与目录版一致性有测试锁定。
6. **密钥与隐私**：扫描只读文件元数据（getattrlistbulk 的 dev/ino/size/blocks），不读取文件内容；无网络上传路径（云端解释是显式开关项，`test_ai` 仅探测 `/models` 鉴权，不发送用户数据）。
7. **IPC 输入零信任**：`apply_plan` 的 `PlanItem` 经 `authorize_items` 重校验——rule 项按规则库重推 executable；**user-manual 项服务端重写全部自证字段**（risk 强制 Yellow、executable 强制 true、rule_id 归一 `user-manual`、durability 重置），并强制通过 `is_protected_path` 系统路径保护名单（前缀保护：/System /private /usr /bin /sbin /etc /var /dev 整棵子树，含 /etc /var /tmp 的符号链接目标；自身保护：/ /Volumes /Applications /Library /opt、HOME 本身与 Desktop/Documents/Downloads/Music/Movies/Pictures/Public/Library/Applications 一级目录挡自身、子路径放行——`~/Downloads/setup.dmg` 允许，`~/Downloads` 整体挡住）。canonicalize 归一 symlink 与 `..` 绕过；canonicalize 失败（不存在）按保护处理，安全方向单调。`restore_item` 的 id 经 UUID 格式校验；隔离区外零写入。

## 待办

- [x] W8 发布前重跑 Mimosa 并确认 completeness: complete（若仍 partial，人工审计 Tauri command 层与 executor 路径作为替代证据）——**本次人工审计已覆盖全部 11 个 command 入口与 executor 路径，发现 2 个问题已修复**。
- [x] Tauri capability 最小化复查——**复查发现项目此前从未创建 capability 文件**（Tauri v2 无 capability 即拒绝全部前端 API，`listen` 静默失败，这正是进度事件到不了前端的根因之一）；已新建 `capabilities/default.json` 最小集：`core:default` + `core:event:default`（事件订阅必需）+ `core:window:allow-{set-focus,show,unminimize}`（Dock Reopen 唤窗所需）。文件即权限清单，新增 IPC 能力必须同步此文件。
- [ ] 干净机验收（第二台 Mac，需用户执行）。
- [ ] notarization（需 Apple Developer Program 注册，$99/年；公开发布/收费前必须补）。
