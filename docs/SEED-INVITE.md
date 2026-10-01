# 种子用户内测包（分发材料）

> 2026-10-01 更新至 rc3（体验打磨 / 安全加固 / 规则库 175 条，详见 Release Notes）。分发动作由用户执行；本文件是随附文案与操作清单。

## 分发物

| 物料 | 位置 |
|---|---|
| 安装包 | GitHub Release `v0.1.0-rc3`（pre-release）：https://github.com/Partiverse/slimit/releases/tag/v0.1.0-rc3 |
| SHA256 | `9360661cddf0fdf6a54eb73afbb06d4e38fbba0dfa2e47d38c4322d7c5219d5b` |
| 详细说明 | 仓库 `docs/BETA.md`（安装/已知限制/回报渠道） |
| 产品介绍 | https://slimit.pages.dev |

**分发前提**：种子用户需要能访问私有仓的 Release——两种方式任选：① 在仓库 Settings → Collaborators 添加对方 GitHub 账号（顺带可用 Issues 收反馈）；② 直接把 DMG 文件发给他（AirDrop/网盘），反馈走微信/邮件。

## 邀请文案（三句话，微信/邮件可直接发）

> 我在做一款 macOS 磁盘清理工具 SlimIt，特点是删任何东西之前先解释清楚「这是什么、删了会怎样」——规则库 + AI 双重解释，清理只做可一键恢复的隔离，绝不直接删。新包 rc3 刚出：界面大改版（分页签更好找了）、规则库扩到 175 条、又做了一轮安全加固，在：https://github.com/Partiverse/slimit/releases/tag/v0.1.0-rc3 （首次打开记得右键 → 打开）。想请你再试试：设置里可以填自己的 API Key 开启云端 AI 解释；「规则库」页签能直接看到它认识哪些目录，欢迎吐槽漏了什么。

**短版（群发/朋友圈）**：

> 自研的 macOS 瘦身工具开始内测：先解释后清理、隔离区可恢复、微信聊天数据这类危险目标只提醒不代删。求几位种子用户，会非常认真地听吐槽。

## 随附使用提示（对方拿到安装包后）

1. 挂载 DMG → 拖进「应用程序」→ **右键 SlimIt → 打开**（未签名应用必须右键开一次，之后正常双击）。
2. 建议先扫一个小目录找感觉：粘贴 `~/Library/Caches` 点「扫描并生成计划」。
3. 每个条目有「解释」按钮；green/yellow 可勾选执行（进隔离区），红色只提示不代删。
4. 「隔离区」面板可一键完整恢复任何一次清理。
5. 有问题请带截图反馈；如涉及清理异常，请附 `~/Library/Application Support/dev.partiverse.slimit/audit/audit.jsonl` 尾部几行。

## 用户跟踪建议

- 记录每位种子用户的 macOS 版本（本包仅在 macOS 26 / Apple Silicon 实测过）。
- 三个关键问题：解释是否可信？是否敢点执行？有没有想清却清不了的目录？
- 反馈回流后优先级：正确性 > 体验 > 新规则需求（新规则走 golden 流程）。
