# SlimIt 产品方案（立项审批稿）

> 版本：v0.1 草案 · 日期：2026-09-28 · 状态：待审批
> 调研方法：4 个并行调研（macOS / Windows / Linux / 竞品与市场），全部结论有信源支撑，见文末。

---

## 1. 一句话定位

**SlimIt 是一款跨 macOS / Windows / Linux 的存储瘦身工具，核心差异化是"语义化解释 + 机制级安全"：不只告诉用户哪里占了多少 GB，而是解释每个目录是什么、谁产生的、删了会怎样，并以可回滚的方式安全清理。**

产品名取自 "slim it"——把盘瘦身。

---

## 2. 问题：三大系统的空间都被什么吃掉了

调研确认的三平台"空间黑洞"全景（详细清单见附录 A）：

### 2.1 共性痛点

| 痛点 | 证据 |
|---|---|
| **"系统数据"黑盒** | Apple 官方对 System Data 的定义就是"不属于任何标准类别的所有文件"，Apple 自己都不拆解。社区一年内出现三个专治 System Data 的独立产品（FreeUpMyMac、Trace、MacPrune），有用户 System Data 高达 220–300GB 且 CleanMyMac 清不动 |
| **"看起来能删，删了会痛"** | Xcode Archives（删了无法符号化已发布应用）、iOS 整机备份（单台 10–100GB）、Docker volume、conda pkgs（官方警告会破坏环境）、微信聊天记录目录（10–80GB）——这些是所有清理工具的误删重灾区 |
| **统计幻觉** | macOS purgeable space 最高可占磁盘 80% 且同时被算进"可用"和"已用"；Windows WinSxS 因 NTFS 硬链接"虚胖"（显示 5GB 实际开销 507MB）；Docker.raw 稀疏文件让开发者自己都分不清显示占用与真实占用；Linux 上 df 满、du 加总对不上（已删除仍被进程持有的文件、overlayfs 层） |
| **开发者场景命令割裂** | `xcrun simctl delete unavailable`、`brew cleanup`、`docker system prune`、`journalctl --vacuum`、`paccache -r`、diskpart compact vdisk——没有一个工具把这些和浏览器缓存、旧备份放进同一张地图 |

### 2.2 平台特有痛点

- **macOS**：System Data 失控是长期第一痛点；Xcode 全家桶（DerivedData 单项目 1–10GB、模拟器运行时每个 5–8GB、DeviceSupport 每个 iOS 版本 1–5GB）；APFS 快照与 purgeable 造成的"明明删了文件空间不回来"。
- **Windows**：C 盘爆红是中文互联网常青话题；WinSxS 不能碰只能 DISM 清理；hiberfil.sys 要用 powercfg 调而不是删；WSL/Docker 的 ext4.vhdx 只涨不缩需要 diskpart compact；微信/QQ 数据目录动辄几十 GB 且绝不能手删。
- **Linux**：桌面端"CleanMyMac 级"整合产品空位真实存在（分析器只读、清理器不可视化、唯一整合者 Stacer 已弃坑）；服务器端 Docker overlay2、journald、snap 旧 revision（默认留 3 份）是三大黑洞；还有"已删除但仍被进程持有"这类需要专业诊断的场景。

---

## 3. 竞品格局与空位

### 3.1 主要玩家（2026-09 现状）

| 产品 | 平台 | 价格 | 软肋 |
|---|---|---|---|
| CleanMyMac（MacPaw） | macOS | ≈$40/年 订阅，或 ~$120 买断 | 订阅疲劳（HN："为删缓存收 $40/年够了"）；System Data 清不动；黑盒 |
| CCleaner（Gen Digital） | Win/Mac/Android | 免费 + ~$40/年 | 2017 供应链攻击（227 万用户中毒）；Avast 卖浏览数据被 FTC 罚 $16.5M；2025-10 v7 移除自启管理再引众怒 |
| DaisyDisk | macOS | $9.99 终身买断 | 只可视化不解释不清理 |
| WizTree | Windows | 个人免费 | 纯分析器，无语义无清理 |
| 微软电脑管家 | Windows | 免费 | 与 Storage Sense 重叠，压制第三方收费空间 |
| Pearcleaner | macOS | 免费开源 | 开发已停滞；无全系统规则库 |
| DiskCopilot / CleanMyJunk / ai-disk-cleaner | macOS 等 | $9.90 买断 / 免费 | "AI"实现很薄（右键跳浏览器问 ChatGPT），无原生集成、无安全执行闭环 |
| **MacPaw Eney（最大威胁）** | macOS | 含在 Setapp 订阅 | 2025-05 上线的 AI 助理，已覆盖 system cleanup。但是对话式助理形态，不是"逐目录语义标注 + 一键安全清理" |

### 3.2 五个空位假设的验证结论

| 假设 | 结论 | 对产品的含义 |
|---|---|---|
| (a) 工具只给数字不给语义解释 | **成立，但窗口正在收窄**（Eney、DiskCopilot 已入场，现存 AI 实现都很薄） | 必须快，且要做出"原生集成 + 离线规则库"的深度，不能只做套壳 |
| (b) System Data 黑盒没人解好 | **强成立**，主流玩家无人把它当核心场景 | 列为招牌场景，官网 demo 直接演这个 |
| (c) 开发者与普通用户场景割裂 | **成立**，无工具把 DerivedData/Docker.raw/node_modules/WSL vhdx 与消费级清理放进一张语义地图 | 同时覆盖两类用户，是收费能力的主要来源（开发者目录价值密度最高） |
| (d) 安全信任缺失 | **强成立**，且是品类最深护城河 | 安全必须是**可验证机制**（隔离区、可回滚、dry-run、官方命令路径），不是口号 |
| (e) 跨平台一致体验缺失 | **成立**，无任何产品跨三端提供一致语义化体验；Linux 侧几乎空白 | 三端共享同一规则库与引擎，Linux 作为开源信誉阵地 |

### 3.3 市场参照

- System utilities 软件市场 2025 年规模约 **$13.6B**（The Business Research Company 口径），无"磁盘清理"独立细分报告。
- 价格锚被两头挤压：$9.99 买断（DaisyDisk/DiskCopilot）与完全免费（WizTree/PC Manager/开源）之间，$40/年订阅只能靠品牌全家桶支撑。
- 用户社区（r/macapps）明确表达的最优模式：**买断为主 + 可选订阅**。清理是入口，解释与持续健康监测才是合理的续费理由。

---

## 4. 产品方案

### 4.1 产品概念：五层架构

```
┌─────────────────────────────────────────────┐
│  L5 存储健康监测     增长告警、体检分、周报      │
├─────────────────────────────────────────────┤
│  L4 安全执行引擎     计划预演 → 隔离区 → 恢复/清空 │
├─────────────────────────────────────────────┤
│  L3 AI 语义解释      规则库未覆盖路径的按需解释    │
├─────────────────────────────────────────────┤
│  L2 真实占用分析器    硬链接/稀疏/快照/purgeable 感知│
├─────────────────────────────────────────────┤
│  L1 语义规则库       500+ 路径知识库，开源可审计   │
└─────────────────────────────────────────────┘
```

**L1 语义规则库（核心资产，开源）**
- 覆盖三平台 500+ 已知路径/目标，每条包含：路径、语义解释、产生者、安全等级、删除后果说明、重建成本、正确的官方清理命令。
- **开源可审计**——直接回应 CleanMyMac "Safety Database 黑盒"的信任痛点，也是社区贡献与 SEO 入口。
- 规则库里"清理动作"优先调用**官方命令**（`simctl delete unavailable`、`brew cleanup`、`docker system prune`、`journalctl --vacuum`、DISM、powercfg、diskpart compact），而不是裸 `rm`。这是与所有竞品的本质区别：竞品删文件，SlimIt 按各系统设计者预期的姿势清理。

**L2 真实占用分析器**
- 性能对标 WizTree/dust：Windows 直读 NTFS MFT，全平台并行扫描，Rust 实现。
- **正确处理统计幻觉**：硬链接不重复计数、稀疏文件区分显示占用与真实占用、macOS 标注 purgeable 与 APFS 快照、Linux 检测已删除仍被持有的文件（等价 `lsof +L1`）、Windows 对 WinSxS 调用 `DISM /AnalyzeComponentStore` 给真实开销。
- 这是"语义化"的地基：先给用户看对的数字，再谈解释。

**L3 AI 语义解释**
- 规则库未覆盖的路径：基于路径上下文、文件签名、应用元数据、邻居文件，由 **本地小模型**生成解释（"这是什么 / 谁产生 / 删了会怎样 / 建议安全等级"），可选云端大模型增强。
- 隐私即卖点：默认离线，文件路径不出本机；云端模式显式开关。
- **AI 只做解释与建议，永不直接授权删除**。安全等级由确定性规则库决定；AI 判断仅作为提示。这条红线同时解决 LLM 幻觉风险与信任问题。

**L4 安全执行引擎（信任的机制化）**
- **两阶段执行**：先出"清理计划"（每项标注后果预演：删了会怎样、能回收多少、何时能重建），用户确认后才执行。
- **隔离区而非直接删除**：所有删除先进 SlimIt 管理的隔离区（带清单 manifest），默认保留 14 天，可一键完整恢复，确认无恙后自动清空。特例（`journalctl vacuum`、DISM 等系统级操作不可逆的）明确标黄提示。
- **三档安全等级**（每项清理目标都标注）：
  - 🟢 **S 级可自动清**：各类可再生缓存（~/Library/Caches、%TEMP%、~/.cache、包管理器缓存、缩略图）；
  - 🟡 **B 级需确认**：旧 iOS 备份、APFS 快照、Windows.old、旧内核、snap 旧 revision、传递优化缓存；
  - 🔴 **R 级仅提示不代删**：Xcode Archives、微信/QQ 数据目录、conda pkgs、Docker volumes、系统还原点、WSL vhdx、WinSxS——给出解释和官方操作路径，让用户自己动手或一键跳转官方工具。
- **保护名单硬编码**：用户文档、照片、SSH keys、Mail、任何不在规则库中的未识别路径默认按 R 级处理（默认安全，识别了才升级）。

**L5 存储健康监测**
- 常驻轻量监测（菜单栏/托盘）：空间增长异常告警（"过去 7 天 System Data 增长 12GB，主要来自 XX"）、体检分、月度存储报告。
- 这是订阅续费的合理理由：清理是一次性的，健康是持续的。

### 4.2 旗舰场景（营销主打）

1. **"解剖 System Data"**（macOS）：一键把"系统数据"拆解成可理解的清单，每行带解释。直击搜索量最大的痛点。
2. **"开发者空间账单"**：一张表列出 Xcode/Docker/模拟器/node_modules/WSL/包管理器缓存各自占多少、能安全回收多少。竞品无人覆盖。
3. **"C 盘体检"**（Windows）：WinSxS 真实开销、hiberfil 调优建议（powercfg 而非删文件）、vhdx 压缩、微信目录迁移指引。
4. **Linux 一体化清理器**：填补 Stacer 死后的空位（分析 + 解释 + 安全删除 + 官方命令封装），开源版优先发布积累信誉。

### 4.3 三平台功能矩阵

| 能力 | macOS | Windows | Linux |
|---|---|---|---|
| 快速扫描（真实占用） | ✅ | ✅ MFT 直读 | ✅ 并行 + 硬链接感知 |
| 语义规则库 | ✅ 150+ 条 | ✅ 150+ 条（含微信/QQ/WSL） | ✅ 150+ 条（apt/pacman/snap/flatpak/docker/journald） |
| AI 本地解释 | ✅ | ✅ | ✅（含 CLI） |
| 隔离区 + 恢复 | ✅ | ✅ | ✅ |
| 官方命令封装 | simctl/brew/tmutil | DISM/powercfg/diskpart/cleanmgr | journalctl/paccache/dnf/snap |
| 稀疏文件/硬链接/purgeable 感知 | ✅ | ✅ | ✅ |
| 健康监测 | ✅ 菜单栏 | ✅ 托盘 | ✅ CLI + 可选守护 |
| 界面形态 | 原生 GUI | GUI（Tauri） | GUI + CLI 优先 |

---

## 5. 技术架构（概要）

- **核心引擎**：Rust 单一跨平台内核（扫描器、规则引擎、执行器、隔离区），对标 dua/dust/WizTree 的性能水准。平台特异逻辑（MFT、purgeable、DISM）做成平台 adapter。
- **规则库**：独立数据仓库（YAML/JSON），版本化 + 社区 PR，随应用热更新（不依赖发版）。
- **GUI**：Tauri 2（Windows/Linux）+ macOS 原生 SwiftUI 或 Tauri 统一（MVP 阶段建议 Tauri 统一起步，控制成本）。
- **AI**：本地解释用 3B 级小模型（MLX/ONNX Runtime/GGUF），云端可选接入主流 API。解释输出结构化（what/producer/consequence/risk），渲染层统一。
- **分发**：官网直发 + Apple notarization（Developer ID 签名，预算内做 Defender/各 AV 白名单申请）。Mac App Store 只放功能受限版当获客橱窗（沙盒限制决定了清理类应用无法全功能上架，这是 CleanMyMac 验证过的路径）。Linux 走 GitHub Releases + 包管理器 + Homebrew。
- **安全工程**：所有删除操作经统一执行器审计日志；隔离区与 Time Machine/系统回收站不冲突；签名 + 公证 + 可复现构建。

---

## 6. 商业模式

| 档位 | 定价（建议） | 内容 |
|---|---|---|
| **SlimIt Free** | ¥0 / $0 | 全平台扫描 + 语义解释 + 每次清理上限 2GB / S 级目标 |
| **SlimIt Pro 买断** | ¥98 / $12.99（单平台）或 ¥148 / $19.99（三平台） | 解锁全部清理、开发者场景包、隔离区延长保留 |
| **SlimIt Pro+ 订阅** | ¥68/年 / $9.99/年 | 云端 AI 增强、健康监测与告警、规则库优先更新、多设备 |

定价逻辑：买断锚定 DaisyDisk（$9.99）之上、CleanMyMac（$40/年）之下，占"比可视化贵一点、比全家桶便宜一个量级"的心智；订阅只绑可持续成本（云 AI、监测），避免"为删缓存付订阅费"的公愤点。中国市场为首发重点（Windows C 盘痛点 + 微信场景 + 官网直发免抽成）。

---

## 7. 风险与对策

| 风险 | 等级 | 对策 |
|---|---|---|
| MacPaw Eney 等大厂 AI 清理收窄窗口 | 高 | 聚焦 Eney 没有的形态：逐目录语义标注 + 开源规则库 + 开发者场景；6 个月内出 MVP |
| 误删事故毁掉品牌（品类死于信任） | 高 | R 级默认不代删 + 隔离区可恢复 + AI 无删除授权 + 开源规则库接受审计；把"零不可逆事故"作为工程 KPI |
| Windows Defender / AV 误报 | 中 | 签名 + 公证预算从第一天列入；主动申请主流 AV 白名单 |
| Apple 沙盒限制 | 中 | 官网直发为主，App Store 只放阉割版（行业标准路径） |
| LLM 幻觉给出错误删除建议 | 中 | AI 永不持删除权；解释标注置信度；规则库答案优先于 AI |
| 免费开源竞品压价 | 中 | 免费版保留完整解释能力（信任入口），付费点在执行闭环与持续服务；核心引擎可选 source-available |
| 单平台一款爆品路径依赖 | 低 | 三平台共享内核，边际成本低；Linux 版兼作开源信誉资产 |

---

## 8. 路线图

- **M1–M2 · MVP（macOS）**：L2 扫描器（含 purgeable/快照/稀疏感知）+ L1 规则库 150 条 + L4 隔离区执行 + L3 本地 AI 解释（规则库兜底）。旗舰场景 1（System Data 解剖）完整可用。
- **M3–M4 · Windows 版**：MFT 扫描 + WinSxS DISM 集成 + hiberfil/vhdx/微信场景 + 中文市场官网首发。
- **M5–M6 · Linux 版（开源）**：CLI 优先 + GUI；GitHub 开源规则库与核心；服务器场景（docker/journald/K8s 节点）差异化。
- **M7+ · Pro+ 与生态**：健康监测上线、云端 AI、规则库社区共建、Setapp/渠道分发评估。

成功指标（MVP 期）：首次扫描中位可回收空间 ≥ 15GB；清理后 7 天隔离区恢复率 < 1%；零不可逆事故；免费→买断转化 ≥ 3%。

---

## 附录 A：三平台清理目标速查（调研精华，将固化为规则库 v0）

### macOS（Top 目标）
| 目标 | 语义 | 等级 | 官方姿势 |
|---|---|---|---|
| ~/Library/Caches | 应用可再生缓存 | 🟢 | 逐应用清，保留目录 |
| Xcode DerivedData | 构建产物+索引 | 🟢 | 全删自动重建 |
| 模拟器设备/运行时 | 每台数 GB / 每运行时 5–8GB | 🟡 | `xcrun simctl delete unavailable`；`simctl runtime delete --notUsedSinceDays N`（先 dry-run） |
| Homebrew 缓存 | 下载包，可达 20GB+ | 🟢 | `brew cleanup --prune=all` |
| Docker.raw | 稀疏文件，显示≠实际 | 🟡 | VM 内 `docker system prune`，勿删文件 |
| APFS 本地快照 | purgeable 主要来源 | 🟡 | `tmutil thinlocalsnapshots` |
| iOS 备份 | 单台 10–100GB | 🟡 | Finder > Manage Backups |
| Xcode Archives | 符号化依据，删了不可逆 | 🔴 | Organizer 逐个删 |
| conda pkgs | 链接中的包不可删 | 🔴 | `conda clean --all --dry-run` 先看 |

### Windows（Top 目标）
| 目标 | 语义 | 等级 | 官方姿势 |
|---|---|---|---|
| WinSxS | 组件库，硬链接虚胖，直删=变砖 | 🔴→🟡 | `DISM /AnalyzeComponentStore` → `StartComponentCleanup`（/ResetBase 需告知不可回滚更新） |
| SoftwareDistribution\Download | 更新缓存 | 🟡 | 停 wuauserv/bits 后删 Download 子目录 |
| hiberfil.sys | 休眠文件，删文件会被重建 | 🟡 | `powercfg /h /type reduced`（省 30%）或 `/h off`（失去快速启动，需用户确认） |
| Docker/WSL ext4.vhdx | 只涨不缩 | 🟡 | `docker system prune` → `wsl --shutdown` → diskpart compact vdisk / `--set-sparse` |
| Windows.old | 旧系统回滚副本 15–40GB | 🟡 | 回滚期满后 cleanmgr "Previous Windows installations" |
| 还原点/VSS | 删了不可逆 | 🔴 | `vssadmin delete shadows` 前必须用户明示 |
| 微信 xwechat_files / QQ Tencent Files | 聊天记录，整删=数据丢失 | 🔴 | 应用内迁移路径 + 应用内缓存清理 |
| %TEMP%、缩略图、着色器缓存、传递优化 | 可再生 | 🟢 | 直接清 / cleanmgr |

### Linux（Top 目标）
| 目标 | 语义 | 等级 | 官方姿势 |
|---|---|---|---|
| journald | 默认上限 10%/4G，但小盘占比高 | 🟢 | `journalctl --vacuum-size=200M`，勿 rm journal 目录 |
| apt/dnf/pacman 缓存 | pacman 永不自动清，滚动用户 10–50GB | 🟢/🟡 | `apt clean` / `dnf clean all` / `paccache -rk1`（保留降级能力） |
| snap 旧 revision | 默认留 3 份完整 squashfs | 🟡 | 删 disabled revision；`refresh.retain=2` |
| flatpak 未用 runtime | 无引用的 runtime | 🟢 | `flatpak uninstall --unused` |
| /var/lib/docker | overlay2 层+卷+构建缓存，CI 机 50–200GB | 🟡/🔴 | `docker system prune`（--volumes 需明示，可能删数据）；禁碰 overlay2 内部 |
| 已删仍被持有文件 | df 满 du 对不上的真凶 | 🟡 | `lsof +L1` → 截断或重启持有进程 |
| node_modules / pip / conda / uv 缓存 | 可再生 | 🟢 | `pip cache purge`、`uv cache clean`、`conda clean --all` |
| ~/.config 无主残留 | 已卸载应用的配置 | 🟡 | 核对包名后删，默认按未识别处理 |

---

## 附录 B：调研审计记录

- 4 个并行子代理产出独立报告（macOS / Windows / Linux / 竞品市场），原始报告含完整信源 URL。
- 合并规则：≥2 个信源或官方文档背书的事实进入正文；单一社区信源标注"社区报告"；未能核实原文（403 反爬）的数字标注转述来源。
- 主要取舍：市场缺乏"磁盘清理"独立细分数据，采用 system utilities 口径并自下而上佐证（Gen Digital $5B 营收、CCleaner 20 亿下载、CleanMyMac 2000 万下载）；CCleaner/TBRC 定价与规模数字为二手转述，已在附录标注。
- 平台调研与竞品调研结论互洽处（System Data 痛点、订阅疲劳、跨平台空位）已交叉引用。
