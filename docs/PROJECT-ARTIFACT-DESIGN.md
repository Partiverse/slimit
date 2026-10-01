# 项目感知规则设计：schema v1 的 project-artifact 扩展（R1+R2）

> 目标：让 SlimIt 能命中**任意项目目录内的构建产物**（`target/`、`node_modules`、`.venv`…）——现状 175 条规则全部锚定 `~` 固定路径，用户 80GB 的 Rust `target/` 恰好落在盲区（背景与动机见 [RECLAIM-STRATEGY.md](./RECLAIM-STRATEGY.md) §6）。
> 日期：2026-10-01 · 状态：设计定稿，本文同批提交试点实现（R1 全量 + R2 降级为后续优化，理由见 §3）。

## 1. 现状与能力边界

- 规则匹配的输入是 `ScanResult.dirs` 的目录快照（`DirSnapshot{path, apparent, actual}`），命中方式两种：`~` 展开后精确匹配、globset 模板匹配随机后缀（如 Firefox profile）。
- glob 元字符只能表达「固定层级 + 随机段」，**表达不了任意深度**（`**` 语义），更表达不了「存在性条件」（旁边有 `Cargo.toml` 才算 Rust 项目）。
- 结论：项目内构建产物需要新的规则形态与匹配逻辑，不是加规则文件能解决的。

## 2. 方案总览与关键决策

| # | 决策 | 内容与理由 |
|---|---|---|
| D1 | **不 bump apiVersion** | `project` 是 v1 的可选扩展字段，175 条既有规则零影响；bump v2 只会制造 175 个文件的噪音 diff。schema `$id` 与文件名不变，`description` 注明扩展。 |
| D2 | **`paths` 与 `project` 互斥** | project 规则必须 `paths: []`，schema 用 `allOf if/then` 强制，避免两种命中语义混在一条规则里。 |
| D3 | **pilot 在匹配层实现，不改 walker** | 扫描器已经全量枚举目录（含项目内），匹配层对「目录名 ∈ rel_paths」的少数候选做一次 marker 存在性检查（lstat）即可命中。walker 短路统计（原 R2）降级为后续性能优化：它破坏与 `du` 对照的可比性，且聚合层已能处理，等 bench 证明收益再做（见 §8）。 |
| D4 | **年龄证据一等公民** | `max_age_days` + 命中时实测 `age_days`。口径用目录 mtime（构建产物目录的 mtime = 最近一次构建活动；APFS `atime` 不可靠不用）。 |
| D5 | **低于年龄阈值 ⇒ 不可执行** | `below_min_age=true` 的命中 `executable=false`（服务端授权与 plan 同逻辑）。跨天边界只会让可执行变不可执行——安全方向单调。社区启发式：node_modules 30 天、构建产物 14 天未动才建议清。 |
| D6 | **授权重放一致性** | `authorize_items` 对 IPC 传回的 PlanItem 重放 `match_rules` 重推 executable。project 规则的重放在删除前执行（marker 与 age 均在盘上），与首次匹配一致；不一致的失败模式是「降级不可执行」，安全方向。 |
| D7 | **动作类型限 purge-dir / advise** | 项目产物清理无官方 command 可言（`cargo clean` 只作用于 cwd 单项目），command 类型对 project 规则无意义且危险，lint 禁止。 |

## 3. Schema 扩展（向后兼容）

`rules/schema-v1.json` 新增可选属性 `project`：

```json
"project": {
  "type": "object",
  "additionalProperties": false,
  "required": ["markers", "rel_paths"],
  "properties": {
    "markers":   { "type": "array", "minItems": 1, "items": { "type": "string", "pattern": "^[^/\\\\*?\\[<>|:\"]+$" },
                   "description": "项目根标记文件（单段文件名），如 Cargo.toml" },
    "rel_paths": { "type": "array", "minItems": 1, "items": { "type": "string", "pattern": "^[^/\\\\*?\\[<>|:\"]+$" },
                   "description": "项目根下相对目录名（单段），如 target" },
    "max_age_days": { "type": "integer", "minimum": 1, "description": "低于该天数只提示不执行（below_min_age）" }
  }
}
```

配套 `allOf` 约束：

```json
{ "if": { "required": ["project"] },
  "then": { "properties": { "paths": { "maxItems": 0 } } },
  "else": { "properties": { "paths": { "minItems": 1 } } } }
```

互斥规则两侧的字段模式（marker 文件名 / 目录名单段、禁 glob 与路径分隔符）是**路径注入防线**：rel_paths 只允许匹配目录的 `file_name()`，规则无法把命中锚到项目根之外。

## 4. 数据结构与匹配算法

**loader.rs**：`Rule` 增加 `#[serde(default)] pub project: Option<ProjectCfg>`；`ProjectCfg { markers: Vec<String>, rel_paths: Vec<String>, max_age_days: Option<u32> }`（`deny_unknown_fields` 与全局一致）。

**matcher.rs**：`Match` 增加 `#[serde(default)] pub age_days: Option<u64>` 与 `#[serde(default)] pub below_min_age: bool`（旧路径规则恒为 `None/false`，序列化对旧前端透明）。

```
match_rules 内，对带 project 的规则：
for dir in snapshot:
    if dir.path.file_name() ∈ rule.project.rel_paths:
        project_root = dir.path.parent()
        if project_root 存在任一 markers 文件:            # 一次 lstat/metadata
            age_days = 目录 mtime 距今天数                # 一次 metadata
            below_min_age = max_age_days.map_or(false, |m| age_days < m)
            push Match{...}
```

- FS 访问只发生在「目录名命中候选」上（全库扫描下通常个位数到几十个），成本可忽略；matcher 由纯函数变为「带受控 lstat 的函数」，在模块注释中说明。
- `~/Library` 下与项目无关的同名目录（如手工建的 `~/target`）因父目录无 marker 天然不命中——存在性检查本身就是降噪。

**plan.rs**：`PlanItem` 增加 `age_days` / `below_min_age` 透传；`executable` 推导追加一条红线：

```
(Green|Yellow, PurgeDir) => !m.below_min_age      # 其余不变：Red/advise/command 恒 false
```

`authorize_items` 重放同逻辑（复用 `plan_from_snapshots`，自动一致）。

## 5. lint 约束（loader lint.rs 与 validate.py 同步实现）

project 规则额外校验：
1. `paths` 必须为空（schema 已拦，lint 双保险）；
2. `markers` / `rel_paths` 非空且每项为单段名（无 `/` `\` glob 元字符）；
3. `action.kind` ∈ {purge-dir, advise}；
4. `max_age_days` ≥ 1；
5. `risk: red` 的 project 规则只允许 advise（与全局 red 纪律一致）。

## 6. 试点规则（macos 先行——walker 唯一实装平台）

| id | markers | rel_paths | max_age_days | risk | 语义要点 |
|---|---|---|---|---|---|
| `macos-project-cargo-target` | Cargo.toml | target | 14 | yellow | 「代码不丢，下次 `cargo build` 重新编译（首次变慢几分钟）」；debug+release 分目录随工具链累积，单项目常见 5–15GB |
| `macos-project-node-modules` | package.json, package-lock.json | node_modules | 30 | yellow | 「`npm install` 可完全重建（需重新下载依赖）」；30 天社区启发式 |
| `macos-project-python-venv` | pyproject.toml, requirements.txt, setup.py | .venv, venv | 30 | yellow | 「`pip install`/`uv sync` 可重建」；符号链接与 bin 脚本硬编码绝对路径，重建即修复 |

风险判定从紧：三者的重建成本（时间/流量）用户可感知，按纪律归 yellow（green 留给零感知缓存）；refs 引 cargo/npm/pip 官方文档。

## 7. 测试计划

1. loader：project 规则解析、未知字段拒绝；
2. lint：§5 每条约束的失败用例；
3. matcher（tempdir）：命中（marker+目录名齐备）、无 marker 不命中、年龄过滤、同名无 marker 目录不命中；
4. plan：below_min_age ⇒ executable=false；authorize_items 重放幂等与降级；
5. golden 更新（`SLIMIT_UPDATE_GOLDEN=1`）+ validate.py 175+3 全绿 + workspace 全测试。

## 8. 后续路线（本设计不含）

- **R2'（性能）**：walker 对「marker 命中的 rel_path 目录」短路统计（不逐文件下钻）。前提：bench 证明收益 + 接受与 `du` 对照口径变化。当前全量遍历 ~1M 折算 10–18s，暂无必要。
- **R3（UI）**：Plan 列表按净收益排序；`below_min_age` 项默认不勾选并显示年龄证据（「97 天未动」）；A 类「重建成本」徽章。
- **R4（解释）**：explain 对 project 命中返回含年龄证据的语义模板（现有 云端→规则库→启发式 链）。
- **R5（档位）**：新手档只展示 A 类高置信；专家档放开 B 类与 command 规则。
- **跨平台**：Windows/Linux walker 落地后，三条试点规则镜像为 win-/linux- 前缀（marker 机制平台无关）。
