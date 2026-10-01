# SlimIt 开发现状与工具链交接（HANDOFF）

> 写作目的：把项目截至本文的现状、进度、约定与原开发环境（ZCode）所使用的技能 / MCP / 命令 / hook 全量记录下来，供新的 AI 开发环境（华为 CodeArts agent）接手开发时一次性读懂上下文。
> 基线：main @ `8bdbe3d`（2026-10-01）。仓库：https://github.com/Partiverse/slimit（私有）。

## 1. 项目是什么

SlimIt（取自 "slim it"）是跨 macOS / Windows / Linux 的存储瘦身工具，差异化定位为「**AI 语义化解释 + 机制级安全清理**」：解释每个目录是什么、谁产生的、删了会怎样，并以隔离区可回滚方式清理。商业模式：免费 / ¥98 买断 / ¥68 每年订阅。路线：macOS MVP（8 周计划，已完成）→ Windows（+2 月）→ Linux 开源（+2 月）。

四条不可妥协红线（全文见 [README](../README.md)，实现层依据见 [SECURITY.md](./SECURITY.md)）：

1. AI 输出永不授权删除；执行授权只来自规则库。
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

W8 已完成：落地页上线 https://slimit.pages.dev（Cloudflare Pages，项目名 `slimit`）；rc1 → rc2 常驻 pre-release（DMG+SHA256+安装说明；**体验打磨与安全修复已提交但尚未发包，rc3 不存在**，下一版发包时随包带上）；种子用户分发材料 [SEED-INVITE.md](./SEED-INVITE.md)；**首批种子反馈 4 条全部落地并随 rc2 发布**——① 云端 AI 解释（OpenAI 兼容端点、用户自备 Key、默认关闭全离线，解释优先级 云端→规则库→启发式，UI 标来源）② 扫描实时进度（条目数/当前目录/速率/用时）③ 多任务并发扫描列表 ④ 规则 81→90 条。**体验打磨已提交（未发包）**：Tab 导航（4 组 6 面板）、自动加载、Plan 表格中文化、空状态、规则面板（list_rules）。**安全收口完成**：cargo-audit 0 漏洞、人工审计 Tauri command 层与 executor 路径，发现 2 个问题已修复（`restore` id 路径穿越、`apply` 信任前端 `executable`），详见 [SECURITY.md](./SECURITY.md)。

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

规则库：`rules/macos/` 共 **90 条**、`rules/linux/` 共 **51 条**、`rules/windows/` 共 **34 条**，合计 **175 条**（CodeArts 批量扩充至 188 后经平台核查收口：删除 13 条平台错配/无可靠来源规则，修复 11 条路径与风险错标——含 Telegram `tdata` 会话密钥误标为可清缓存这一严重项；收口明细见 [COMPLETION-REPORT.md](../COMPLETION-REPORT.md)），契约 `slimit.rules/v1`（[schema-v1.json](../rules/schema-v1.json)，CI 强制校验），一规则一文件，文件名 = 规则 id，`_` 前缀不参与校验。red/advise 双层保险（前端不可勾选 + apply 再拒）。

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
6. **开发者大额回收（v0.2 主打候选，优先级待用户定夺）**：项目感知规则（schema v2 `project-artifact`）+ 扫描器 marker 探测 + 净回收持久度评级 + 重建成本披露——直击「竞品只会清 2GB 浏览器缓存，80GB target/ 无人敢碰」的真空地带，方案见 [RECLAIM-STRATEGY.md](./RECLAIM-STRATEGY.md)。

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
| [rules/README.md](../rules/README.md) | 规则编写规范与执行语义 |
| 本文档 | 现状/进度/工具链交接 |
