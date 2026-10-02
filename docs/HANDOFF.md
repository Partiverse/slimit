# SlimIt 开发现状与工具链交接（HANDOFF）

> 写作目的：把项目截至本文的现状、进度、约定与原开发环境（ZCode）所使用的技能 / MCP / 命令 / hook 全量记录下来，供新的 AI 开发环境（华为 CodeArts agent）接手开发时一次性读懂上下文。
> 基线：main @ `8bdbe3d`（2026-10-01）。仓库：https://github.com/Partiverse/slimit（私有）。

## 1. 项目是什么

SlimIt（取自 "slim it"）是跨 macOS / Windows / Linux 的存储瘦身工具，差异化定位为「**AI 语义化解释 + 机制级安全清理**」：解释每个目录是什么、谁产生的、删了会怎样，并以隔离区可回滚方式清理。商业模式：免费 / ¥98 买断 / ¥68 每年订阅。路线：macOS MVP（8 周计划，已完成）→ Windows（+2 月）→ Linux 开源（+2 月）。

四条不可妥协红线（全文见 [README](../README.md)，实现层依据见 [SECURITY.md](./SECURITY.md)）：

1. AI 输出永不授权删除；执行授权只来自规则库或用户对具体路径的显式手动选择（手动项仍强制隔离区 + 审计 + 系统路径保护，2026-10-02 扩展）。
2. `risk: red` 目标永不代删，只提示。
3. 删除一律进隔离区（14 天可恢复）；系统级不可逆命令必须标 `irreversible` 并二次确认。
4. 未识别路径默认按 `advise` 处理（默认安全）。

## 2. 当前进度总览

8 周 MVP 计划（[SPEC_MVP.md](./SPEC_MVP.md) §计划）**W1–W7 全部完成并核对通过**；W8（发布准备）大部分完成。当前版本以常驻 pre-release [v0.1.0-rc2](https://github.com/Partiverse/slimit/releases/tag/v0.1.0-rc2) 向首批种子用户分发中。

| 阶段 | 内容 | 状态 |
|---|---|---|
| W1 | 产品方案与技术规格（PRODUCT_PLAN / SPEC_MVP / 规则 schema） | ✅ 完成，用户已批准三决策（名字 / macOS 先行 / 买断为主） |
| W2 | 扫描器：`getattrlistbulk` 并行 walker | ✅ ~/Library 实测 296 万文件热 57s（同机 `du` 913s，约 16×） |
| W3 | 规则库起步 + 校验/金样测试 | ✅（33 条起步，后扩至 90 条） |
| W4 | APFS 探测：快照列举、卷容量口径 | ✅ `slimit snapshots` / `slimit volume`（数据源 `diskutil -plist`，无私有 API） |
| W5 | Tauri 2 + React TS 前端骨架、桥接契约 | ✅ [BRIDGE.md](./BRIDGE.md) |
| W6 | GUI 清理链串联：进度事件 / plan → 隔离 → 恢复 / AI 解释 | ✅ workspace 全测试绿、clippy 0 warning |
| W7 | 收尾：规则 81 条、性能回归基线、内测包、SPEC 逐项核对 | ✅ 合成树 50k 折算 1M 热 ≈10–18s（达标）；4 项偏差如实记录于 [W8-PLAN.md](./W8-PLAN.md) §4 |
| W8 | 发布准备 | 🔶 见下 |

W8 已完成：落地页上线 https://slimit.pages.dev（Cloudflare Pages，项目名 `slimit`）；rc1 → … → **rc7** 常驻 pre-release（rc7 = 手动清理模式 + 大文件榜单，DMG SHA256 往返校验一致）（rc4 = 项目构建产物识别主线；rc5 = `~` 展开体验修复，DMG SHA256 往返校验一致）；种子用户分发材料 [SEED-INVITE.md](./SEED-INVITE.md) 已更新至 rc5；**首批种子反馈 4 条全部落地并随 rc2 发布**——① 云端 AI 解释（OpenAI 兼容端点、用户自备 Key、默认关闭全离线，解释优先级 云端→规则库→启发式，UI 标来源）② 扫描实时进度（条目数/当前目录/速率/用时）③ 多任务并发扫描列表 ④ 规则 81→90 条。**体验打磨**（随 rc3 发布）：Tab 导航（4 组 6 面板）、自动加载、Plan 表格中文化、空状态、规则面板（list_rules）。**安全收口**：cargo-audit 0 漏洞、人工审计 Tauri command 层与 executor 路径，修复 2 个问题（`restore` id 路径穿越、`apply` 信任前端 `executable`）＋手测追加 1 个（`restore` 未写审计，已修），详见 [SECURITY.md](./SECURITY.md)。

**种子反馈第二批收口（2026-10-02，@ 06c3a25，随 rc8 发布）**：①tab 保活（四面板常驻挂载 display 切换——切 tab 不再丢扫描数据）；②check_fda 授权探测（~/Library/Safari read_dir 可读性）+ 清理页未授权常驻引导条/已授权状态（回应「一个一个点累」「授权后状态不变」）；③计划分页（15 条/页）+ 执行按钮行 sticky 吸底；④表格排布（路径/规则列单行省略号、行高压紧、rule-cell 加宽）；⑤**matcher 对路径/glob 命中也统计 mtime 天数**——所有规则都显示「N 天未动」（此前仅 project 规则）；⑥隔离区单条「立即彻底删除」purge_entry（UUID 校验防穿越 + purge-entry 审计 + UI 两段式）；⑦清理页顶部全局实时进度条。**真机验证（新规矩首次执行，computer-use）**：✅FDA banner、✅tab 保活（切回任务表保留）、✅年龄徽章、✅扫描→计划→错误提示链路、✅加入计划后输入框清空；❌未能捕获：进度条中间态（AX 观察等待期扫描即完成，需大库+FDA 人工确认）、分页（需 >15 条命中）、隔离区删除按钮（验证链被手动加入误报阻断）、表格视觉排布（AX 不呈现换行）。**验证中发现的疑点**：CUA typeText 向手动框输入的串疑似带不可见尾部字符致 probe 误报「路径不存在」（文件确认存在），未复现根因，rc8 notes 已向用户如实说明。dev 实例（bundle_id=null）键盘注入不可用，release 包可用——真机验证一律用 release 包。

**手动清理模式实装（2026-10-02，rc7）**：红线语义正式扩展——执行授权 = 规则库命中 **或** 用户对具体路径的显式手动选择（`PlanItem.origin: rule|user-manual`）。安全设计：①AI 解释永不产生 manual 项（数据流不可达）；②`authorize_items` 对 manual 项服务端**重写全部自证字段**（risk=Yellow、executable=true、rule_id 归一 `user-manual`、durability=OneShot），伪造 IPC 字段无效；③`is_protected_path` 系统路径保护名单：前缀保护（/System /private /usr /bin /sbin /etc /var /dev 整棵子树——/private 覆盖 /etc /var /tmp 的 symlink 目标）+ 自身保护（/ /Volumes /Applications /Library /opt、HOME 与 Desktop/Documents/Downloads/Music/Movies/Pictures/Public/Library/Applications 挡自身、子路径放行，如 `~/Downloads/setup.dmg` ✓、`~/Downloads` ✗）；canonicalize 归一 symlink 与 `..`，失败按保护处理。④全部走隔离区可恢复，审计事件加 origin 标注。配套：core `ScanSummary.top_files` 大文件榜（.dmg/.pkg 安装包场景）、tauri `probe_manual` 探测命令、UI 手动输入 + 大文件/大目录 Top 榜一键加入 + 扫描中不确定进度条。**测试注记**：macOS 测试环境可写路径全在 /private 下，保护名单「放行」断言只能落在纯函数 `is_protected_resolved` 上（is_protected_path 包装层只测保护方向）。56 测试 0 失败。红线措辞已同步 README/AGENTS/HANDOFF/SECURITY。

**种子反馈第一批收口（2026-10-02，@ 5f16e27，随 rc6 发布）**：11 条反馈 9 修 2 排期。已修：①全角 `～` 归一（matcher）；②schema `project.orphan` 反向匹配 + `macos-orphan-node-modules` 孤儿规则（项目已删、产物残留，30 天守卫）；③`macos-trae-cn-leftovers`（本机实查 ~/.trae-cn 为运行数据非纯缓存 → yellow advise）；④AI 设置「测试连接」（slimit-ai::test_connection，/models 探测不耗 token）；⑤高级页 FDA 权限引导卡 + `open_fda_settings` 命令；⑥Dock 偶发无法回窗 → `RunEvent::Reopen` 显式唤起（Builder 改 build().run(closure)）；⑦清理面板全选可执行/反选/全不选 + 大小阈值筛选（全部/10MB/100MB/1GB）；⑧BETA.md 把 `xattr -dr com.apple.quarantine` 提为最可靠打开方式（右键打开部分系统无效——用户实测）；⑨规则库面板副标题说明定位。排期：**手动清理模式**（用户自选任意文件/文件夹进隔离区，回应「没命中规则不能删」与大安装包需求——需放行 authorize_items 对 user-manual 起源项，SECURITY.md 不变量要同步「用户授权 ≠ AI 授权」；下版单独发）与**空间透镜可视化**。另修 QuarantinePanel 重复 useEffect。180 规则 validate 全绿、51 测试 0 失败。rc6 DMG SHA256 f5213e3b。

**手测发现并修复（2026-10-01）**：GUI 手测 rc4 时输入 `~/Library/Caches` 报 `root does not exist`——扫描根从未做 `~` 展开（只有规则路径模板走 `expand_tilde`）。已在 `scan_and_plan` 入口加 `resolve_scan_root`：后端统一展开（`~`、`~/x`，并补 Windows `~\x`）+ 空/不存在/非目录的中文可操作错误提示（HOME 缺失时报错，绝不把字面 `~/x` 当相对路径静默扫错地方）；UI placeholder 改为「支持 ~」。5 个 tauri 单测覆盖（绝对路径/~/~/空白容错/不存在/是文件/HOME 缺失）。**BRIDGE.md 同步**：PlanItem 与 ExplanationRequest 新增 age_days/below_min_age/durability 字段，删除「前端不做 `~` 展开」的旧表述。

**project-artifact 全链路 GUI 手测（2026-10-01，通过）**：造 `oldproj`（`target/` mtime 拨至 2020-01-01，24MB）与 `freshproj`（当日，12MB）两个假 Rust 项目，扫 `/tmp/slimit-e2e` 验证——旧 target 显示「2465 天未动」默认勾选可执行，新 target 显示「0 天未动 · 最近仍在使用，暂不提供清理」且勾选框禁用、沉底排序，执行按钮只计 24MB（守卫项未计入）；两段式确认后 oldproj 的 24MB 完整迁入隔离区、freshproj 的 12MB 零改动、audit.jsonl 落 `quarantine` 事件；UI 恢复后字节级一致（25165824 B），隔离区清空。**抓到并修复 1 个真 bug**：`restore` 从未写审计日志（audit.rs 契约要求 apply/restore 都记），已把 `AuditLog` 收进 `restore()` 签名与 apply 同层，单测断言覆盖。**遗留观察**：`delete_contents_only: true` 时目标目录本身也被 rename 走（原子性优先于留空壳，executor 有注释与 restore 兜底），非 bug；应用热重启后 computer-use 的 app_ref 绑定会失效（自动化层问题，与产品无关），重测需重启绑定。

W8 剩余 / 已决策事项：

- **干净机验收**：需第二台 Mac，未做。
- **正式发布 v0.1 时机**：用户定。
- **签名 + 公证（notarization）**：用户决定暂不注册 Apple Developer Program（$99/年），v0.1 走未签名软启动（安装指引见 [BETA.md](./BETA.md)）；可执行脚本已备好 [scripts/release/sign-notarize.sh](../scripts/release/sign-notarize.sh)（凭据只走环境变量），证书到位后即可用。**公开发布/收费前必须补公证。**
- **域名**：非必须，暂用 slimit.pages.dev；收费/投放前再买。
- **安全扫描**：cargo-audit 0 漏洞（460 crate）；人工审计已覆盖全部 11 个 Tauri command 入口与 executor 路径，发现 2 个问题已修复。详见 [SECURITY.md](./SECURITY.md)。
- v0.2 差异化项：本地模型 AI 解释；Windows/Linux 平台迁移已启动（规则库 51+34 条已就绪并经首轮收口核查，walker 未实现）。

## 3. 架构与代码导览

Rust workspace，五 crate（[Cargo.toml](../Cargo.toml)，license `Apache-2.0 WITH Commons-Clause`）：

| crate | 职责 | 要点 |
|---|---|---|
| `crates/slimit-core` | 扫描器、APFS 探测、聚合 | `getattrlistbulk` 并行 walker；`scan_with_progress` 进度回调（`ScanProgress{files_done,current_dir}`）；`apfs.rs` 快照/卷信息；聚合有「目录永不逃出扫描根」回归测试 |
| `crates/slimit-rules` | 规则库加载与匹配 | `build.rs` 编译期把 `rules/` 嵌入二进制（`embedded_rules()` 与文件加载同管线，测试保证一致）；golden 测试 |
| `crates/slimit-exec` | 计划执行、隔离区、审计 | 删除一律进隔离区（app data 目录，14 天可恢复，`Quarantine::list()`）；审计 JSONL（`<app_data>/audit/audit.jsonl`） |
| `crates/slimit-ai` | AI 解释 | `AiSettings` + 云端解释（OpenAI 兼容端点，用户自备 Key，config.json 存储，默认关闭）；规则命中优先返回规则库 semantics（confidence 0.95），启发式仅兜底 |
| `crates/slimit-tauri` | Tauri 2 壳 + UI | 命令 `scan_and_plan` / `explain` / `apply_plan` / `restore_item` / `list_quarantine`，进度事件 `scan-progress`（带 seq 防乱序）；UI 在 `crates/slimit-tauri/ui/`（手写 Vite React-TS），桥接契约 [BRIDGE.md](./BRIDGE.md)，前端类型化镜像 `ui/src/bridge.ts` |

规则库：`rules/macos/` 共 **93 条**（含 3 条 project-artifact 试点）、`rules/linux/` 共 **51 条**、`rules/windows/` 共 **34 条**，合计 **178 条**（CodeArts 批量扩充至 188 后经平台核查收口：删除 13 条平台错配/无可靠来源规则，修复 11 条路径与风险错标——含 Telegram `tdata` 会话密钥误标为可清缓存这一严重项；收口明细见 [COMPLETION-REPORT.md](../COMPLETION-REPORT.md)），契约 `slimit.rules/v1`（[schema-v1.json](../rules/schema-v1.json)，CI 强制校验；含向后兼容的 `project` 项目感知扩展，见 [PROJECT-ARTIFACT-DESIGN.md](./PROJECT-ARTIFACT-DESIGN.md)），一规则一文件，文件名 = 规则 id，`_` 前缀不参与校验。red/advise 双层保险（前端不可勾选 + apply 再拒）。

## 4. 构建与验证命令

```bash
# 测试 + 静态检查（提交前期望全绿、clippy 0 warning）
cargo test --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace

# 规则库校验（CI 同款）
python3 .agents/skills/slimit-rule-author/scripts/validate.py rules/

# 修改规则后更新 golden（先更新、再普通跑验证）
SLIMIT_UPDATE_GOLDEN=1 cargo test -p slimit-rules
cargo test -p slimit-rules

# GUI 开发 / 打包（注意 tauri.conf.json 的 beforeBuildCommand cwd 已指向 ui/）
cargo tauri dev
cargo tauri build

# 性能回归基线（同日同树对照，跨日数字不可直比——方法学见 BENCH.md）
cargo run -p slimit-core --example bench -- --synth   # 固定 50k 文件合成树

# 落地页部署（Cloudflare Pages，需 wrangler 登录）
npx wrangler pages deploy landing --project-name slimit

# 签名 + 公证（需 Apple Developer 证书；四个凭据变量必须先导出，见脚本头注释）
bash scripts/release/sign-notarize.sh
```

GitHub Release 发布用 `gh` CLI（现例：`gh release create v0.1.0-rcX` 上传 DMG + SHA256 + 安装说明，发布前后做 SHA256 往返校验）。

## 5. 开发约定

- **规则新增/修改必须走** [rules/README.md](../rules/README.md) 编写规范 + `.agents/skills/slimit-rule-author/SKILL.md` 工作流：事实核查先行（`refs` 必填官方来源）→ 模板填写 → 风险分级从紧（green=可再生 / yellow=有代价需判断 / red=只 advise 且必填 `red_flags`）→ `command` 规则必须带 `dry_run` → 删除默认 `delete_contents_only: true` → 官方清理命令优先于裸删 → 本地 validate 通过再提交；`yellow`/`red` 判级必须人工复核，不得自动合并风险升级。
- **性能结论**必须同日同树对照（历史教训：跨日跨树数字漂移会被误判为回归）。
- **提交信息**：中文 conventional 风格（`feat:` / `fix:` / `docs:` / `chore:`），偏差与未决事项如实写进文档，不粉饰（先例：SECURITY.md 如实记录 inconclusive）。
- **手测注意**：dev 模式 rust 重编会重启应用并可能残留孤儿进程，自动化测试前先 `pkill` 清场；Tauri WKWebView 里 `window.confirm` 不可靠，一律用应用内两段式确认。
- **凭据纪律**：一切密钥只走环境变量，不入库、不进日志（sign-notarize.sh 是范本）。

## 6. 工具链说明

原开发环境（ZCode CLI）的部分能力属环境专属（安全扫描插件、网页调研、GUI 自动化、发布插件、commit hook 提醒等），**不随仓库分发，接手环境无需寻找对应物**——所需的实际操作全部是 §4 中仓库内可直接执行的命令。仓库内自带、接手环境直接可用的工具只有：

- `.agents/skills/slimit-rule-author/`（已入库）：规则编写/审查/批量生成工作流（SKILL.md）；其 `scripts/validate.py` 为 CI 同款校验，`python3 .../validate.py rules/` 直接跑。
- 依赖安全审计等价物：`cargo-audit` / `cargo-deny`（历史结论：551 依赖 0 advisory 命中；当前 460 依赖 0 漏洞，3 个非漏洞警告按可达性 triage 均不进入 macOS 发布路径，详见 [SECURITY.md](./SECURITY.md)）。

用户协作习惯（回复语言、推进方式、下一步建议等）已固化在根 [AGENTS.md](../AGENTS.md)「协作习惯」节，接手环境直接遵循；下一步工作清单见 [AGENTS.md](../AGENTS.md)「下一步工作」与本档 §7。

## 7. 建议的任务池（接手后可立即开工）

1. **干净机验收**：按 [W8-PLAN.md](./W8-PLAN.md) §3 发布 checklist 在第二台 Mac 上走完（需用户提供机器）。
2. **Windows/Linux 规则库持续扩充与剩余核查**：现 51+34 条（首轮收口后）；低置信条目（win-teams / win-spotify / win-unity、linux-zoom / linux-teams / linux-spotify）与全部批量生成规则的 refs 链接待逐条核查；Windows/Linux walker 未实现（规则库已就绪），建议同步核对缺口。
3. **体验打磨**：种子反馈「交互一般、语义模板化」的后续迭代（UI 与解释文案质量），配合云端 AI 使用率观察。
4. **签名 + 公证（notarization）**：用户决定暂不注册 Apple Developer Program（$99/年），v0.1 走未签名软启动；**公开发布/收费前必须补公证**。
5. **v0.2 差异化**：本地模型（~3B）AI 解释，保持「默认全离线」承诺。
6. **开发者大额回收（v0.2 主打候选，优先级待用户定夺）**：R1（schema `project` 扩展 + 3 条试点规则 + 年龄守卫）、R3（UI 年龄徽章/守卫沉底）、R4（explain 年龄证据模板）、R5（durability A/B/C 分类 + A 类优先排序 + 新手/专家档）**均已实装并随 rc4 发布**；未做：Windows/Linux walker（规则库已就绪）、跨平台镜像试点规则、R2' walker 短路优化。方案见 [RECLAIM-STRATEGY.md](./RECLAIM-STRATEGY.md) 与 [PROJECT-ARTIFACT-DESIGN.md](./PROJECT-ARTIFACT-DESIGN.md)。

## 8. 文档索引

| 文档 | 内容 |
|---|---|
| [PRODUCT_PLAN.md](./PRODUCT_PLAN.md) | 产品方案（市场/竞品/架构/商业模式/路线图） |
| [SPEC_MVP.md](./SPEC_MVP.md) | MVP 技术规格（crate 划分、各模块规格、8 周计划） |
| [BRIDGE.md](./BRIDGE.md) | Tauri 桥接协议（命令/事件/数据结构契约） |
| [BENCH.md](./BENCH.md) | 扫描器基准日志、回归基线与方法学 |
| [SECURITY.md](./SECURITY.md) | 安全状态、覆盖缺口、设计级安全不变量 |
| [BETA.md](./BETA.md) | 内测分发：未签名应用安装指引、已知限制 |
| [W8-PLAN.md](./W8-PLAN.md) | 发布计划：notarization、落地页、发布 checklist、W7 核对结论 |
| [SEED-INVITE.md](./SEED-INVITE.md) | 种子用户邀请文案与分发材料 |
| [RECLAIM-STRATEGY.md](./RECLAIM-STRATEGY.md) | 深度回收策略调研：竞品为什么清不动大头、A/B/C 分类学、项目感知方案（v0.2 主打候选） |
| [PROJECT-ARTIFACT-DESIGN.md](./PROJECT-ARTIFACT-DESIGN.md) | 项目感知规则设计：schema `project` 扩展、匹配算法、年龄守卫、授权重放一致性 |
| [rules/README.md](../rules/README.md) | 规则编写规范与执行语义 |
| 本文档 | 现状/进度/工具链交接 |
