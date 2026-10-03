# W8 计划：notarization、落地页、发布 v0.1

> 2026-09-29 制定。前置：W1–W7 已完成（81 条规则、GUI 清理链经手测、内测包就绪、性能基线达标）。
> W7 收尾核对结论附在文末。
>
> **用户决策（2026-09-29）**：暂不注册 Apple Developer Program。v0.1 以未签名构建按 BETA.md
> 方案（右键打开）软启动内测；**公开发布/收费前必须补 notarization**（本文件 §1 整段顺延）。
> 签名+公证的可执行骨架已备好：`scripts/release/sign-notarize.sh`（全部凭据走环境变量）。

## 1. Notarization（公证与签名）——暂缓，证书到位后执行

**前置决策（用户待办，已暂缓）**：加入 Apple Developer Program（$99/年），获得 Developer ID Application 证书的签发权。没有账号则无法公证，`xattr` 右键方案（BETA.md）是唯一替代，不适合公开发布。

| 步骤 | 内容 | 产出/验证 |
|---|---|---|
| 8.1 | 注册 Apple Developer Program，Xcode/钥匙串里创建 **Developer ID Application** 证书 | `security find-identity -v -p codesigning` 可见 |
| 8.2 | `tauri.conf.json` 增加签名配置（`signingIdentity`）；`cargo tauri build` 前设 `APPLE_SIGNING_IDENTITY` | `codesign -dv --verbose=2 Slimit.app` 显示 Authority=Developer ID |
| 8.3 | 公证：`APPLE_ID`/`APPLE_PASSWORD`（App 专用密码）/`APPLE_TEAM_ID` 环境变量 + `tauri notarize`（或 `xcrun notarytool submit` + `xcrun stapler staple`） | `spctl -a -vv Slimit.app` → accepted；`xcrun stapler validate` 通过 |
| 8.4 | DMG 同样签名+公证 | 双击安装不再触发 Gatekeeper 警告 |
| 8.5 | 干净机器（或新用户账号）全量验收：下载 → 挂载 → 安装 → 扫描 → 隔离 → 恢复 | 记录到 BETA.md，移除「未签名」相关指引 |

预估：账号审批 1–2 天，脚本化签名/公证半天。

## 2. 落地页

**初稿已完成**：`landing/index.html`（单文件、无依赖、深浅色自适应），部署说明见 `landing/README.md`。

- **域名非必须**：Cloudflare Pages 免费子域（如 `slimit.pages.dev`）足够内测与软启动；正式收费/投放前再选购绑定，几分钟生效，不阻塞任何事。（2026-09-29 结论）
- 单页结构（已按此实现）：首屏一句话定位「给 macOS 瘦身：解释每个目录是什么，删了会怎样，可完整恢复」→ 三卖点（AI 语义解释 / 机制级安全隔离区 / 真实占用 vs 表观大小）→ 规则行为示例表（green/yellow/red 各一）→ 性能数据（标注测试环境）→ 定价（免费 / ¥98 买断预告）→ 隐私声明（只读元数据、不上传）。
- 内容遵守广告法与竞品对比合规：不点名贬低竞品，性能数据标注测试环境。
- MVP 先中文单页；英文版 v0.2。

## 3. 发布 checklist（v0.1）

- [ ] 干净机器全量验收：下载 → 挂载 → 安装（右键打开）→ 扫描 → 隔离 → 恢复，结果记入 BETA.md
- [ ] `SECURITY.md`：重跑 Mimosa 确认 completeness: complete（partial 则人工审计 Tauri command 层与 executor 作为替代证据）
- [ ] GitHub Release：tag `v0.1.0`，附 DMG + SHA256 校验和 + Release Notes（取自 BETA.md 功能清单）
- [x] 落地页部署：**已上线 https://slimit.pages.dev**（2026-09-29，Cloudflare Pages 免费子域，wrangler 部署；占位链接已替换为 Releases/Issues 实际地址）
- [x] GitHub Release 流程演练通过（v0.1.0-rc1：上传→下载 SHA256 往返一致）；rc1 已重建为常驻 pre-release 供种子分发，分发材料见 `docs/SEED-INVITE.md`
- [ ] README 状态段更新为「v0.1 已发布」
- [ ] 崩溃/反馈渠道落地（GitHub Issues 即可，落地页放链接）
- [ ] 定价与购买（买断 ¥98）：v0.1 先「免费下载 + 打赏/预售」软启动，正式收费前补 notarization 与 License 机制（v0.2，避免为收费延期发布）

**明确不做**（沿用 SPEC §8）：自动更新器（v0.2 手动下载更新）、Windows/Linux、License 强校验。

## 4. W7 收尾核对结论（按 SPEC §7）

| W7 交付项 | 状态 | 证据 |
|---|---|---|
| 内测包（官网直发） | ✅ 包就绪，渠道动作待用户 | Slimit.app 9.0 MB / DMG 3.1 MB（release 冒烟存活），`docs/BETA.md` 分发说明 |
| 性能回归 | ✅ 达标（热） | 合成树 50k 基线 0.5–0.9s，折算 1M ≈ 10–18s（SPEC 热 <15s 线内）；冷未测（需 sudo purge，不阻塞）；W6 进度回调零开销经同日双版本对照证实 |
| 规则库 80+ | ✅ 81 条 | validate 全绿 + golden 同步 + 与目录加载一致性测试 |

**与计划的偏差（如实记录）**：① W5 的「语义地图 + 解剖视图」暂为聚合表格视图，解剖视图留待后续版本；② W6「模型版 AI」延后 v0.2（降级版 + 规则库语义已上线，trait 就位）；③ PRODUCT_PLAN M1–M2 的「规则库 150 条」是更大的里程碑口径，81 条先满足 SPEC W7 的 80+；④ 附带成果：GUI 全链路手测修复 2 个正确性 bug（aggregate 祖先泄漏、window.confirm 失效）。
