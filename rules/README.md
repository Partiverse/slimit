# SlimIt 规则库

规则库是本产品的核心资产：每条规则描述一个"空间目标"的语义与安全处置方式。开源、可审计、社区可贡献。

## 结构

- `schema-v1.json` — 唯一契约（JSON Schema），CI 强制校验，改 schema 必须走 PR + 版本号递增（`slimit.rules/vN`）。
- `macos/`、`windows/`、`linux/` — 一文件一规则，文件名 = 规则 id。
- `_` 前缀文件（如 `_template.yaml`）不参与 schema 校验与打包。

## 编写规则（作者必读）

1. 复制 `macos/_template.yaml` 起稿。
2. `id` 全局唯一 kebab-case，带平台前缀（`macos-`/`win-`/`linux-`）。
3. `risk` 判级纪律：
   - `green`：内容可再生，删除无用户可感损失（缓存、构建产物、包管理器下载缓存）。
   - `yellow`：可删但有代价或需用户判断（旧备份、快照、旧内核、更新缓存）。
   - `red`：不可逆或高风险，**禁止代删**，只允许 `advise` 或 `irreversible` command；`red_flags` 必填。
4. `action.kind: command` 时 `dry_run` 必填——不能预览的命令不准入库。
5. 删除类动作默认 `delete_contents_only: true`（保留目录本身，防止应用失去目录句柄/权限）。
6. `refs` 至少一条官方文档或权威信源。
7. `paths` 模板语义：
   - 以 `~` 开头的路径按主目录展开（解析链 `HOME` → `USERPROFILE`，Windows 无 `HOME` 时仍可命中）。
   - 含 glob 元字符（`*`、`?`、`[`）的模板按 [globset](https://docs.rs/globset) 语义匹配——用于目标带随机后缀的场景（如 Firefox 的 `Profiles/<随机>/cache2`）；字面路径不受影响，仍做精确匹配。
   - **禁止使用 `%VAR%` 风格模板**（如 `%LOCALAPPDATA%`）。Windows 惯用环境变量写法，但规则库统一用 `~` 语义（`~` 展开为 `HOME` → `USERPROFILE`，与跨平台一致）。若需引用特殊目录，用 `~` 加相对路径（如 `~/AppData/Local` 对应 `%LOCALAPPDATA%`）。
8. 文案（semantics.*）用目标用户语言写清楚：这是什么、谁产生的、删了会怎样。

CI 会拒绝：重复 id、red 缺 red_flags、command 缺 dry_run、schema 不合、refs 为空。

## 执行语义速查

| risk | 执行器行为 |
|---|---|
| green | 可进"一键清理"，免费版有量限额 |
| yellow | 必须逐项勾选确认 |
| red | 永不执行；UI 只展示解释与官方路径 |
