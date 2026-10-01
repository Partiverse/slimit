# AGENTS.md — SlimIt 开发者/AI 代理须知

跨平台存储瘦身工具（Rust workspace + Tauri 2）。**完整现状、进度与工具链交接见 [docs/HANDOFF.md](./docs/HANDOFF.md)，接手前必读。**

## 不可妥协红线

1. AI 输出永不授权删除；执行授权只来自规则库。
2. `risk: red` 目标永不代删，只提示。
3. 删除一律进隔离区（14 天可恢复）；不可逆命令必须标 `irreversible` 并二次确认。
4. 未识别路径默认 `advise`（默认安全）。

## 协作习惯（用户偏好，所有 AI 开发环境直接遵循）

- 中文交流；技术术语、代码、命令、文件名保留原文。
- 回复结论先行、高信息密度，不铺垫、不复述任务；正文精简（电报体即可），交付持久产物（文档/代码/规则）用完整行文。
- 用户指令极简（「继续」「同意」「完成 X」是常态）：默认自主连续推进，不逐项确认；只有不可逆操作（删除 / push / 发布 / 覆盖未读文件）或真正的方向分叉才停下来问。
- 实质性回复末尾附「下一步建议」1–3 条，按推荐排序。

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

## 下一步工作（建议起点，详情见 docs/HANDOFF.md §7）

1. 干净机验收：按 docs/W8-PLAN.md §3 发布 checklist 在第二台 Mac 走完。
2. Windows/Linux 规则库持续扩充：51+34 条（首轮核查收口后），低置信条目与 refs 待逐条核查，按 rules/README.md 纪律执行。
3. 体验打磨：种子反馈「交互一般、语义模板化」的后续迭代。
4. 签名 + 公证（notarization）：用户决定暂不注册 Apple Developer Program（$99/年），v0.1 走未签名软启动；**公开发布/收费前必须补公证**。
5. v0.2 差异化：本地模型（~3B）AI 解释，保持「默认全离线」。
6. 开发者大额回收（v0.2 主打候选，优先级待用户定夺）：R1 项目感知规则已实装（docs/PROJECT-ARTIFACT-DESIGN.md），R3–R5 UI/解释/档位待做，背景见 docs/RECLAIM-STRATEGY.md。

## 关键文档

[docs/HANDOFF.md](./docs/HANDOFF.md)（现状+工具链） · [docs/SPEC_MVP.md](./docs/SPEC_MVP.md)（规格） · [docs/BRIDGE.md](./docs/BRIDGE.md)（前后端契约） · [docs/W8-PLAN.md](./docs/W8-PLAN.md)（发布） · [docs/SECURITY.md](./docs/SECURITY.md)（安全不变量）
