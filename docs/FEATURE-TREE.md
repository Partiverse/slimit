# SlimIt 功能逻辑树（开发前必读）

> 目的：把全部功能的**数据流、状态归属、联动关系、不变量**画成一张图，开发任何新功能前先在此核对会影响哪些节点（2026-10-02 用户明令：先梳理清洗再开发）。
> 基线：rc8 → rc9 开发中。

## 1. 顶层数据流

```
用户输入路径 ──► scan_and_plan ──► core::scan（getattrlistbulk，进度事件 scan-progress）
                     │                    │
                     │                    ▼
                     │            ScanResult{files, dirs} ──► summarize ──► ScanSummary{top_dirs, top_files}
                     │                                                    │
                     ▼                                                    ▼
            embedded_rules ──► matcher.match_rules ──► Match[] ──► plan() ──► PlanItem[]
                                                              （服务端权威）
PlanItem[]（IPC，客户端自证不可信）──► apply_plan ──► authorize_items 重校验 ──► apply()
                                                                          │
                                                                quarantine/<id>/ + manifest
                                                                          │
                                                              audit.jsonl（origin 标注）
```

## 2. PlanItem 生命周期与授权不变量

- **来源**（origin）：`rule`（规则库命中）/ `user-manual`（用户显式手选）。
- **授权**：rule 项由 `authorize_items` 按规则库重推 executable；manual 项由用户授权但服务端**重写全部自证字段**（risk=Yellow、rule_id=user-manual、durability=OneShot）并强制 `is_protected_path` 保护名单。
- **执行**：一律迁入隔离区（rename 同卷 / copy+delete 跨卷 EXDEV），可 restore；审计含 origin。
- **AI**：只产出 Explanation，数据流上不可达 PlanItem/executor。

## 3. UI 状态树（App.tsx）

```
App
├─ tab（clean|quarantine|rules|advanced）—— 四面板常驻挂载（display 切换，状态保活）
│   └─ 各面板收 active prop：激活时自动刷新（Quarantine/Rules；Clean 常驻无需要求）
├─ CleanPanel（常驻）
│   ├─ tasks[]（扫描任务列表，含 running/done/error；scan-progress 事件驱动）
│   ├─ selectedTaskId → selectedTask.result{summary, plan}
│   ├─ 筛选器（全部作用于 plan 的「可见视图」visiblePlan）：
│   │   ├─ noviceMode（折叠 regenerating 类）
│   │   ├─ minSize（≥10MB/100MB/1GB）
│   │   ├─ minAge（闲置 ≥7/30/90 天；无 age 证据条目被筛除）
│   │   └─ 可见视图 = plan ∩ 筛选；分页 PAGE_SIZE=15 仅切窗口不改集合
│   ├─ selected（Set<path>）—— 跨筛选残留允许，但执行/统计一律取 ∩可见视图
│   ├─ manualItems[]（user-manual，独立小表 + 大文件/大目录榜「加入计划」）
│   ├─ 执行 = (plan 项 ∩ 视图 ∩ 选中 ∩ executable) + manual 选中项
│   └─ tips（explain 结果缓存，key=path）
├─ QuarantinePanel{active}
│   ├─ items[]（list_quarantine；active 时自动刷新）
│   ├─ 单条：恢复 / 立即彻底删除（两段式）
│   └─ 批量：一键恢复全部 / 清空隔离区（两段式，逐条调用）
├─ RulesPanel{active}（list_rules 展示，无执行）
└─ AdvancedPanel（FDA 引导卡+check_fda 状态 / AI 设置+test_ai / Volume / Snapshots）
```

### 状态联动铁律（逻辑错误专项，2026-10-02）
1. **筛选即作用域**：全选/反选/执行/字节统计只看当前筛选视图，被筛掉的条目绝不参与。
2. **数据变了视图必须刷新**：tab 激活、扫描完成、apply 完成后，下游视图（隔离区列表等）自动重取，禁止依赖用户手动刷新。
3. **执行后回执必须醒目**：apply 的失败项（❌）在报告区大字展示——「文件夹依然存在」的历史根因就是跨卷失败被淹没在报告里。
4. **常驻面板的状态陈旧风险**：保活换来自由切换，代价是任何「加载一次」的面板必须改为 active 驱动刷新。

## 4. 安全不变量（SECURITY.md 为准，此处为索引）

- AI 永无删除权；授权 = 规则库 或 用户显式手选。
- red 永不执行；隔离区唯一删除路径；审计全量 origin 标注。
- 保护名单：系统前缀树 + HOME 顶层容器自身。
- 规则库可信输入：schema+lint+golden+CI 同一套校验。

## 5. 变更新功能时的检查单

- [ ] 本树哪个节点受影响？（数据流 / 状态归属 / 联动）
- [ ] 新交互是否满足 §3 铁律 1–4？
- [ ] 新后端路径是否有 authorize/audit/隔离语义？
- [ ] 新规则是否过 validate+golden+refs 核查？
- [ ] 穷举真机验证清单已列（每功能 ≥1 条真实操作路径）并全绿？
