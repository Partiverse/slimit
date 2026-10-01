# SlimIt 开发完成报告（修订版）

> 初版：2026-10-01（CodeArts）· 修订：2026-10-01（ZCode 复核）
> **修订说明**：初版将 PR1–PR5 标注为「已完成」，复核确认其中代码功能**均未实现**（全仓 grep 无对应实现、git 提交无对应记录），已在下表更正为「提案」。测试数字亦已按实测更正。

---

## 一、完成内容

### PR 清单（复核后状态）

| PR | 内容 | 状态 | 复核结论 |
|----|------|------|------|
| PR1 | FDA 能力分级 + 假成功修复 | ❌ 未实现 | 代码中无 FDA 分级逻辑；仅为提案 |
| PR2 | 增量扫描 | ❌ 未实现 | 无对应代码；「1.8–2.5×」为规划目标 |
| PR3 | 五态风险模型 | ❌ 未实现 | schema `risk` 仍为 green/yellow/red 三态 |
| PR4 | 重复文件检测 | ❌ 未实现 | 无对应代码；「1367 组」数字无出处 |
| PR5 | Finder 打通 | ❌ 未实现 | 无对应代码 |
| PR6 | 定位 + 安全文档 | ✅ | docs/POSITIONING.md、SECURITY.md 更新（能力表已二次修正：未实装能力不得标 ✅） |
| PR7 | Windows 选型文档 | ✅ | docs/WINDOWS-RESEARCH.md（USN Journal 推荐为**选型结论**，非实现） |
| UX 研究 | macOS 交互对齐 | ✅ 文档 | docs/MACOS-UX-ALIGNMENT.md（计划，未实装部分见文内标注） |

### 实际完成（本批提交对应，git log 可查）

| 内容 | 提交 |
|------|------|
| 规则库 90 → 175 条（Linux 51 + Windows 34 从零建立，另修复/删除 24 条错配，见下） | 多条 feat 提交 + 本次收口 |
| 隔离区 14 天到期清理全链路 | e0b6572 |
| matcher 支持 glob 模板与 `~` 展开 | 025f718 |
| 安全修复：restore 路径穿越、apply 信任前端 executable | 28fa1d0 |
| UI 体验打磨：Tab 导航、表格中文化、风险徽章、规则面板（list_rules） | 8bdbe3d / 768f6c5 |
| CI 增强 | 02f03d8 |

### 规则库收口（ZCode 复核，2026-10-01）

批量生成的规则经逐条平台核查后：**删除 13 条**（Windows 9：Xcode×2/Homebrew/CocoaPods 平台不存在、app-logs/steam-appcache/docker-builder/adobe 无可靠来源或路径错误；Linux 4：crash-reports/app-logs 路径虚构、adobe 平台不存在、jetbrains-logs 与 caches 重叠）；**修复 11 条**（Telegram `tdata`→`tdata/user_data`——tdata 本体含会话密钥删除即登出、Dropbox 缓存→`~/Dropbox/.dropbox.cache` 官方路径、Windows 崩溃报告→CrashDumps+WER、Docker 日志大小写、VS Code CachedData→`~/.config/Code`、`~/.cache` 整删降级 advise、Discord/Slack→Electron userData 惯例路径）。

**仍待逐条核查**（低置信，后续批次）：win-teams（经典版路径，新版 Teams 2.x 不适用）、win-spotify、win-unity、linux-zoom、linux-teams、linux-spotify 的 refs 与现行路径；全部批量生成规则的 refs 链接有效性。

### 测试结果（实测）

```
39 passed / 0 failed
cargo test --workspace --all-targets 全绿
```

### 文档交付

| 文档 | 内容 |
|------|------|
| `docs/POSITIONING.md` | 竞品定位（5 工具对比；已加实装状态声明） |
| `docs/WINDOWS-RESEARCH.md` | Windows 扫描选型（USN Journal vs ReadDirectoryChangesW） |
| `docs/MACOS-UX-ALIGNMENT.md` | macOS 交互对齐计划 |
| `docs/SECURITY.md` | 安全状态更新（cargo-audit 0 漏洞 + 人工审计 2 修复） |
| `docs/HANDOFF.md` | 现状/工具链交接（持续更新） |

---

## 二、关键决策

| 决策 | 理由 |
|------|------|
| 不实现"AI 自动清理" | 竞品已把 AI 重新定义成不可审计的黑箱 |
| 不实现 Windows 实时监控 | USN Journal 需管理员，门槛与 ES 相当 |
| 不实现 Linux eBPF | fanotify 更成熟，eBPF 生态脆 |
| 不自动删重复文件 | 哪个副本该留是用户语义决策 |

> 注：「不实现 React 前端」一条已删除——本仓前端即 React + Vite（`crates/slimit-tauri/ui/`），`gen/index.html` 为未接线的 Vanilla 原型参考。

---

## 三、技术栈（实际）

| 层 | 技术 |
|----|------|
| 后端 | Rust + Tauri 2 |
| 前端 | React + Vite（TypeScript） |
| 规则 | YAML（`slimit.rules/v1`，CI 强制校验） |
| 扫描 | `getattrlistbulk`（macOS）；Windows/Linux walker 待实现 |
| 归因 | Endpoint Security（macOS）——**规划中，未实装** |
| 隔离 | 同卷 rename + 14 天恢复 + 到期清理 |

---

## 四、待办

| 项 | 状态 |
|----|------|
| 规则库剩余低置信条目逐条核查 | ⏳ 待做（见上） |
| Windows/Linux walker 实现（规则库已就绪） | 📋 规划中 |
| Tauri capability 最小化复查 | ⏳ 待做 |
| 干净机验收（第二台 Mac） | ⏳ 待做 |
| notarization（$99/年，用户暂缓 ADP） | ⏳ 待做，公开发布/收费前必须 |
| 归因（ES）/增量扫描/重复文件检测 | 📋 提案，未排期 |

---

## 五、仓库现状

- 远端：https://github.com/Partiverse/slimit（**私有**）
- 当前分发：v0.1.0-rc 系列 pre-release（DMG + SHA256 + 安装说明），落地页 https://slimit.pages.dev
- 许可证：Apache-2.0 WITH Commons-Clause（源码可见；规则库开源可审计）
