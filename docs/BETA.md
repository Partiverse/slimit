# SlimIt 内测分发（W7）

状态：**未签名构建**（notarization 在 W8）。本页说明内测包的产出与安装方式。

## 产出物

`cargo tauri build` 产出：

- `target/release/bundle/macos/SlimIt.app` — 主程序（arm64，macOS 26 实测）
- `target/release/bundle/dmg/SlimIt_0.1.0_aarch64.dmg` — 分发镜像（hdiutil 生成，无装饰布局）

> 备注：tauri 自带 bundle_dmg.sh 依赖 Finder AppleScript 定位图标，偶发 -10006
> （自动化授权缺失）导致 DMG 步骤失败；失败时可 `hdiutil create -volname SlimIt
> -srcfolder target/release/bundle/macos/SlimIt.app -ov -format UDZO <输出>.dmg`
> 直接生成。W8 notarization 阶段一并处理签名与 DMG 美化。

## 安装指引（未签名应用的 Gatekeeper 通行方式）

内测包没有 Developer ID 签名，首次打开 macOS 会拦截，两种通行方式：

**方式一（推荐）：右键打开**

1. 挂载 DMG，把 SlimIt 拖入 Applications（或直接使用 .app）。
2. 在 Finder 中**右键点击 SlimIt → 打开**，再点弹窗中的「打开」。
3. 此后系统记住许可，以后双击正常启动。

**方式二：系统设置放行**

若双击后提示"无法打开，因为无法验证开发者"：
系统设置 → 隐私与安全性 → 底部「仍要打开」按钮。

**彻底移除隔离属性（备用）**：

```sh
xattr -dr com.apple.quarantine /Applications/SlimIt.app
```

## 内测范围与红线

- 现有功能：扫描（进度事件流）→ 规则计划 → 隔离执行（可完整恢复）→ AI 语义解释。
- 规则库 81 条编译期嵌入；隔离区位于 `~/Library/Application Support/dev.partiverse.slimit/quarantine/`，审计日志同目录 `audit/audit.jsonl`。
- **AI 解释仅为提示层，永不影响执行授权**；red 目标永不代删；所有清理动作只做"迁入隔离区"。
- 已知限制：command 类规则（brew cleanup、conda clean 等）不在应用内自动执行，仅显示官方命令供手动运行；跨卷目标（EXDEV）暂不支持清理。

## 回报渠道

问题请记录到仓库 issue 或直接反馈：复现步骤 + 「隔离区」面板截图 + `audit/audit.jsonl` 尾部几行。
