# W8 计划：notarization、落地页、发布 v0.1

> 2026-09-29 制定。前置：W1–W7 已完成（81 条规则、GUI 清理链经手测、内测包就绪、性能基线达标）。
> W7 收尾核对结论附在文末。

## 1. Notarization（公证与签名）

**前置决策（用户待办）**：加入 Apple Developer Program（$99/年），获得 Developer ID Application 证书的签发权。没有账号则无法公证，`xattr` 右键方案（BETA.md）是唯一替代，不适合公开发布。

| 步骤 | 内容 | 产出/验证 |
|---|---|---|
| 8.1 | 注册 Apple Developer Program，Xcode/钥匙串里创建 **Developer ID Application** 证书 | `security find-identity -v -p codesigning` 可见 |
| 8.2 | `tauri.conf.json` 增加签名配置（`signingIdentity`）；`cargo tauri build` 前设 `APPLE_SIGNING_IDENTITY` | `codesign -dv --verbose=2 SlimIt.app` 显示 Authority=Developer ID |
| 8.3 | 公证：`APPLE_ID`/`APPLE_PASSWORD`（App 专用密码）/`APPLE_TEAM_ID` 环境变量 + `tauri notarize`（或 `xcrun notarytool submit` + `xcrun stapler staple`） | `spctl -a -vv SlimIt.app` → accepted；`xcrun stapler validate` 通过 |
| 8.4 | DMG 同样签名+公证 | 双击安装不再触发 Gatekeeper 警告 |
| 8.5 | 干净机器（或新用户账号）全量验收：下载 → 挂载 → 安装 → 扫描 → 隔离 → 恢复 | 记录到 BETA.md，移除「未签名」相关指引 |

预估：账号审批 1–2 天，脚本化签名/公证半天。

## 2. 落地页

- 域名与托管（用户待办：域名选购；托管建议 Cloudflare Pages/Vercel 免费档）。
- 单页结构（素材直接取自产品）：首屏一句话定位「给 macOS 瘦身：解释每个目录是什么，删了会怎样，可完整恢复」→ 三卖点（AI 语义解释 / 机制级安全隔离区 / 真实占用 vs 表观大小）→ 旗舰场景截图（System Data 解剖、微信数据 red 提示）→ 下载按钮（直发 DMG）→ 定价（免费 / ¥98 买断）→ 隐私声明（扫描只读元数据、不上传）。
- 内容遵守广告法与竞品对比合规：不点名贬低竞品，性能数据标注测试环境。
- MVP 先中文单页；英文版 v0.2。

## 3. 发布 checklist（v0.1）

- [ ] 公证通过的全量验收（8.5）在另一台未开 FD 的机器重跑
- [ ] `SECURITY.md`：重跑 Mimosa 确认 completeness: complete（partial 则人工审计 Tauri command 层与 executor 作为替代证据）
- [ ] GitHub Release：tag `v0.1.0`，附 DMG + SHA256 校验和 + Release Notes（取自 BETA.md 功能清单）
- [ ] README 状态段更新为「v0.1 已发布」
- [ ] 崩溃/反馈渠道落地（GitHub Issues 即可，落地页放链接）
- [ ] 定价与购买（买断 ¥98）：MVP 可先「免费下载 + 打赏/预售」软启动，正式收费依赖 License 机制（v0.2，避免为发布延期强做）

**明确不做**（沿用 SPEC §8）：自动更新器（v0.2 手动下载更新）、Windows/Linux、License 强校验。

## 4. W7 收尾核对结论（按 SPEC §7）

| W7 交付项 | 状态 | 证据 |
|---|---|---|
| 内测包（官网直发） | ✅ 包就绪，渠道动作待用户 | SlimIt.app 9.0 MB / DMG 3.1 MB（release 冒烟存活），`docs/BETA.md` 分发说明 |
| 性能回归 | ✅ 达标（热） | 合成树 50k 基线 0.5–0.9s，折算 1M ≈ 10–18s（SPEC 热 <15s 线内）；冷未测（需 sudo purge，不阻塞）；W6 进度回调零开销经同日双版本对照证实 |
| 规则库 80+ | ✅ 81 条 | validate 全绿 + golden 同步 + 与目录加载一致性测试 |

**与计划的偏差（如实记录）**：① W5 的「语义地图 + 解剖视图」暂为聚合表格视图，解剖视图留待后续版本；② W6「模型版 AI」延后 v0.2（降级版 + 规则库语义已上线，trait 就位）；③ PRODUCT_PLAN M1–M2 的「规则库 150 条」是更大的里程碑口径，81 条先满足 SPEC W7 的 80+；④ 附带成果：GUI 全链路手测修复 2 个正确性 bug（aggregate 祖先泄漏、window.confirm 失效）。
