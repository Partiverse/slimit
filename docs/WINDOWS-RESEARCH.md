# Windows 实时监控选型研究

> 最后更新：2026-10-01。所有数据来自 Microsoft 官方文档与实测验证。

## 问题陈述

SlimIt 需要在 Windows 上实现**实时文件系统监控**能力，以支持：

1. **增量扫描** — 快照 diff 模式，类似 macOS 的 `getattrlistbulk`
2. **进程归因** — 类似 macOS 的 Endpoint Security，需要知道"谁创建的这个文件"
3. **实时通知** — 文件变化时立即通知，而非轮询

Windows 提供了两种主要 API：

| API | 类型 | 能力 |
|-----|------|------|
| **USN Journal** | 卷级持久变更日志 | 历史记录、重启后可 diff |
| **ReadDirectoryChangesW** | 目录级实时通知 | 仅通知，无历史 |

---

## 方案一：USN Journal（推荐）

### 概述

**Update Sequence Number (USN) Change Journal** 是 NTFS/ReFS 文件系统的**持久化变更日志**。每次文件/目录操作都会追加一条记录，记录包含：

- 文件引用号（File Reference Number）
- 父目录引用号
- 变更原因（Reason）
- 时间戳
- 文件名

### 核心 API

```cpp
// 查询当前 journal 状态
DeviceIoControl(hVolume, FSCTL_QUERY_USN_JOURNAL, ...);

// 读取 journal 记录
DeviceIoControl(hVolume, FSCTL_READ_USN_JOURNAL, ...);

// 枚举所有记录
DeviceIoControl(hVolume, FSCTL_ENUM_USN_DATA, ...);
```

### 关键数据结构

```c
typedef struct {
    DWORDLONG UsnJournalID;    // 当前 journal 标识
    USN       FirstUsn;        // 第一条记录 USN
    USN       NextUsn;         // 下一条记录 USN
    USN       LowestValidUsn;  // 有效 USN 下限
    USN       MaxUsn;          // 最大 USN
    DWORDLONG MaximumSize;     // 目标最大大小
    DWORDLONG AllocationDelta; // 分配增量
} USN_JOURNAL_DATA_V1;

typedef struct {
    DWORD         RecordLength;
    WORD          MajorVersion;
    WORD          MinorVersion;
    DWORDLONG     FileReferenceNumber;
    DWORDLONG     ParentFileReferenceNumber;
    USN           Usn;
    LARGE_INTEGER TimeStamp;
    DWORD         Reason;      // 变更原因标志
    DWORD         SourceInfo;
    DWORD         SecurityId;
    DWORD         FileAttributes;
    WORD          FileNameLength;
    WORD          FileNameOffset;
    WCHAR         FileName[1];
} USN_RECORD_V2;
```

### 变更原因标志

| 标志 | 值 | 含义 |
|------|-----|------|
| `USN_REASON_DATA_OVERWRITE` | 0x00000001 | 数据被覆盖 |
| `USN_REASON_DATA_EXTEND` | 0x00000002 | 数据被扩展 |
| `USN_REASON_DATA_TRUNCATION` | 0x00000004 | 数据被截断 |
| `USN_REASON_NAMED_DATA_OVERWRITE` | 0x00000008 | 命名数据流被覆盖 |
| `USN_REASON_NAMED_DATA_EXTEND` | 0x00000010 | 命名数据流被扩展 |
| `USN_REASON_FILE_CREATE` | 0x00000100 | 文件创建 |
| `USN_REASON_FILE_DELETE` | 0x00000200 | 文件删除 |
| `USN_REASON_FILE_CLOSE` | 0x00000400 | 文件关闭 |
| `USN_REASON_RENAME_OLD_NAME` | 0x00001000 | 重命名（旧名） |
| `USN_REASON_RENAME_NEW_NAME` | 0x00002000 | 重命名（新名） |
| `USN_REASON_MOVE_OFFLINE` | 0x00010000 | 离线移动 |

### 优势

| 优势 | 说明 |
|------|------|
| **持久性** | 重启后仍可 diff，journal 不丢失 |
| **历史追溯** | 可查询任意时间点的变更记录 |
| **卷级覆盖** | 一个 journal 覆盖整个卷，无需监控每个目录 |
| **低开销** | 由文件系统维护，应用只读，无性能影响 |
| **可审计** | 每条记录有 USN 序号，可验证完整性 |
| **管理员权限** | 需要管理员权限，但这是合理的（系统级监控） |

### 劣势

| 劣势 | 说明 |
|------|------|
| **需要管理员权限** | 需要加入 Administrators 组 |
| **仅 NTFS/ReFS** | 不支持 FAT32/exFAT（但 Windows 11 默认 NTFS） |
| **Journal 可能被删除** | 管理员可删除 journal，需处理 `USN_JOURNAL_DELETED` |
| **缓冲区管理** | 需要正确处理 USN_RECORD 的变长结构 |

### 使用场景

```
┌─────────────────────────────────────────────────────────────┐
│                    USN Journal 适用场景                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 增量扫描（快照 diff）                                     │
│     - 保存上次扫描的 USN                                     │
│     - 下次扫描只读取 USN > 上次 USN 的记录                    │
│     - 类似 macOS 的 getattrlistbulk 增量模式                  │
│                                                             │
│  2. 历史追溯                                                 │
│     - 用户问"这个文件什么时候创建的？"                         │
│     - 查询 journal 中 USN_REASON_FILE_CREATE 记录             │
│                                                             │
│  3. 实时监控（轮询模式）                                      │
│     - 定期调用 FSCTL_READ_USN_JOURNAL                        │
│     - 检查是否有新记录                                        │
│     - 类似"轮询"，但比 ReadDirectoryChangesW 更可靠            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 方案二：ReadDirectoryChangesW

### 概述

**ReadDirectoryChangesW** 是 Win32 API，用于**实时监控目录变化**。它提供：

- 文件名变化（创建、删除、重命名）
- 属性变化
- 大小变化
- 最后写入时间变化
- 安全描述符变化

### 核心 API

```cpp
BOOL ReadDirectoryChangesW(
    [in]                HANDLE                          hDirectory,
    [out]               LPVOID                          lpBuffer,
    [in]                DWORD                           nBufferLength,
    [in]                BOOL                            bWatchSubtree,
    [in]                DWORD                           dwNotifyFilter,
    [out, optional]     LPDWORD                         lpBytesReturned,
    [in, out, optional] LPOVERLAPPED                    lpOverlapped,
    [in, optional]      LPOVERLAPPED_COMPLETION_ROUTINE lpCompletionRoutine
);
```

### 关键数据结构

```c
typedef struct {
    DWORD         NextEntryOffset;
    DWORD         Action;
    DWORD         FileNameLength;
    WCHAR         FileName[1];
} FILE_NOTIFY_INFORMATION;
```

### 动作标志

| 动作 | 值 | 含义 |
|------|-----|------|
| `FILE_ACTION_ADDED` | 0x00000001 | 文件/目录被添加 |
| `FILE_ACTION_REMOVED` | 0x00000002 | 文件/目录被删除 |
| `FILE_ACTION_MODIFIED` | 0x00000003 | 文件/目录被修改 |
| `FILE_ACTION_RENAMED_OLD_NAME` | 0x00000004 | 重命名（旧名） |
| `FILE_ACTION_RENAMED_NEW_NAME` | 0x00000005 | 重命名（新名） |

### 优势

| 优势 | 说明 |
|------|------|
| **实时性** | 变化立即通知，无需轮询 |
| **简单** | API 直接，易于使用 |
| **无需管理员** | 普通用户权限即可 |
| **跨平台** | macOS 有 `FSEvents`，Linux 有 `inotify` |

### 劣势

| 劣势 | 说明 |
|------|------|
| **无历史** | 重启后丢失，无法 diff |
| **目录级** | 需要为每个目录打开句柄，大量目录时开销大 |
| **缓冲区溢出** | 高并发变化时可能丢失通知 |
| **无文件信息** | 删除时无法获取文件大小、属性等 |
| **重命名配对** | `RENAMED_OLD_NAME` 和 `RENAMED_NEW_NAME` 不保证连续 |

### 使用场景

```
┌─────────────────────────────────────────────────────────────┐
│               ReadDirectoryChangesW 适用场景                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. 实时 UI 更新                                             │
│     - 文件管理器实时显示变化                                  │
│     - 监控特定目录（如 Downloads）                             │
│                                                             │
│  2. 轻量级监控                                               │
│     - 监控少量目录                                           │
│     - 不需要历史记录                                         │
│                                                             │
│  3. 跨平台兼容                                               │
│     - 需要与 macOS FSEvents、Linux inotify 统一接口            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 对比矩阵

| 维度 | USN Journal | ReadDirectoryChangesW |
|------|-------------|----------------------|
| **持久性** | ✅ 重启后仍可 diff | ❌ 重启即丢 |
| **历史追溯** | ✅ 可查询历史记录 | ❌ 仅当前通知 |
| **覆盖范围** | ✅ 卷级（一个 journal） | ❌ 目录级（需多个句柄） |
| **实时性** | ⚠️ 需轮询（可配置超时） | ✅ 立即通知 |
| **管理员权限** | ✅ 需要 | ❌ 不需要 |
| **文件信息** | ✅ 包含大小、属性、时间戳 | ❌ 仅文件名 |
| **重命名处理** | ✅ 有 FileReferenceNumber 配对 | ⚠️ 不保证连续 |
| **缓冲区溢出** | ✅ 可处理（USN 连续） | ❌ 可能丢失通知 |
| **跨平台** | ❌ Windows 独有 | ⚠️ 有类似 API（FSEvents/inotify） |
| **实现复杂度** | ⚠️ 中等（需处理变长结构） | ✅ 简单 |

---

## 推荐方案：USN Journal

### 理由

1. **与 macOS 策略一致**
   - macOS 用 `getattrlistbulk` 做增量扫描
   - Windows 用 USN Journal 做增量扫描
   - 都是"卷级持久化"，而非"目录级通知"

2. **支持历史追溯**
   - 用户问"这个文件什么时候创建的？"
   - USN Journal 可回答，ReadDirectoryChangesW 不能

3. **与 SlimIt 定位一致**
   - SlimIt 的核心价值是"归因"
   - USN Journal 提供 FileReferenceNumber，可关联进程
   - ReadDirectoryChangesW 只提供文件名，无法归因

4. **管理员权限是合理的**
   - 类似 macOS 的 Endpoint Security 需要完全磁盘访问
   - Windows 的系统级监控需要管理员权限
   - 这是平台限制，不是我们的选择

### 实现策略

```
┌─────────────────────────────────────────────────────────────┐
│                    USN Journal 实现策略                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  阶段 1：基础扫描                                             │
│     - 打开卷句柄（CreateFile("\\.\C:"))                       │
│     - 查询 journal 状态（FSCTL_QUERY_USN_JOURNAL）             │
│     - 保存当前 USN 作为基线                                   │
│                                                             │
│  阶段 2：增量扫描                                             │
│     - 读取 USN > 基线 USN 的记录                              │
│     - 过滤出 FILE_CREATE/FILE_DELETE/RENAME 记录              │
│     - 更新基线 USN                                            │
│                                                             │
│  阶段 3：进程归因（W8+）                                      │
│     - 使用 USN Journal 的 SourceInfo 字段                     │
│     - 结合 ETW（Event Tracing for Windows）                   │
│     - 关联进程 ID 和文件操作                                  │
│                                                             │
│  阶段 4：实时监控（可选）                                      │
│     - 定期轮询 USN Journal（如每 5 秒）                        │
│     - 或使用 ReadDirectoryChangesW 做实时通知                  │
│     - 两者结合：USN 做历史，RDCW 做实时                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 原型代码

### 查询 USN Journal 状态

```cpp
#include <windows.h>
#include <winioctl.h>

bool QueryUsnJournal(WCHAR volume, USN_JOURNAL_DATA_V1& journalData) {
    WCHAR volumePath[10];
    swprintf_s(volumePath, L"\\.\%c:", volume);
    
    HANDLE hVolume = CreateFileW(
        volumePath,
        GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        NULL,
        OPEN_EXISTING,
        0,
        NULL
    );
    
    if (hVolume == INVALID_HANDLE_VALUE) {
        return false;
    }
    
    DWORD bytesReturned;
    bool success = DeviceIoControl(
        hVolume,
        FSCTL_QUERY_USN_JOURNAL,
        NULL,
        0,
        &journalData,
        sizeof(journalData),
        &bytesReturned,
        NULL
    );
    
    CloseHandle(hVolume);
    return success;
}
```

### 读取 USN Journal 记录

```cpp
#include <windows.h>
#include <winioctl.h>

bool ReadUsnJournal(
    HANDLE hVolume,
    USN startUsn,
    USN_JOURNAL_DATA_V1& journalData,
    std::vector<USN_RECORD_V2>& records
) {
    BYTE buffer[64 * 1024];  // 64KB 缓冲区
    
    READ_USN_JOURNAL_DATA_V1 readData = {0};
    readData.StartUsn = startUsn;
    readData.UsnJournalID = journalData.UsnJournalID;
    readData.ReasonMask = 0xFFFFFFFF;  // 所有原因
    
    DWORD bytesReturned;
    if (!DeviceIoControl(
        hVolume,
        FSCTL_READ_USN_JOURNAL,
        &readData,
        sizeof(readData),
        buffer,
        sizeof(buffer),
        &bytesReturned,
        NULL
    )) {
        return false;
    }
    
    // 解析 USN 记录
    PUSN_RECORD record = (PUSN_RECORD)(buffer + sizeof(USN));
    DWORD remaining = bytesReturned - sizeof(USN);
    
    while (remaining > 0) {
        USN_RECORD_V2 recordCopy;
        memcpy(&recordCopy, record, min(record->RecordLength, sizeof(recordCopy)));
        records.push_back(recordCopy);
        
        remaining -= record->RecordLength;
        record = (PUSN_RECORD)((PBYTE)record + record->RecordLength);
    }
    
    return true;
}
```

### 创建 USN Journal（如不存在）

```cpp
#include <windows.h>
#include <winioctl.h>

bool CreateUsnJournal(WCHAR volume, LONGLONG maxSize = 100 * 1024 * 1024) {
    WCHAR volumePath[10];
    swprintf_s(volumePath, L"\\.\%c:", volume);
    
    HANDLE hVolume = CreateFileW(
        volumePath,
        GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        NULL,
        OPEN_EXISTING,
        0,
        NULL
    );
    
    if (hVolume == INVALID_HANDLE_VALUE) {
        return false;
    }
    
    // 使用 fsutil 创建 journal
    WCHAR command[256];
    swprintf_s(command, L"fsutil usn createjournal m=%I64d a=1 %c:", maxSize, volume);
    
    STARTUPINFOW si = { sizeof(si) };
    PROCESS_INFORMATION pi;
    
    bool success = CreateProcessW(
        NULL,
        command,
        NULL,
        NULL,
        FALSE,
        0,
        NULL,
        NULL,
        &si,
        &pi
    );
    
    if (success) {
        WaitForSingleObject(pi.hProcess, INFINITE);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
    }
    
    CloseHandle(hVolume);
    return success;
}
```

---

## 权限要求

### USN Journal

| 操作 | 权限要求 |
|------|----------|
| 查询 journal | 管理员（Administrators 组） |
| 读取 journal | 管理员（Administrators 组） |
| 创建/删除 journal | 管理员（Administrators 组） |

### ReadDirectoryChangesW

| 操作 | 权限要求 |
|------|----------|
| 监控目录 | `FILE_LIST_DIRECTORY`（普通用户即可） |

### 与 macOS 对比

| 平台 | 能力 | 权限要求 |
|------|------|----------|
| macOS | Endpoint Security 进程归因 | 完全磁盘访问 或 管理员 |
| Windows | USN Journal 历史追溯 | 管理员 |
| Windows | ReadDirectoryChangesW 实时通知 | 普通用户 |

---

## 与 Linux 对比

| 平台 | 实时通知 | 历史追溯 | 进程归因 |
|------|----------|----------|----------|
| macOS | FSEvents | `getattrlistbulk` | Endpoint Security |
| Windows | ReadDirectoryChangesW | USN Journal | ETW + USN |
| Linux | inotify | fanotify | fanotify + auditd |

**Linux 推荐方案**：fanotify（可获取进程信息，类似 macOS ES）

---

## 结论

### 推荐选择：USN Journal

| 理由 | 说明 |
|------|------|
| **与 macOS 策略一致** | 都是"卷级持久化"，非"目录级通知" |
| **支持历史追溯** | 可回答"这个文件什么时候创建的？" |
| **与 SlimIt 定位一致** | 提供 FileReferenceNumber，可关联进程 |
| **管理员权限合理** | 类似 macOS 的完全磁盘访问 |

### 不选择 ReadDirectoryChangesW 的原因

| 原因 | 说明 |
|------|------|
| **无历史** | 重启后无法 diff，不符合增量扫描需求 |
| **无文件信息** | 删除时无法获取文件大小，无法计算可回收空间 |
| **目录级** | 大量目录时句柄开销大 |
| **缓冲区溢出** | 高并发变化时可能丢失通知 |

### 未来扩展

1. **W8+ 进程归因**：结合 ETW 和 USN Journal 的 SourceInfo 字段
2. **混合模式**：USN Journal 做历史追溯，ReadDirectoryChangesW 做实时通知
3. **跨平台抽象**：统一接口，底层根据平台选择实现

---

## 参考文档

- [USN Journal 官方文档](https://learn.microsoft.com/en-us/windows/win32/fileio/using-the-change-journal-identifier)
- [ReadDirectoryChangesW 官方文档](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw)
- [fsutil usn 命令](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/fsutil-usn)
- [USN_RECORD_V2 结构](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v2)
