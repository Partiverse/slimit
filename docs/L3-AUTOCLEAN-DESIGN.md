# L3 设计稿：green 预授权自动清理（无感化第三档）

> 状态：**设计稿，未实装**。实装前置条件：SECURITY.md「设计级安全不变量」增补第 8 条（预授权）并经用户确认。
> 背景：无感化 L1（定时扫描+通知）/ L2（自动带出结果）已实装；L3 回答「能不能连确认都省了」。
> 日期：2026-10-03

## 1. 语义定义（红线评估）

**授权主体是用户的预先 opt-in，不是 AI 决策**——这是 L3 与「AI 自动清理」的本质区别：

- 用户在设置页显式开启「自动清理可再生缓存」，即对**当时规则库内全部 `risk=green` 且 `action=purge-dir` 的规则**做了预先授权；
- 授权范围**封闭且可审计**：只认规则库 green+purge-dir，规则库本身可读、可挑战（规则库页签全量可见）；
- 规则库更新引入新 green 规则时，自动清理范围随之扩大——因此设置页开关文案必须写明「规则库新增的可再生缓存条目也将自动清理」，并在每次自动清理通知中列出命中的规则 id。

**不变量（全部保持）**：AI 永无删除权（L3 由 LaunchAgent 定时任务驱动，AI 不参与）；red 永不执行；一切删除仍走隔离区（14 天可恢复）；审计 origin=`auto-schedule`；yellow/red/project/user-manual 项**永不自动执行**。

## 2. 触发与执行流

```
LaunchAgent（每周六 10:00，复用 L1 任务）
  → --scheduled-scan --auto-clean
  → scan + plan
  → 筛选：origin=rule ∧ risk=green ∧ action=purge-dir ∧ !below_min_age
  → apply()（照常隔离区 + 审计，origin=auto-schedule）
  → 系统通知：「已自动清理 N 项可再生缓存（X GiB），14 天内可在隔离区恢复」
  → yellow/red/project/manual 命中项：只进通知摘要（「另有 M 项需您确认」），不执行
```

- **范围严格性**：L3 自动执行的集合是 plan 的**子集**，由既有 `authorize_items` 推导后再过滤 green——不新写授权逻辑，只在授权结果上做减法。
- **执行前置条件**：扫描根所在卷可写、非保护路径（沿用 is_protected_path）。

## 3. 配置与 UI

- 设置页「定时扫描」分区新增开关：「同时自动清理可再生缓存（绿色条目，仍可 14 天内恢复）」，默认 **关闭**；
- 开启前置条件：定时扫描已开启（L3 依附于 L1 触发器）；
- 通知文案区分两档：自动清理了什么（可恢复）+ 哪些需要用户确认。

## 4. 实施计划（确认后执行）

1. SECURITY.md 不变量第 8 条（预授权）定稿；
2. `schedule.json` 增加 `auto_clean: bool`（默认 false）；
3. `--scheduled-scan` 增加 `--auto-clean` 传递；无头模式内：plan 结果过滤 green+purge-dir → `apply()`（复用 authorize + audit）→ 通知分两段（已自动清理 / 待确认）；
4. 设置页开关 + 文案；
5. 测试：单元（过滤逻辑）+ 真机（opt-in 后 kickstart，验证 green 项被隔离、yellow/project 不动、审计含 origin=auto-schedule、通知两段文案）。

## 5. 风险与开放问题

- **误伤面**：green 规则的判定依赖规则库质量（homebrew-cleanup 等命令类规则不参与；purge 类 green 规则均为下载/编译缓存）。隔离区 + 14 天是兜底。
- **频率**：每周一次 + 仅 green，最坏情况是「清了用户本来也想清的东西」。
- **开放问题**：是否给 green 自动清理设单次字节上限（如 >20GiB 时降级为通知）？倾向不做（复杂度 > 收益），依赖 14 天恢复窗口兜底。
