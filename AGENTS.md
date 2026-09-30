# AGENTS.md — SlimIt 开发者/AI 代理须知

跨平台存储瘦身工具（Rust workspace + Tauri 2）。**完整现状、进度与工具链交接见 [docs/HANDOFF.md](./docs/HANDOFF.md)，接手前必读。**

## 不可妥协红线

1. AI 输出永不授权删除；执行授权只来自规则库。
2. `risk: red` 目标永不代删，只提示。
3. 删除一律进隔离区（14 天可恢复）；不可逆命令必须标 `irreversible` 并二次确认。
4. 未识别路径默认 `advise`（默认安全）。

## 常用命令

```bash
cargo test --workspace --all-targets      # 提交前全绿
cargo clippy --workspace                  # 0 warning
cargo fmt --all -- --check
python3 .agents/skills/slimit-rule-author/scripts/validate.py rules/   # 规则校验（CI 同款）
SLIMIT_UPDATE_GOLDEN=1 cargo test -p slimit-rules                      # 改规则后更新 golden
cargo tauri dev                           # GUI（tauri.conf.json cwd 已指向 ui/）
```

## 关键约定

- 新增/修改规则：先读 `rules/README.md` 与 `.agents/skills/slimit-rule-author/SKILL.md`。事实核查先行（refs 必填官方来源）、风险分级从紧、`command` 必带 `dry_run`、删除默认只清内容留目录、官方清理命令优先于裸删。
- 性能结论必须同日同树对照（见 docs/BENCH.md 方法学）。
- 凭据只走环境变量（范本 scripts/release/sign-notarize.sh）。
- 提交信息：中文 conventional（feat/fix/docs/chore: …）。

## 关键文档

[docs/HANDOFF.md](./docs/HANDOFF.md)（现状+工具链） · [docs/SPEC_MVP.md](./docs/SPEC_MVP.md)（规格） · [docs/BRIDGE.md](./docs/BRIDGE.md)（前后端契约） · [docs/W8-PLAN.md](./docs/W8-PLAN.md)（发布） · [docs/SECURITY.md](./docs/SECURITY.md)（安全不变量）
