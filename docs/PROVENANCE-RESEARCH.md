# 文件来源追踪（File Provenance）深度调研

> 调研目标：从源头上跟踪文件数据产生的来源，实现"安装 QQ 后，slimit 能监控到它产生的所有数据所在的文件目录和部分不在标准文件夹内的文件"。

## 1. 问题陈述

### 1.1 当前方案的局限

SlimIt 当前采用**规则库匹配**方案：
- 规则库：90 条 macOS 规则，路径模式如 `~/Library/Caches/Homebrew`
- 匹配方式：扫描器遍历目录，规则库 glob 匹配
- 局限：**只能匹配已知模式**，无法发现"软件卸载后藏在系统文件夹的残留数据"

### 1.2 用户诉求

> 当我安装 QQ 时，slimit 就可以监控到它产生的所有数据所在的文件目录和部分不在标准文件夹内的文件。当想要瘦身或删除 QQ 时，slimit 可以真正意义上完整执行了用户的诉求。

### 1.3 核心挑战

| 挑战 | 说明 |
|---|---|
| **动态性** | 软件安装时产生的文件路径无法预先知道 |
| **隐蔽性** | 某些软件将数据藏在非标准位置（如 `/Library/Application Support/` 的子目录） |
| **跨平台** | macOS / Linux / Windows 各有不同的文件系统机制 |
| **隐私** | 监控文件系统可能涉及用户隐私数据 |

---

## 2. 技术调研

### 2.1 macOS 方案

#### 2.1.1 Endpoint Security Framework（推荐）

**来源**：Apple 官方文档、WWDC2020、Objective-See 博客

**核心能力**：
- 用户态 API，无需内核扩展
- 可监控文件创建、写入、重命名、删除等事件
- 提供**进程信息**：pid、路径、代码签名信息
- 支持**授权事件**：可拦截并允许/拒绝操作

**关键事件类型**：
```
ES_EVENT_TYPE_NOTIFY_CREATE   // 文件创建
ES_EVENT_TYPE_NOTIFY_WRITE    // 文件写入
ES_EVENT_TYPE_NOTIFY_RENAME   // 文件重命名
ES_EVENT_TYPE_NOTIFY_DELETE   // 文件删除
ES_EVENT_TYPE_NOTIFY_OPEN     // 文件打开
```

**代码签名信息**：
```c
es_process_t *process;
process->executable->path;      // 可执行文件路径
process->signing_id;            // 签名标识（如 com.apple.Safari）
process->team_id;               // 团队标识
process->cdhash;                // 代码目录哈希
```

**Entitlement 要求**：
- `com.apple.developer.endpoint-security.client`（需向 Apple 申请）
- 测试阶段可禁用 SIP 临时使用

**实现难度**：⭐⭐⭐⭐⭐（官方支持，文档完善）

**参考实现**：
- Objective-See 的 `FileMonitor`：https://objective-see.org/blog/blog_0x48.html
- Apple 官方 `eslogger`：macOS 自带工具
- Mac Monitor（RedCanary）：https://github.com/redcanaryco/mac-monitor

#### 2.1.2 Provenance Sandbox（macOS Ventura+）

**来源**：Black Hat USA 2025（FFRI/ShowProvenanceInfo）

**核心能力**：
- macOS Ventura 引入的安全机制
- 为进程分配 `com.apple.provenance` 扩展属性
- 进程创建/修改的文件自动标记 `com.apple.provenance`

**限制**：
- 需要进程在 Provenance Sandbox 中运行
- 不是所有应用都支持

**参考实现**：https://github.com/FFRI/ShowProvenanceInfo

#### 2.1.3 文件来源属性（扩展属性）

**来源**：file-observer RFC、FileGrail

**现有机制**：
| 属性 | 说明 | 平台 |
|---|---|---|
| `com.apple.quarantine` | 下载标记（含来源 URL、代理应用） | macOS |
| `kMDItemWhereFroms` | Spotlight 属性，文件来源 URL | macOS |
| `com.apple.provenance` | 进程来源追踪 | macOS Ventura+ |

**局限性**：
- 仅对"从网络下载"的文件有效
- 不追踪本地应用产生的文件

#### 2.1.4 审计日志（OpenBSM）

**来源**：Apple 已废弃，推荐迁移到 Endpoint Security

**状态**：已废弃，不推荐新项目使用

---

### 2.2 Linux 方案

#### 2.2.1 eBPF（推荐）

**来源**：Datadog 博客、Elastic 博客、Isovalent 博客

**核心能力**：
- 内核级监控，性能极高
- 可获取**进程信息**：pid、uid、命令名
- 可获取**文件路径**
- 支持**CO-RE**（Compile Once - Run Everywhere）

**关键 Hook 点**：
```c
// VFS 函数
SEC("kprobe/vfs_create")
SEC("kprobe/vfs_mkdir")
SEC("kprobe/vfs_write")
SEC("kprobe/vfs_unlink")
SEC("kprobe/vfs_rename")

// 系统调用追踪点
TRACEPOINT(sys_enter_open)
TRACEPOINT(sys_enter_openat)
```

**优势**：
- 性能极高，可处理 100 亿+ 事件/分钟
- 可在内核中预过滤，减少用户态开销
- 无需 root（需 `CAP_BPF` + `CAP_PERFMON`）

**劣势**：
- 需要 Linux 5.8+（BTF 支持）
- 内核 ABI 不稳定，可能随版本变化
- 开发复杂度高

**参考实现**：
- Polka Engine：https://github.com/notslok/polka
- fs-watcher：https://github.com/amandeepsp/fs-watcher
- fim-ebpf：https://github.com/harshavmb/fim-ebpf
- Tetragon（Isovalent）：https://github.com/cilium/tetragon

#### 2.2.2 fanotify

**来源**：Linux man-pages、kernel-internals.org

**核心能力**：
- 监控整个挂载点
- 可获取文件描述符
- 支持**权限事件**（可拦截访问）

**关键特性**：
```c
// 监控整个文件系统
fanotify_mark(fd, FAN_MARK_ADD | FAN_MARK_FILESYSTEM,
              FAN_CREATE | FAN_DELETE | FAN_RENAME,
              AT_FDCWD, "/");

// 获取进程信息
struct fanotify_event_metadata *m;
m->pid;  // 进程 ID
```

**限制**：
- 需要 `CAP_SYS_ADMIN`（root 权限）
- Linux 5.1+ 才支持 create/delete/move 事件
- 不递归监控子目录（需手动添加 mark）

**参考实现**：filemon：https://github.com/mathscantor/filemon

#### 2.2.3 inotify

**来源**：Linux man-pages

**核心能力**：
- 简单易用
- 无需 root 权限

**限制**：
- 不提供**进程信息**（不知道谁创建了文件）
- 不递归监控子目录
- 事件队列可能溢出

**适用场景**：轻量级监控，不需要进程信息

---

### 2.3 Windows 方案

#### 2.3.1 ETW（Event Tracing for Windows）

**来源**：Microsoft 官方文档

**核心能力**：
- 内核级事件追踪
- 可监控文件 I/O 事件
- 可获取进程信息

**关键 Provider**：
```
FileIoGuid = {0x90cbdc39, 0x4a3e, 0x11d1, {0x84, 0xf4, 0x00, 0x00, 0xf8, 0x04, 0x64, 0xe3}}
```

**事件类型**：
```c
PERFINFO_LOG_TYPE_FILE_IO_CREATE  // 文件创建
PERFINFO_LOG_TYPE_FILE_IO_READ    // 文件读取
PERFINFO_LOG_TYPE_FILE_IO_WRITE   // 文件写入
```

**限制**：
- 需要管理员权限
- 事件数据量大，需过滤

**参考实现**：https://gist.github.com/pa-0/6f51fd19a4742a81cc438321e0a4e0ef

#### 2.3.2 文件系统微过滤器（Minifilter Driver）

**来源**：Microsoft 官方文档、HeathenEDR

**核心能力**：
- 内核级文件系统驱动
- 可监控所有文件操作
- 可拦截并修改操作

**关键回调**：
```c
// 预操作回调
FLT_PREOP_CALLBACK_STATUS
PreOperationCallback(
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _Out_ PVOID *CompletionContext
);
```

**限制**：
- 需要 Windows Driver Kit (WDK)
- 需要数字签名
- 开发复杂度极高

**参考实现**：
- HeathenEDR：https://github.com/Heathen-Software/HeathenEDR
- MinifilterMonitor：https://github.com/belazr/MinifilterMonitor

---

## 3. 技术可行性分析

### 3.1 各平台方案对比

| 平台 | 推荐方案 | 权限要求 | 开发难度 | 性能 | 进程信息 |
|---|---|---|---|---|---|
| **macOS** | Endpoint Security | Full Disk Access + Entitlement | ⭐⭐⭐⭐ | 高 | ✅ 完整 |
| **macOS** | Provenance Sandbox | 无 | ⭐⭐⭐ | 中 | ✅ 自动 |
| **Linux** | eBPF | CAP_BPF + CAP_PERFMON | ⭐⭐⭐⭐⭐ | 极高 | ✅ 完整 |
| **Linux** | fanotify | CAP_SYS_ADMIN | ⭐⭐⭐ | 高 | ✅ 有 |
| **Linux** | inotify | 无 | ⭐⭐ | 中 | ❌ 无 |
| **Windows** | ETW | 管理员 | ⭐⭐⭐ | 高 | ✅ 有 |
| **Windows** | Minifilter | 驱动签名 | ⭐⭐⭐⭐⭐ | 极高 | ✅ 完整 |

### 3.2 关键问题：Entitlement 获取

**macOS Endpoint Security 的 `com.apple.developer.endpoint-security.client` 需要向 Apple 申请。**

**申请流程**：
1. 注册 Apple Developer Program（$99/年）
2. 通过 Apple Developer 门户提交申请
3. 说明用途（安全/监控/企业内部分发）
4. 等待审核（可能数周）

**实际案例**：
- 开源项目：有开发者反馈"Suck it and see"，但成功率不确定
- 企业项目：需提供企业级用途说明
- 个人项目：极难获得

**替代方案**：
1. **eslogger**：macOS 自带工具，无需 entitlement
2. **Provenance Sandbox**：macOS Ventura+，无需 entitlement
3. **审计日志**：已废弃，不推荐

---

## 4. 隐私规避策略

### 4.1 核心原则

> 文件来源追踪涉及用户隐私，必须设计隐私规避策略。

### 4.2 策略设计

#### 4.2.1 本地优先（Local-First）

**原则**：所有数据在本地处理，不上传云端。

**实现**：
- 监控数据存储在本地数据库（SQLite）
- 不向任何外部服务发送文件路径或内容
- 用户可完全控制数据

#### 4.2.2 数据最小化

**原则**：只收集必要信息，不收集文件内容。

**收集的数据**：
| 数据项 | 是否收集 | 说明 |
|---|---|---|
| 文件路径 | ✅ | 用于匹配规则 |
| 文件大小 | ✅ | 用于计算回收空间 |
| 进程信息 | ✅ | 用于追踪来源 |
| 文件内容 | ❌ | 不收集 |
| 文件哈希 | ⚠️ 可选 | 用于去重，但可能泄露信息 |

#### 4.2.3 用户控制

**原则**：用户可随时查看、导出、删除监控数据。

**实现**：
- 提供"查看监控数据"功能
- 提供"导出报告"功能
- 提供"清除所有数据"功能

#### 4.2.4 透明性

**原则**：向用户明确说明监控什么、不监控什么。

**实现**：
- 在隐私政策中说明
- 在 UI 中显示监控状态
- 提供"停止监控"按钮

### 4.3 隐私设计参考

**参考项目**：
- **Privacy-Agent**：https://github.com/jlynshue/local-private-orchestration
  - 本地优先，PII 检测，审计链
- **Sovereign File Tracker**：https://github.com/ProjectPAIE/sovereign-file-tracker
  - 本地存储，版本控制，知识图谱

---

## 5. 与现有方案组合

### 5.1 组合架构

```
┌─────────────────────────────────────────────────────────────┐
│                     SlimIt 架构                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐      │
│  │   规则库     │    │  来源追踪    │    │   扫描器     │      │
│  │  (静态匹配)  │    │  (动态监控)  │    │  (目录遍历)  │      │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘      │
│         │                  │                  │              │
│         └──────────────────┼──────────────────┘              │
│                            ▼                                 │
│                    ┌─────────────┐                          │
│                    │   计划生成   │                          │
│                    │  (规则+来源) │                          │
│                    └──────┬──────┘                          │
│                            ▼                                 │
│                    ┌─────────────┐                          │
│                    │   隔离区     │                          │
│                    └─────────────┘                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 工作流程

#### 5.2.1 安装监控模式

```
1. 用户启动"安装监控"
2. slimit 通过 Endpoint Security/eBPF 开始监控文件创建事件
3. 用户安装软件（如 QQ）
4. slimit 记录所有被该软件创建/修改的文件
5. 用户停止监控
6. slimit 生成"软件数据报告"，显示：
   - 创建的文件列表
   - 占用的磁盘空间
   - 文件位置（标准/非标准）
```

#### 5.2.2 清理模式

```
1. 用户选择要清理的软件（如 QQ）
2. slimit 显示该软件产生的所有文件
3. 用户确认清理
4. slimit 将文件迁入隔离区
5. 生成清理报告
```

### 5.3 数据模型

```rust
/// 文件来源记录
struct ProvenanceRecord {
    id: Uuid,
    file_path: PathBuf,
    file_size: u64,
    created_at: DateTime<Utc>,
    
    /// 创建该文件的进程
    creator: ProcessInfo,
    
    /// 关联的软件（通过进程签名识别）
    software: Option<SoftwareInfo>,
    
    /// 监控会话 ID
    session_id: Uuid,
}

/// 进程信息
struct ProcessInfo {
    pid: u32,
    path: PathBuf,
    signing_id: Option<String>,  // 如 com.tencent.qq
    team_id: Option<String>,
}

/// 软件信息
struct SoftwareInfo {
    name: String,
    signing_id: String,
    version: String,
}
```

### 5.4 与规则库的协同

| 场景 | 规则库 | 来源追踪 |
|---|---|---|
| 已知软件缓存 | ✅ 直接匹配 | - |
| 未知软件数据 | - | ✅ 动态发现 |
| 系统级文件 | ⚠️ 需添加规则 | ✅ 自动识别 |
| 用户自定义路径 | - | ✅ 自动记录 |

---

## 6. 实现建议

### 6.1 分阶段实施

#### 阶段 1：基础监控（MVP）

**目标**：实现基本的文件创建监控

**平台优先级**：
1. **macOS**：Endpoint Security（用户基数最大）
2. **Linux**：eBPF（技术成熟）
3. **Windows**：ETW（相对简单）

**功能**：
- 监控文件创建事件
- 记录进程信息
- 生成软件数据报告

#### 阶段 2：智能关联

**目标**：自动识别软件与文件的关联

**功能**：
- 进程签名识别（如 com.tencent.qq）
- 软件分组（同一软件的多进程）
- 数据分类（缓存/日志/配置）

#### 阶段 3：预测推荐

**目标**：基于历史数据预测可清理项

**功能**：
- 软件卸载后残留数据识别
- 长期未访问文件推荐
- 智能清理建议

### 6.2 技术选型

| 组件 | 推荐技术 | 理由 |
|---|---|---|
| macOS 监控 | Endpoint Security | 官方支持，文档完善 |
| Linux 监控 | eBPF + Aya | 性能高，Rust 生态好 |
| Windows 监控 | ETW + Tracing | 相对简单，无需驱动 |
| 数据存储 | SQLite | 本地优先，无需服务器 |
| 进程识别 | 代码签名 | 跨平台，可靠 |

### 6.3 风险与缓解

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| macOS Entitlement 被拒 | 无法使用 Endpoint Security | 使用 eslogger 作为备选 |
| eBPF 内核版本不兼容 | Linux 功能受限 | 降级到 fanotify |
| 性能开销 | 系统变慢 | 内核预过滤，减少事件量 |
| 隐私泄露 | 用户信任受损 | 本地优先，数据最小化 |

---

## 7. 参考资源

### 7.1 学术论文

- **FileGrail**：https://pypi.org/project/filegrail/ — 文件来源追踪工具
- **ShowProvenanceInfo**：https://github.com/FFRI/ShowProvenanceInfo — macOS Provenance Sandbox
- **OriginBlame**：https://github.com/tzbkk/originblame — 数据级来源追踪

### 7.2 开源项目

| 项目 | 平台 | 技术 | 链接 |
|---|---|---|---|
| Polka Engine | Linux | eBPF | https://github.com/notslok/polka |
| fs-watcher | Linux | eBPF | https://github.com/amandeepsp/fs-watcher |
| fim-ebpf | Linux | eBPF | https://github.com/harshavmb/fim-ebpf |
| FileMonitor | macOS | Endpoint Security | Objective-See 博客 |
| Mac Monitor | macOS | Endpoint Security | https://github.com/redcanaryco/mac-monitor |
| MinifilterMonitor | Windows | Minifilter | https://github.com/belazr/MinifilterMonitor |
| Privacy-Agent | 跨平台 | 本地优先 | https://github.com/jlynshue/local-private-orchestration |

### 7.3 官方文档

- Apple Endpoint Security：https://developer.apple.com/documentation/endpointsecurity
- Linux fanotify：https://man7.org/linux/man-pages/man7/fanotify.7.html
- Windows ETW：https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/adding-event-tracing-to-kernel-mode-drivers

---

## 8. 结论

### 8.1 核心发现

1. **技术上可行**：各平台均有成熟的文件来源追踪技术
2. **macOS 需 Entitlement**：Endpoint Security 需要向 Apple 申请
3. **Linux eBPF 最优**：性能高，功能完整
4. **隐私必须优先**：本地优先，数据最小化

### 8.2 推荐方案

**短期（MVP）**：
- macOS：Endpoint Security（申请 entitlement）或 eslogger
- Linux：eBPF（需 5.8+）或 fanotify
- Windows：ETW

**长期**：
- 与现有规则库组合，形成"静态匹配 + 动态监控"双引擎
- 实现"安装监控"和"软件清理"核心功能
- 保持本地优先，保护用户隐私

### 8.3 下一步行动

1. **申请 macOS Endpoint Security Entitlement**
2. **实现 eBPF 监控原型（Linux）**
3. **设计 Provenance 数据模型**
4. **编写隐私政策**
5. **用户测试**
