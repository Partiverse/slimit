# 深度回收策略调研：从「每周 2GB 安慰剂」到「一次 80GB 真回收」

> 动机：用户实测 CleanMyMac——一次扫描清理 2–3 GB，以浏览器缓存为主（清了很快再生，无净收益），而机器上近 80 GB 的 Rust 构建产物（`target/`）它完全不碰。本文分析这类「清理工具为什么清不动真正的磁盘大头」的结构性原因，提出 SlimIt 的应对方案与路线建议。
> 日期：2026-10-01 · 状态：调研稿，待用户定夺优先级。

## 1. 问题的三个失败模式（以 CleanMyMac 为样本）

1. **总量小**：单次 2–3 GB，对动辄 200–500 GB 可回收空间的开发机是零头。开发者社区的普遍共识与之一致：清理类 App「clean 的是缓存，不是磁盘大头」，Reddit 上 "Stop using CleanMyMac" 是高赞主流意见。
2. **对象低效**：主战场是浏览器缓存、应用缓存、日志——B 类「周期性再生」目标，清完下次扫描又满，净收益趋近于零，本质是**安慰剂循环**。
3. **对大头视而不见**：`target/`、`node_modules`、DerivedData、VM 磁盘这类单点 10–80 GB 的目标不进入清理策略。社区指南给出的一般估计：开发者机器上失效的 `node_modules`/DerivedData/构建目录普遍积压 **50–100 GB**；单个 Rust `target/` 可达 5–15 GB。

## 2. 为什么竞品只能做到这样（结构性原因，不是执行力问题）

| 原因 | 机制 |
|---|---|
| **责任规避** | 缓存「删错了也没事」；项目构建产物是**工作成果的衍生物**，删错（比如误删了用户源码混着的目录）要担责。把策略限制在可再生缓存上，是法务/客诉最优解，不是用户价值最优解。 |
| **商业模式反激励** | 订阅制需要**会再生的**清理对象：浏览器缓存下周又满、下次扫描又有 2GB 可报——这恰恰是续费理由。一次帮用户回收 80GB 且不再复发的工具，对按年收费是负资产。**激励与用户利益错位**是这个品类的病根。 |
| **技术架构限制** | 传统清理器的策略库锚定固定路径（`~/Library/Caches/...`）。而构建产物活在**任意项目目录**里——「任何一个旁边有 `Cargo.toml` 的 `target/`」这种模式，路径库架构表达不了。SlimIt 当前规则引擎同样有此限制（见 §6）。 |
| **客群错位** | 它们的目标用户是「想一键维护的普通 Mac 用户」，开发者属于「用 DaisyDisk/`du` 自己动手」的 geeker 群体。于是出现真空地带：**不敢自己动手的小白 + 想省事的所有人**，没人服务。 |

## 3. 真实磁盘大头分类学（按「净回收持久度」排序）

核心指标：**净收益 = 释放量 × 持久度 − 重建成本**（重建成本 = 时间 + 流量 + 心理负担）。

| 类 | 特征 | 典型目标（macOS 开发机） | 单点量级 | 重建成本 |
|---|---|---|---|---|
| **A·一次性大额** | 建成后不再复现，回收一次赢一次 | 项目 `target/`、`node_modules`、Xcode DerivedData、Bazel/CMake 输出、`.next`/`dist`、Xcode DeviceSupport、旧 iOS Simulator 运行时、HuggingFace/Ollama 旧模型、Steam shader cache | 5–80 GB/项 | 首次重建变慢（分钟级）或需重新下载 |
| **B·周期性再生** | 清了会回来 | 浏览器缓存、App 缓存、日志、包管理器下载缓存 | MB–2 GB/类 | 近乎为零 |
| **C·用户数据** | 删了不可逆 | WeChat/QQ 聊天数据、VM 磁盘、Xcode Archives | 数 GB–数百 GB | 不可接受（SlimIt red：只提示） |

竞品全部火力集中在 B 类；**A 类是无人区**，也是用户痛点最深（80GB 摆在那）却最需要安全机制才能解锁的地方。

## 4. 核心论点：小白不敢删，缺的不是「看到大小」，是「被担保的安全」

空间透镜（DaisyDisk/CleanMyMac Space Lens）解决的是**看见**；geeker 看见即行动，小白看见即恐惧。AI 时代的技术门槛被 AI 抹平意味着：用户敢去尝试以前不敢做的事——**前提是有人给他可验证的担保**。这恰好是 SlimIt 的既有红线体系（AI 只解释、规则才授权、red 不代删、隔离区 14 天、审计日志）的用武之地，但要把「担保」做进产品语义里，还差四块：

1. **再生担保**：明说「删了会发生什么、什么会回来、什么永远不会回来」。对 A 类的语义是「**代码不会丢，只是下次构建/安装要重新来一遍**」——这句话就是小白需要的全部勇气。
2. **年龄证据**：`mtime` 距今天数（APFS 上 `atime` 不可靠，用 mtime）。「这个 `node_modules` 已 97 天未动」比「它有 3.2GB」更有说服力，也是社区共识的安全启发式（**30 天未动的项目依赖才建议删**）。
3. **重建成本披露**：执行前如实标注「首次 `cargo build` 预计 3–10 分钟 / `npm install` 需重新下载 ~400MB」——诚实本身就是信任。
4. **回滚兜底**：隔离区已有的 14 天机制，在 A 类场景要把「删错了也能救回来」讲在删除**之前**。

## 5. 生态工具扫描：单点工具已验证需求，统一层仍空缺

| 工具 | 范围 | 缺口 |
|---|---|---|
| [cargo-sweep](https://github.com/holmgr/cargo-sweep) / cargo-clean-all | 仅 Rust `target/`（可按天数/工具链清） | 单语言、CLI |
| [npkill](https://npkill.js.org) / Spaci | 仅 `node_modules` | 单生态、CLI |
| DevCleaner for Xcode | 仅 Xcode 域 | 单域、GUI 但无解释 |
| [DevClean](https://devclean.app)（macOS） | node_modules/DerivedData/Pods/构建输出 | **最接近的直接竞品**；无隔离区回滚、无可审计规则、无 AI 解释、无 B/C 类统一视角 |
| CleanMyMac / DaisyDisk | B 类缓存 / 纯可视化 | 见 §1、§2 |

**SlimIt 的空位**：唯一同时具备「A+B+C 全类别统一视角 + 项目感知回收 + 可审计规则 + 隔离区回滚 + AI 解释证据链」的工具。对小白，是唯一敢替他把 80GB 拆成「可放心清理」并担保障责任的层。

## 6. 技术方案：SlimIt 需要的改动（按依赖序）

现状核实：当前 175 条规则全部锚定 `~` 固定路径（glob 仅用于随机后缀场景，如 Firefox Profiles），**没有任何规则能命中项目内构建产物**——用户 80GB 的 `target/` 正落在这个盲区。这不是加规则能解决的，需要引擎升级：

- **R1 规则 schema 升级（v1→v2）**：新增 `kind: project-artifact` 规则类型——`marker`（项目根标记：`Cargo.toml` / `package.json` / `.xcodeproj` / `pyproject.toml` / `go.mod`…）+ `rel_paths`（`target` / `node_modules` / `DerivedData` / `.gradle` …）+ `max_age_days`（低于阈值只提示不勾选）。沿用现有风险分级/dry_run/refs 纪律。
- **R2 扫描器项目感知**：walker 命中 marker 目录后按 project-artifact 规则聚合子目录体积。性能上是**利好**：命中 `target/` 后可短路统计而不再遍历几十万文件明细（现在的全量遍历反而是这类目录的性能杀手）。
- **R3 PlanItem 扩展**：`durability`（one-shot/regenerating/user-data）与 `rebuild_cost` 字段；UI 按净收益排序，B 类默认折叠或降权——把「每次 2GB」的安慰剂排名让位给「一次 80GB」。
- **R4 解释模板**：A 类专属语义（「代码不丢，只是重新编译/下载」+ 年龄证据 + 重建成本预估），喂给现有 云端→规则库→启发式 解释链。
- **R5 新手/专家双档**：新手档默认只展示 A 类高置信 + 全部强制隔离区；专家档放开 B 类与命令类规则。

## 7. 路线建议

1. **把「开发者大额回收」提为 v0.2 主打，优先于本地模型 AI**。理由：本地 3B 模型解决的是「解释更有 AI 味」（体验增益），R1–R5 解决的是「敢删 80GB」（价值增益）；后者同时是对 CleanMyMac 系的差异化主战场，且用户本人就是第一受益人（80GB 实证）。
2. B 类不放弃但**重新定位**：作为「顺手清理」而非卖点，UI 明示「此类清完会再生」——诚实本身构成与竞品的区分度。
3. 发布口径调整：从「每周清理工具」改叙为「**大扫除 + 大额回收**」——landing 页与种子用户反馈轮可验证该叙事。

## 8. 参考证据

- Reddit r/mac 对 CleanMyMac 的主流评价（"Stop using CleanMyMac, it's not worth"）：reddit.com/r/mac/comments/slvme0、r/macapps PureMac 对比帖
- cargo-sweep：github.com/holmgr/cargo-sweep（按天数/工具链清 target/）；cargo-clean-all（递归清多项目）
- npkill：npkill.js.org；Spaci（同类新工具）
- DevClean：devclean.app（macOS 开发垃圾清理，直接竞品）
- 社区量级共识：失效 node_modules/DerivedData/构建目录普遍 50–100GB；单个 Rust target/ 5–15GB；「30 天未动才删」启发式（MacPaw/diskcleaner.app/tidy.bar 指南综合）
