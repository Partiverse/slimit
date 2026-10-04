# Slimit 内测分发（W7）

状态：**未签名构建**（notarization 在 W8）。本页说明内测包的产出与安装方式。

## 产出物

`cargo tauri build` 产出：

- `target/release/bundle/macos/Slimit.app` — 主程序（arm64，macOS 26 实测）
- `target/release/bundle/dmg/Slimit_0.1.0_aarch64.dmg` — 分发镜像（hdiutil 生成，无装饰布局）

> 备注：tauri 自带 bundle_dmg.sh 依赖 Finder AppleScript 定位图标，偶发 -10006
> （自动化授权缺失）导致 DMG 步骤失败；失败时可 `hdiutil create -volname Slimit
> -srcfolder target/release/bundle/macos/Slimit.app -ov -format UDZO <输出>.dmg`
> 直接生成。W8 notarization 阶段一并处理签名与 DMG 美化。

## 安装指引（未签名应用的 Gatekeeper 通行方式）

内测包没有 Developer ID 签名，首次打开 macOS 会拦截。**种子用户实测：部分系统版本上「右键打开」无效**——如果方式一/二都失败，直接用方式三（一条命令，必成功）：

**方式三（最可靠）：终端移除隔离属性**

```sh
xattr -dr com.apple.quarantine /Applications/Slimit.app
```

执行后即可正常双击打开（需先把 Slimit 拖入 Applications；路径不同请对应修改）。

**方式一：右键打开**

1. 挂载 DMG，把 Slimit 拖入 Applications（或直接使用 .app）。
2. 在 Finder 中**右键点击 Slimit → 打开**，再点弹窗中的「打开」。
3. 此后系统记住许可，以后双击正常启动。
   （注：部分系统版本此方式可能无效，请改用方式三。）

**方式二：系统设置放行**

若双击后提示"无法打开，因为无法验证开发者"：
系统设置 → 隐私与安全性 → 底部「仍要打开」按钮。

## 内测范围与红线

- 现有功能：扫描（进度事件流）→ 规则计划 → 隔离执行（可完整恢复）→ AI 语义解释。
- 规则库 81 条编译期嵌入；隔离区位于 `~/Library/Application Support/dev.partiverse.slimit/quarantine/`，审计日志同目录 `audit/audit.jsonl`。
- **AI 解释仅为提示层，永不影响执行授权**；red 目标永不代删；所有清理动作只做"迁入隔离区"。
- 已知限制：command 类规则（brew cleanup、conda clean 等）不在应用内自动执行，仅显示官方命令供手动运行；跨卷目标（EXDEV）暂不支持清理。

## 常见疑问（FAQ）

**Q1：黄色「注意」的条目能隔离吗？**
能。yellow = 可清理但有代价（比如下次构建要重新编译），正常勾选执行即可。你看到的**灰色不可勾选**黄色条目是另一种情况：它闲置时间太短（如 target/ 不足 14 天、node_modules 不足 30 天），说明还在活跃使用中，等它闲置够了会自动变为可勾选——这与风险等级无关。

**Q2：「隔离」是什么意思？**
不是删除。执行清理时，文件/文件夹被**原样搬进一个隔离区文件夹**（同盘秒搬、不复制内容），在「隔离区」页签随时可一键**完整恢复**。超过 14 天未恢复的条目才会被自动清掉（清前应用会再确认一次）。所以误删的窗口期是 14 天。

**Q3：隔离后，原来正在使用这些文件的程序会怎样？**
按路径访问会立刻找不到文件（这正是清理的目的——腾出空间）。已经打开的文件句柄仍能继续读写直到关闭，所以**正在运行中的应用一般不会崩**，但建议清理前退出相关应用（如清 JetBrains 缓存前先关 IDE，规则说明里会写）。隔离区一键恢复后一切照旧。

**Q4：有实时监控吗？**
v0.1 刻意不做（技术决策见 docs/COMPLETION-REPORT.md §2：macOS 实时归因需要系统特权框架、Windows 需管理员权限，门槛和隐私代价都太高）。Slimit 是**按需扫描**模式：你想清的时候扫一次。定时自动扫描在路线图里评估中。

**Q5：为什么有些空文件夹扫描不到？**
扫描结果中的文件夹由「其下有文件」推导而来：完全空的目录（以及整条链上都没有文件的目录）不会出现在列表里——它们不占空间，清理也没有意义。此为设计语义，见 docs/HANDOFF.md。

## 回报渠道

问题请记录到仓库 issue 或直接反馈：复现步骤 + 「隔离区」面板截图 + `audit/audit.jsonl` 尾部几行。
