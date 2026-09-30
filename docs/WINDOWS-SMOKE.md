# Windows 端到端冒烟清单

> 目标：在真实 Windows 机器上验证规则库的加载、匹配、预览、执行全链路。
> 前提：SlimIt 已安装（`slimit install` 或手动放置二进制 + 规则目录）。

## 环境准备

- [ ] Windows 10 22H2 或 Windows 11 23H2（x64 / ARM64）
- [ ] 至少 10 GB 可用磁盘空间（规则会清理缓存，需有可清理目标）
- [ ] PowerShell 5.1+ 或 PowerShell 7+
- [ ] 已安装以下至少一个应用（用于产生缓存目标）：
  - [ ] Google Chrome
  - [ ] Microsoft Edge
  - [ ] Mozilla Firefox
  - [ ] Node.js + npm
  - [ ] Python + pip
  - [ ] pnpm
  - [ ] NuGet（Visual Studio 或 dotnet CLI）

## 冒烟步骤

### 1. 规则加载验证

```powershell
# 确认规则目录存在
Get-ChildItem "$env:APPDATA\slimit\rules" -Recurse -Filter "*.yaml" | Select-Object FullName

# 确认规则数量（预期 ≥ 7 条 Windows 规则）
(Get-ChildItem "$env:APPDATA\slimit\rules\windows" -Filter "*.yaml").Count
```

- [ ] 规则数量 ≥ 7（win-chrome-cache / win-edge-cache / win-firefox-cache / win-npm-cache / win-pip-cache / win-pnpm-store / win-nuget-packages）
- [ ] 无 YAML 解析错误（`slimit rules list` 应正常输出）

### 2. 路径匹配验证

```powershell
# 确认各缓存目录存在（至少部分存在）
$paths = @(
    "$env:LOCALAPPDATA\Google\Chrome\User Data\Default\Cache",
    "$env:LOCALAPPDATA\Microsoft\Edge\User Data\Default\Cache",
    "$env:LOCALAPPDATA\Mozilla\Firefox\Profiles\*\cache2",
    "$env:LOCALAPPDATA\npm-cache",
    "$env:LOCALAPPDATA\pip\Cache",
    "$env:LOCALAPPDATA\pnpm\store"
)
foreach ($p in $paths) {
    if (Test-Path $p) { Write-Host "EXISTS: $p" }
    else { Write-Host "MISSING: $p" }
}
```

- [ ] 至少 3 条规则的路径目录存在
- [ ] Firefox 的 `Profiles/*/cache2` glob 应命中实际存在的 profile 目录

### 3. 预览（dry-run）验证

对每条 green 规则执行 dry-run：

```powershell
# 示例：npm 缓存预览
# 预期：输出缓存路径与大小，不执行删除
# 实际命令取决于 CLI 设计，此处为概念性步骤
# slimit preview win-npm-cache
```

- [ ] 每条规则的 dry-run 输出包含路径与预估大小
- [ ] 无错误输出
- [ ] 实际文件未被修改（时间戳不变）

### 4. 执行验证（单条规则）

> ⚠️ 此步骤会实际删除缓存，请在非生产机器上执行。

```powershell
# 选择一条规则（建议 win-npm-cache，影响最小）
# slimit purge win-npm-cache
```

- [ ] 命令执行成功（exit code 0）
- [ ] 目标目录内容被清空（`delete_contents_only: true` 保留目录本身）
- [ ] 审计日志记录了删除事件（`~/.slimit/audit.log` 或等效位置）
- [ ] 应用功能不受影响（npm 仍可正常 install）

### 5. 隔离区验证（如适用）

```powershell
# 确认隔离区目录存在
Get-ChildItem "$env:APPDATA\slimit\quarantine" -ErrorAction SilentlyContinue

# 确认隔离区条目格式
# 每行应为：<path> <size> <unix-seconds> (unix-seconds)
```

- [ ] 隔离区目录存在且可写
- [ ] 条目格式符合 `"{secs} (unix-seconds)"` 规范
- [ ] 14 天后 `purge_expired` 可正确清理（需调整系统时间或等待）

### 6. 回退验证

```powershell
# 确认 HOME 未设置时，~ 展开回退到 USERPROFILE
# 在 PowerShell 中：
$env:HOME = $null
# 运行 slimit，确认 Windows 规则仍可命中
```

- [ ] 无 HOME 时，`~` 展开为 `%USERPROFILE%`
- [ ] Windows 规则正常匹配

## 通过标准

| 步骤 | 通过条件 |
|------|----------|
| 规则加载 | 规则数量 ≥ 7，无解析错误 |
| 路径匹配 | 至少 3 条规则路径存在，glob 命中正确 |
| 预览 | 每条规则 dry-run 输出正常，无文件修改 |
| 执行 | 单条规则删除成功，审计日志有记录 |
| 隔离区 | 目录存在，格式正确 |
| 回退 | 无 HOME 时仍可命中 Windows 规则 |

## 失败处理

| 现象 | 可能原因 | 排查方向 |
|------|----------|----------|
| 规则数量 < 7 | 规则目录未正确部署 | 检查 `slimit install` 输出，确认 `rules/windows/` 目录存在 |
| glob 未命中 | globset 在 Windows 上行为差异 | 检查路径分隔符（`/` vs `\`），确认 `*` 匹配 profile 目录 |
| dry-run 无输出 | 路径不存在或权限不足 | 检查目标目录是否存在，当前用户是否有读权限 |
| 执行失败 | 权限不足或文件被占用 | 关闭相关应用（Chrome/Firefox）后重试 |
| 审计日志无记录 | 日志路径未配置 | 检查 `~/.slimit/audit.log` 是否可写 |

## 自动化建议

长期可考虑：
1. GitHub Actions Windows runner 上跑冒烟（需处理应用安装）
2. 规则库 CI 增加 Windows 路径存在性检查（不执行删除）
3. 隔离区到期清理用 CI cron 模拟时间推进
