# SlimIt macOS 交互体验对齐计划

> 最后更新：2026-10-01。基于 Apple HIG 2026、Tauri 最佳实践、Linear/Raycast 等顶级 Mac 应用分析。

---

## 一、现代 macOS 软件交互水平基准

### 1.1 Apple Human Interface Guidelines 2026 核心原则

| 原则 | 说明 | 关键指标 |
|------|------|----------|
| **直接操控** | 用户直接操作内容，而非通过菜单 | 点击、拖拽、手势 |
| **反馈** | 每个操作都有即时视觉反馈 | <50ms 响应 |
| **一致性** | 遵循平台约定，降低学习成本 | 标准快捷键、菜单结构 |
| **层级清晰** | 信息架构直观，用户知道"我在哪" | 面包屑、标题层级 |
| **无障碍** | 所有用户都能使用 | VoiceOver、键盘导航、动态字体 |

### 1.2 macOS 26 Liquid Glass 设计语言

| 特性 | 说明 | 应用建议 |
|------|------|----------|
| **玻璃材质** | 半透明、模糊背景、折射效果 | 窗口背景、侧边栏、工具栏 |
| **连续圆角** | `.cornerRadius(style: .continuous)` | 所有容器、按钮、卡片 |
| **SF Symbols** | 系统符号，支持动态颜色 | 图标、按钮、列表项 |
| **系统颜色** | 自动适配深色/浅色模式 | 使用 `NSColor` 语义颜色 |
| **动态字体** | 支持 Dynamic Type | 使用 `UIFont` 或 `-apple-system` |

### 1.3 顶级 Mac 应用交互模式

#### Linear（产品开发平台）

| 模式 | 实现 | 效果 |
|------|------|------|
| **信息密度** | 高密度表格、紧凑行高 | 快速扫描、高效工作 |
| **视觉层级** | 侧边栏降暗、内容区突出 | 注意力聚焦 |
| **微交互** | 按钮按压、悬停提升、弹簧曲线 | 物理感、响应感 |
| **颜色系统** | LCH 色彩空间、对比度变量 | 可访问性、主题生成 |
| **动画** | 进入减速、退出加速、80ms 颜色变化 | 自然、不干扰 |

#### Raycast（生产力启动器）

| 模式 | 实现 | 效果 |
|------|------|------|
| **速度** | <50ms 启动、预加载、激进缓存 | 即时响应 |
| **键盘优先** | 所有操作有快捷键、内联显示 | 高效、可发现 |
| **原生集成** | Vibrancy、SF Symbols、系统颜色 | 属于 macOS |
| **HUD 确认** | 非阻塞通知、自动消失 | 不中断流程 |
| **搜索优先** | 自动聚焦、模糊匹配、高亮结果 | 快速定位 |

#### Divi（设计工具）

| 模式 | 实现 | 效果 |
|------|------|------|
| **画布中心** | 大工作区、工具栏可折叠 | 专注创作 |
| **面板化** | 可停靠、可折叠、可拖拽 | 灵活布局 |
| **实时预览** | 修改即时反映 | 所见即所得 |

---

## 二、SlimIt 当前 UI/UX 差距分析

### 2.1 架构层面

| 维度 | 现代 macOS 标准 | SlimIt 现状 | 差距等级 |
|------|-----------------|-------------|----------|
| **UI 框架** | SwiftUI / AppKit | 无框架，纯 HTML+CSS+JS | 🔴 严重 |
| **状态管理** | 响应式状态 | 全局变量 + DOM 操作 | 🔴 严重 |
| **组件化** | 声明式组件 | 单文件 891 行，无组件 | 🔴 严重 |
| **构建系统** | Vite / Xcode / SPM | 无构建步骤，直接刷新 | 🟡 中等 |
| **测试** | 单元测试 + E2E | 无前端测试 | 🔴 严重 |

### 2.2 视觉层面

| 维度 | 现代 macOS 标准 | SlimIt 现状 | 差距等级 |
|------|-----------------|-------------|----------|
| **深色模式** | 系统跟随，实时切换 | 仅深色，无浅色 | 🔴 严重 |
| **窗口装饰** | 原生标题栏、Traffic Lights | 自定义（无） | 🟡 中等 |
| **圆角** | 连续圆角 (`.continuous`) | 固定 4px | 🟡 中等 |
| **字体** | SF Pro、动态字体 | 系统字体栈，无动态 | 🟡 中等 |
| **图标** | SF Symbols | Emoji | 🔴 严重 |
| **颜色** | 系统语义颜色、LCH 色彩 | 硬编码十六进制 | 🔴 严重 |
| **玻璃效果** | Vibrancy、Liquid Glass | 无 | 🔴 严重 |
| **动画** | 物理曲线、微交互 | 仅 `fadeIn` | 🔴 严重 |

### 2.3 交互层面

| 维度 | 现代 macOS 标准 | SlimIt 现状 | 差距等级 |
|------|-----------------|-------------|----------|
| **键盘导航** | 全键盘可达、Tab 顺序 | 仅 Tab 切换面板 | 🔴 严重 |
| **快捷键** | 标准快捷键 (⌘C, ⌘V, ⌘W) | 无 | 🔴 严重 |
| **搜索** | 即时搜索、模糊匹配 | 无 | 🔴 严重 |
| **加载状态** | Skeleton、进度条 | 仅"扫描中…"文本 | 🔴 严重 |
| **错误处理** | 错误边界、可恢复 | 无 | 🔴 严重 |
| **确认模式** | 原生对话框、HUD | 双击确认（非标准） | 🟡 中等 |
| **拖拽** | 系统拖拽、拖放 | 无 | 🟡 中等 |
| **上下文菜单** | 原生菜单 | 无 | 🔴 严重 |

### 2.4 信息架构层面

| 维度 | 现代 macOS 标准 | SlimIt 现状 | 差距等级 |
|------|-----------------|-------------|----------|
| **导航** | 侧边栏 + 标签栏 | 顶部 5 标签 | 🟡 中等 |
| **面包屑** | 路径导航 | 无 | 🟡 中等 |
| **分页** | 无限滚动、分页 | 全量渲染（50 行截断） | 🔴 严重 |
| **排序** | 可点击表头 | 无 | 🔴 严重 |
| **过滤** | 侧边栏过滤、搜索 | 无 | 🔴 严重 |
| **空状态** | 引导性文案、操作按钮 | 无 | 🟡 中等 |

### 2.5 无障碍层面

| 维度 | 现代 macOS 标准 | SlimIt 现状 | 差距等级 |
|------|-----------------|-------------|----------|
| **ARIA** | 完整 ARIA 属性 | 无 | 🔴 严重 |
| **屏幕阅读器** | VoiceOver 支持 | 无 | 🔴 严重 |
| **动态字体** | Dynamic Type | 无 | 🔴 严重 |
| **对比度** | WCAG AA 4.5:1 | 未测试 | 🟡 中等 |
| **减少动效** | `prefers-reduced-motion` | 无 | 🔴 严重 |

---

## 三、差距量化评分

### 3.1 综合评分

| 类别 | 满分 | 当前得分 | 差距 |
|------|------|----------|------|
| **架构** | 100 | 20 | 80 |
| **视觉** | 100 | 30 | 70 |
| **交互** | 100 | 25 | 75 |
| **信息架构** | 100 | 35 | 65 |
| **无障碍** | 100 | 10 | 90 |
| **总分** | **500** | **120** | **380** |

### 3.2 关键差距 Top 10

| 排名 | 差距 | 影响 | 优先级 |
|------|------|------|--------|
| 1 | 无 UI 框架 | 无法维护、无法扩展 | P0 |
| 2 | 无深色/浅色模式 | 不符合 macOS 标准 | P0 |
| 3 | 无键盘导航 | 无法无障碍使用 | P0 |
| 4 | 无加载状态 | 用户不知道是否卡死 | P0 |
| 5 | 无错误处理 | 异常崩溃 | P0 |
| 6 | 无快捷键 | 效率低下 | P1 |
| 7 | 无搜索功能 | 无法快速定位 | P1 |
| 8 | 无 SF Symbols | 视觉不专业 | P1 |
| 9 | 无动画系统 | 感觉生硬 | P1 |
| 10 | 无分页/排序 | 大数据无法处理 | P1 |

---

## 四、对齐计划

### 4.1 阶段划分

```
┌─────────────────────────────────────────────────────────────┐
│                    阶段 1：基础重构（4 周）                    │
├─────────────────────────────────────────────────────────────┤
│  目标：从"网页应用"变成"Mac 应用"                              │
│                                                             │
│  1. 引入 UI 框架（React + TypeScript + Tailwind）             │
│  2. 实现深色/浅色模式切换                                     │
│  3. 实现键盘导航（全 Tab 可达）                                │
│  4. 实现加载状态（Skeleton、进度条）                           │
│  5. 实现错误边界（Error Boundary）                            │
│                                                             │
│  交付：可运行的新 UI 基础版                                    │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                    阶段 2：视觉升级（3 周）                    │
├─────────────────────────────────────────────────────────────┤
│  目标：符合 macOS 26 Liquid Glass 设计语言                     │
│                                                             │
│  1. 实现 Vibrancy 窗口背景                                    │
│  2. 替换 Emoji 为 SF Symbols                                  │
│  3. 实现连续圆角、系统颜色                                     │
│  4. 实现动画系统（物理曲线、微交互）                           │
│  5. 实现原生标题栏（Traffic Lights）                          │
│                                                             │
│  交付：视觉上"属于 macOS"                                     │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                    阶段 3：交互增强（3 周）                    │
├─────────────────────────────────────────────────────────────┤
│  目标：达到 Linear/Raycast 交互水平                            │
│                                                             │
│  1. 实现全局快捷键（⌘F 搜索、⌘W 关闭、⌘, 设置）               │
│  2. 实现即时搜索（模糊匹配、高亮）                             │
│  3. 实现分页、排序、过滤                                      │
│  4. 实现 HUD 确认、Toast 通知                                 │
│  5. 实现拖拽、上下文菜单                                      │
│                                                             │
│  交付：交互上"高效、专业"                                      │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                    阶段 4：无障碍与测试（2 周）                │
├─────────────────────────────────────────────────────────────┤
│  目标：所有用户都能使用                                        │
│                                                             │
│  1. 完整 ARIA 属性                                           │
│  2. VoiceOver 支持                                           │
│  3. 动态字体支持                                             │
│  4. 减少动效支持                                             │
│  5. 单元测试 + E2E 测试                                      │
│                                                             │
│  交付：可提交 App Store                                       │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 阶段 1：基础重构（4 周）

#### Week 1：技术选型与脚手架

| 任务 | 时间 | 交付 |
|------|------|------|
| 选择 UI 框架 | 1 天 | 决策文档 |
| 初始化 Vite + React + TS 项目 | 2 天 | 项目脚手架 |
| 配置 Tailwind CSS | 1 天 | 样式系统 |
| 设置 Tauri 集成 | 1 天 | 可运行的基础应用 |

**技术选型建议：**

| 选项 | 优点 | 缺点 | 推荐 |
|------|------|------|------|
| **React + Vite** | 生态最大、Tauri 官方支持 | 学习曲线中等 | ✅ |
| **Svelte + Vite** | 编译时优化、更小包 | 生态较小 | ⚠️ |
| **SolidJS** | 性能最优 | 生态最小 | ❌ |
| **Vue + Vite** | 易上手 | 与 Tauri 集成稍复杂 | ⚠️ |

**推荐：React + Vite + TypeScript + Tailwind CSS**

```bash
# 初始化项目
npm create vite@latest slimit-ui -- --template react-ts
cd slimit-ui
npm install

# 安装 Tailwind
npm install -D tailwindcss @tailwindcss/vite
npx tailwindcss init -p

# 安装 Tauri 插件
npm install @tauri-apps/api
```

#### Week 2：状态管理与组件化

| 任务 | 时间 | 交付 |
|------|------|------|
| 设计状态管理方案 | 1 天 | 状态树设计 |
| 实现 Zustand store | 2 天 | 状态管理 |
| 拆分 5 个 Tab 为独立组件 | 2 天 | 组件库 |

**状态管理方案：**

```typescript
// src/store/index.ts
import { create } from 'zustand';

interface AppState {
  activeTab: 'clean' | 'quarantine' | 'rules' | 'provenance' | 'advanced';
  theme: 'light' | 'dark' | 'system';
  scanResults: ScanResult | null;
  quarantineItems: QuarantineItem[];
  rules: Rule[];
  isLoading: boolean;
  error: Error | null;
  
  setActiveTab: (tab: AppState['activeTab']) => void;
  setTheme: (theme: AppState['theme']) => void;
  setScanResults: (results: ScanResult | null) => void;
  setQuarantineItems: (items: QuarantineItem[]) => void;
  setRules: (rules: Rule[]) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: Error | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  activeTab: 'clean',
  theme: 'system',
  scanResults: null,
  quarantineItems: [],
  rules: [],
  isLoading: false,
  error: null,
  
  setActiveTab: (tab) => set({ activeTab: tab }),
  setTheme: (theme) => set({ theme }),
  setScanResults: (results) => set({ scanResults: results }),
  setQuarantineItems: (items) => set({ quarantineItems: items }),
  setRules: (rules) => set({ rules }),
  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),
}));
```

**组件拆分：**

```
src/
├── components/
│   ├── layout/
│   │   ├── Sidebar.tsx        # 侧边栏导航
│   │   ├── Header.tsx         # 窗口标题栏
│   │   └── StatusBar.tsx      # 状态栏
│   ├── tabs/
│   │   ├── CleanTab.tsx       # 扫描面板
│   │   ├── QuarantineTab.tsx  # 隔离区
│   │   ├── RulesTab.tsx       # 规则库
│   │   ├── ProvenanceTab.tsx  # 来源追踪
│   │   └── AdvancedTab.tsx    # 高级功能
│   ├── common/
│   │   ├── Button.tsx         # 按钮
│   │   ├── Table.tsx          # 表格
│   │   ├── Badge.tsx          # 风险徽章
│   │   ├── Skeleton.tsx       # 加载骨架
│   │   ├── EmptyState.tsx     # 空状态
│   │   └── ErrorBoundary.tsx  # 错误边界
│   └── hooks/
│       ├── useScan.ts         # 扫描 Hook
│       ├── useQuarantine.ts   # 隔离区 Hook
│       └── useRules.ts        # 规则 Hook
├── store/
│   └── index.ts
├── styles/
│   └── globals.css
└── App.tsx
```

#### Week 3：深色/浅色模式

| 任务 | 时间 | 交付 |
|------|------|------|
| 实现系统主题检测 | 1 天 | 主题 Hook |
| 定义设计令牌 | 2 天 | CSS 变量 |
| 实现主题切换 UI | 1 天 | 设置面板 |
| 测试所有组件 | 1 天 | 主题适配 |

**设计令牌：**

```css
/* src/styles/tokens.css */
:root {
  /* 颜色 - 浅色模式 */
  --color-bg-primary: #ffffff;
  --color-bg-secondary: #f5f5f7;
  --color-bg-tertiary: #e5e5ea;
  --color-fg-primary: #1d1d1f;
  --color-fg-secondary: #424245;
  --color-fg-tertiary: #86868b;
  --color-accent: #007aff;
  --color-accent-hover: #0056cc;
  --color-border: #d2d2d7;
  --color-danger: #ff3b30;
  --color-warning: #ff9f0a;
  --color-success: #34c759;
  
  /* 圆角 */
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;
  --radius-xl: 16px;
  --radius-continuous: 9999px;
  
  /* 阴影 */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.05);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.07);
  --shadow-lg: 0 10px 15px rgba(0, 0, 0, 0.1);
  
  /* 动画 */
  --duration-fast: 120ms;
  --duration-standard: 200ms;
  --duration-moderate: 280ms;
  --ease-standard: cubic-bezier(0.4, 0, 0.2, 1);
  --ease-enter: cubic-bezier(0, 0, 0.2, 1);
  --ease-exit: cubic-bezier(0.4, 0, 1, 1);
}

/* 深色模式 */
.dark {
  --color-bg-primary: #1d1d1f;
  --color-bg-secondary: #2c2c2e;
  --color-bg-tertiary: #3a3a3c;
  --color-fg-primary: #f5f5f7;
  --color-fg-secondary: #e5e5ea;
  --color-fg-tertiary: #86868b;
  --color-accent: #0a84ff;
  --color-accent-hover: #409cff;
  --color-border: #3a3a3c;
  --color-danger: #ff453a;
  --color-warning: #ffd60a;
  --color-success: #30d158;
}
```

#### Week 4：加载状态与错误处理

| 任务 | 时间 | 交付 |
|------|------|------|
| 实现 Skeleton 组件 | 1 天 | 骨架屏 |
| 实现进度条组件 | 1 天 | 进度指示 |
| 实现 Error Boundary | 1 天 | 错误恢复 |
| 实现 Toast 通知 | 1 天 | 非阻塞反馈 |

**Skeleton 组件：**

```tsx
// src/components/common/Skeleton.tsx
import React from 'react';

interface SkeletonProps {
  className?: string;
  variant?: 'text' | 'circular' | 'rectangular';
  width?: number | string;
  height?: number | string;
}

export const Skeleton: React.FC<SkeletonProps> = ({
  className = '',
  variant = 'text',
  width,
  height,
}) => {
  return (
    <div
      className={`skeleton skeleton--${variant} ${className}`}
      style={{ width, height }}
      aria-hidden="true"
    />
  );
};
```

**Error Boundary：**

```tsx
// src/components/common/ErrorBoundary.tsx
import React from 'react';

interface Props {
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }
      return (
        <div className="error-boundary">
          <h2>出了点问题</h2>
          <p>{this.state.error?.message}</p>
          <button onClick={() => this.setState({ hasError: false, error: null })}>
            重试
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
```

### 4.3 阶段 2：视觉升级（3 周）

#### Week 5：Vibrancy 与窗口装饰

| 任务 | 时间 | 交付 |
|------|------|------|
| 实现 Vibrancy 窗口背景 | 2 天 | 玻璃效果 |
| 实现原生标题栏 | 2 天 | Traffic Lights |
| 实现窗口拖拽区域 | 1 天 | 可拖拽窗口 |

**Vibrancy 实现：**

```tsx
// src/components/layout/Vibrancy.tsx
import React from 'react';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface VibrancyProps {
  children: React.ReactNode;
  className?: string;
  material?: 'hud' | 'sidebar' | 'popover' | 'menu';
}

export const Vibrancy: React.FC<VibrancyProps> = ({
  children,
  className = '',
  material = 'hud',
}) => {
  const [isSupported, setIsSupported] = useState(true);

  useEffect(() => {
    // 检测是否支持 Vibrancy
    // 在 macOS 上通过 Tauri 插件实现
  }, []);

  if (!isSupported) {
    return <div className={className}>{children}</div>;
  }

  return (
    <div
      className={`vibrancy vibrancy--${material} ${className}`}
      data-tauri-drag-region
    >
      {children}
    </div>
  );
};
```

**Tauri 配置：**

```json
// tauri.conf.json
{
  "app": {
    "macOSPrivateApi": true
  },
  "windows": [{
    "title": "SlimIt",
    "width": 1100,
    "height": 760,
    "resizable": true,
    "decorations": false,
    "transparent": true
  }]
}
```

#### Week 6：SF Symbols 与图标系统

| 任务 | 时间 | 交付 |
|------|------|------|
| 安装 SF Symbols 库 | 1 天 | 图标组件 |
| 替换所有 Emoji 为 SF Symbols | 2 天 | 图标系统 |
| 实现图标选择器 | 1 天 | 规则图标 |
| 实现动态颜色 | 1 天 | 语义颜色 |

**SF Symbols 组件：**

```tsx
// src/components/common/SFSymbol.tsx
import React from 'react';

interface SFSymbolProps {
  name: string;
  size?: number;
  color?: string;
  className?: string;
}

export const SFSymbol: React.FC<SFSymbolProps> = ({
  name,
  size = 20,
  color,
  className = '',
}) => {
  return (
    <svg
      className={`sf-symbol ${className}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color || 'currentColor'}
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <use href={`#${name}`} />
    </svg>
  );
};
```

**常用符号映射：**

| 功能 | SF Symbol | 当前 Emoji |
|------|-----------|------------|
| 扫描 | `magnifyingglass` | 🔍 |
| 隔离区 | `archivebox` | 📦 |
| 规则 | `list.bullet` | 📋 |
| 来源 | `arrow.triangle.branch` | 🌿 |
| 设置 | `gearshape` | ⚙️ |
| 删除 | `trash` | 🗑️ |
| 恢复 | `arrow.uturn.backward` | ↩️ |
| 搜索 | `magnifyingglass` | 🔍 |
| 警告 | `exclamationmark.triangle` | ⚠️ |
| 成功 | `checkmark.circle` | ✅ |

#### Week 7：动画系统

| 任务 | 时间 | 交付 |
|------|------|------|
| 定义动画令牌 | 1 天 | CSS 变量 |
| 实现过渡组件 | 2 天 | 动画原语 |
| 实现微交互 | 2 天 | 按钮、卡片、列表 |
| 实现页面过渡 | 1 天 | 路由动画 |

**动画令牌：**

```css
/* src/styles/animations.css */
:root {
  /* 持续时间 */
  --duration-micro: 80ms;
  --duration-fast: 120ms;
  --duration-standard: 200ms;
  --duration-moderate: 280ms;
  --duration-slow: 400ms;
  
  /* 缓动曲线 */
  --ease-standard: cubic-bezier(0.4, 0, 0.2, 1);
  --ease-enter: cubic-bezier(0, 0, 0.2, 1);
  --ease-exit: cubic-bezier(0.4, 0, 1, 1);
  --ease-spring: cubic-bezier(0.175, 0.885, 0.32, 1.275);
  
  /* 减少动效 */
  --reduced-motion: 0;
}

@media (prefers-reduced-motion: reduce) {
  :root {
    --duration-micro: 0.01ms;
    --duration-fast: 0.01ms;
    --duration-standard: 0.01ms;
    --reduced-motion: 1;
  }
}
```

**过渡组件：**

```tsx
// src/components/common/Transition.tsx
import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';

interface TransitionProps {
  children: React.ReactNode;
  show: boolean;
  variant?: 'fade' | 'slide' | 'scale' | 'modal';
  duration?: number;
}

export const Transition: React.FC<TransitionProps> = ({
  children,
  show,
  variant = 'fade',
  duration = 200,
}) => {
  const variants = {
    fade: {
      initial: { opacity: 0 },
      animate: { opacity: 1 },
      exit: { opacity: 0 },
    },
    slide: {
      initial: { opacity: 0, y: 8 },
      animate: { opacity: 1, y: 0 },
      exit: { opacity: 0, y: -8 },
    },
    scale: {
      initial: { opacity: 0, scale: 0.97 },
      animate: { opacity: 1, scale: 1 },
      exit: { opacity: 0, scale: 0.97 },
    },
    modal: {
      initial: { opacity: 0, scale: 0.98, y: 16 },
      animate: { opacity: 1, scale: 1, y: 0 },
      exit: { opacity: 0, scale: 0.98, y: 16 },
    },
  };

  return (
    <AnimatePresence mode="wait">
      {show && (
        <motion.div
          variants={variants[variant]}
          initial="initial"
          animate="animate"
          exit="exit"
          transition={{
            duration: duration / 1000,
            ease: 'easeOut',
          }}
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  );
};
```

### 4.4 阶段 3：交互增强（3 周）

#### Week 8：全局快捷键

| 任务 | 时间 | 交付 |
|------|------|------|
| 定义快捷键映射 | 1 天 | 快捷键文档 |
| 实现全局快捷键 | 2 天 | 快捷键系统 |
| 实现快捷键显示 | 1 天 | 内联提示 |
| 实现命令面板 | 2 天 | ⌘K 面板 |

**快捷键映射：**

| 快捷键 | 功能 | 标准性 |
|--------|------|--------|
| `⌘F` | 搜索 | ✅ macOS 标准 |
| `⌘K` | 命令面板 | ✅ Linear/Raycast 标准 |
| `⌘W` | 关闭窗口 | ✅ macOS 标准 |
| `⌘,` | 打开设置 | ✅ macOS 标准 |
| `⌘R` | 重新扫描 | ⚠️ 自定义 |
| `⌘⇧F` | 全盘搜索 | ⚠️ 自定义 |
| `⌘1-5` | 切换 Tab | ⚠️ 自定义 |
| `⌘N` | 新建规则 | ⚠️ 自定义 |
| `⌘E` | 编辑规则 | ⚠️ 自定义 |
| `⌘D` | 删除选中 | ✅ 标准 |

**命令面板：**

```tsx
// src/components/common/CommandPalette.tsx
import React, { useState, useEffect } from 'react';
import { Transition } from './Transition';

interface Command {
  id: string;
  label: string;
  shortcut: string;
  icon: string;
  action: () => void;
}

export const CommandPalette: React.FC = () => {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');
  
  const commands: Command[] = [
    { id: 'search', label: '搜索文件', shortcut: '⌘F', icon: 'magnifyingglass', action: () => {} },
    { id: 'settings', label: '打开设置', shortcut: '⌘,', icon: 'gearshape', action: () => {} },
    { id: 'rescan', label: '重新扫描', shortcut: '⌘R', icon: 'arrow.clockwise', action: () => {} },
  ];

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === 'k') {
        e.preventDefault();
        setIsOpen(true);
      }
      if (e.key === 'Escape') {
        setIsOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const filtered = commands.filter(c => 
    c.label.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <>
      <Transition show={isOpen} variant="modal">
        <div className="command-palette" onClick={() => setIsOpen(false)}>
          <div className="command-palette__content" onClick={e => e.stopPropagation()}>
            <input
              className="command-palette__search"
              placeholder="输入命令..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              autoFocus
            />
            <div className="command-palette__list">
              {filtered.map(cmd => (
                <button
                  key={cmd.id}
                  className="command-palette__item"
                  onClick={() => {
                    cmd.action();
                    setIsOpen(false);
                  }}
                >
                  <span className="command-palette__icon">
                    <SFSymbol name={cmd.icon} />
                  </span>
                  <span className="command-palette__label">{cmd.label}</span>
                  <span className="command-palette__shortcut">{cmd.shortcut}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      </Transition>
    </>
  );
};
```

#### Week 9：搜索与过滤

| 任务 | 时间 | 交付 |
|------|------|------|
| 实现模糊搜索 | 2 天 | 搜索组件 |
| 实现表格排序 | 1 天 | 可点击表头 |
| 实现侧边栏过滤 | 2 天 | 过滤面板 |
| 实现分页 | 1 天 | 分页组件 |

**搜索组件：**

```tsx
// src/components/common/SearchInput.tsx
import React from 'react';

interface SearchInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  onKeyDown?: (e: React.KeyboardEvent) => void;
}

export const SearchInput: React.FC<SearchInputProps> = ({
  value,
  onChange,
  placeholder = '搜索...',
  onKeyDown,
}) => {
  return (
    <div className="search-input">
      <SFSymbol name="magnifyingglass" className="search-input__icon" />
      <input
        className="search-input__field"
        type="text"
        placeholder={placeholder}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        autoFocus
      />
      {value && (
        <button
          className="search-input__clear"
          onClick={() => onChange('')}
          aria-label="清除搜索"
        >
          <SFSymbol name="xmark.circle.fill" />
        </button>
      )}
    </div>
  );
};
```

**模糊搜索：**

```typescript
// src/utils/fuzzySearch.ts
export function fuzzySearch(query: string, items: string[]): string[] {
  const normalized = query.toLowerCase().trim();
  if (!normalized) return items;
  
  return items.filter(item => {
    const lower = item.toLowerCase();
    // 简单模糊匹配：所有字符按顺序出现
    let i = 0;
    for (const char of normalized) {
      const pos = lower.indexOf(char, i);
      if (pos === -1) return false;
      i = pos + 1;
    }
    return true;
  });
}

// 高亮匹配
export function highlightMatch(text: string, query: string): React.ReactNode {
  const normalized = query.toLowerCase().trim();
  if (!normalized) return text;
  
  const parts: React.ReactNode[] = [];
  let i = 0;
  
  for (const char of normalized) {
    const pos = text.toLowerCase().indexOf(char, i);
    if (pos === -1) break;
    
    if (pos > i) {
      parts.push(text.slice(i, pos));
    }
    parts.push(
      <mark key={pos} className="highlight">
        {text[pos]}
      </mark>
    );
    i = pos + 1;
  }
  
  if (i < text.length) {
    parts.push(text.slice(i));
  }
  
  return parts;
}
```

#### Week 10：HUD 与通知

| 任务 | 时间 | 交付 |
|------|------|------|
| 实现 Toast 组件 | 1 天 | 通知系统 |
| 实现 HUD 确认 | 1 天 | 非阻塞确认 |
| 实现进度通知 | 1 天 | 扫描进度 |
| 实现错误通知 | 1 天 | 错误反馈 |

**Toast 系统：**

```tsx
// src/components/common/Toast.tsx
import React, { useState, useEffect } from 'react';
import { Transition } from './Transition';

interface Toast {
  id: string;
  message: string;
  type: 'success' | 'error' | 'warning' | 'info';
  duration?: number;
}

const toasts: Toast[] = [];
const listeners: ((toasts: Toast[]) => void)[] = [];

export function showToast(message: string, type: Toast['type'] = 'info', duration = 3000) {
  const id = Math.random().toString(36).substring(7);
  toasts.push({ id, message, type, duration });
  notify();
  
  setTimeout(() => {
    const index = toasts.findIndex(t => t.id === id);
    if (index !== -1) {
      toasts.splice(index, 1);
      notify();
    }
  }, duration);
}

function notify() {
  listeners.forEach(fn => fn([...toasts]));
}

export const ToastContainer: React.FC = () => {
  const [toasts, setToasts] = useState<Toast[]>([]);
  
  useEffect(() => {
    const listener = (t: Toast[]) => setToasts(t);
    listeners.push(listener);
    return () => {
      const index = listeners.indexOf(listener);
      if (index !== -1) listeners.splice(index, 1);
    };
  }, []);

  return (
    <div className="toast-container">
      {toasts.map(toast => (
        <Transition key={toast.id} show={true} variant="scale">
          <div className={`toast toast--${toast.type}`}>
            <span className="toast__message">{toast.message}</span>
          </div>
        </Transition>
      ))}
    </div>
  );
};
```

### 4.5 阶段 4：无障碍与测试（2 周）

#### Week 11：无障碍

| 任务 | 时间 | 交付 |
|------|------|------|
| 添加 ARIA 属性 | 2 天 | 完整 ARIA |
| 实现 VoiceOver 支持 | 2 天 | 屏幕阅读器 |
| 实现动态字体 | 1 天 | Dynamic Type |
| 实现减少动效 | 1 天 | 动效开关 |

**ARIA 示例：**

```tsx
// src/components/common/Button.tsx
import React from 'react';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
  size?: 'sm' | 'md' | 'lg';
  loading?: boolean;
  icon?: React.ReactNode;
}

export const Button: React.FC<ButtonProps> = ({
  children,
  variant = 'primary',
  size = 'md',
  loading = false,
  icon,
  className = '',
  disabled,
  ...props
}) => {
  return (
    <button
      className={`button button--${variant} button--${size} ${className}`}
      disabled={disabled || loading}
      aria-busy={loading}
      {...props}
    >
      {icon && <span className="button__icon">{icon}</span>}
      {loading ? (
        <span className="button__spinner" aria-label="加载中" />
      ) : null}
      {children}
    </button>
  );
};
```

#### Week 12：测试

| 任务 | 时间 | 交付 |
|------|------|------|
| 配置测试框架 | 1 天 | Vitest + Testing Library |
| 编写单元测试 | 2 天 | 核心组件测试 |
| 编写 E2E 测试 | 2 天 | Playwright 测试 |
| 无障碍测试 | 1 天 | axe-core |

**测试配置：**

```bash
# 安装测试依赖
npm install -D vitest @testing-library/react @testing-library/user-event jsdom
npm install -D @playwright/test
npm install -D @axe-core/react
```

**单元测试示例：**

```tsx
// src/components/common/Button.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { Button } from './Button';

describe('Button', () => {
  it('渲染正确', () => {
    render(<Button>点击</Button>);
    expect(screen.getByRole('button')).toHaveTextContent('点击');
  });

  it('点击触发回调', () => {
    const onClick = jest.fn();
    render(<Button onClick={onClick}>点击</Button>);
    fireEvent.click(screen.getByRole('button'));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('加载时禁用', () => {
    render(<Button loading>点击</Button>);
    const button = screen.getByRole('button');
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('aria-busy', 'true');
  });

  it('符合无障碍标准', () => {
    render(<Button>点击</Button>);
    expect(screen.getByRole('button')).toHaveAttribute('type', 'button');
  });
});
```

---

## 五、资源需求

### 5.1 人力

| 角色 | 时间 | 职责 |
|------|------|------|
| **前端开发** | 12 周 | UI 实现、状态管理、组件开发 |
| **UI/UX 设计** | 4 周 | 设计系统、交互设计、视觉规范 |
| **测试工程师** | 2 周 | 测试编写、无障碍测试 |

### 5.2 技术栈

| 类别 | 技术 | 原因 |
|------|------|------|
| **UI 框架** | React 18 + TypeScript | 生态最大、Tauri 官方支持 |
| **构建工具** | Vite 5 | 快速、HMR、原生 ESM |
| **样式** | Tailwind CSS 3 | 原子化、设计令牌、深色模式 |
| **状态管理** | Zustand | 轻量、简单、TypeScript 友好 |
| **动画** | Framer Motion | 物理曲线、手势、布局动画 |
| **测试** | Vitest + Playwright | 快速、兼容 Jest |
| **图标** | SF Symbols + 自定义 | 系统原生、动态颜色 |

### 5.3 依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `@tauri-apps/api` | 2.x | Tauri IPC |
| `zustand` | 4.x | 状态管理 |
| `framer-motion` | 11.x | 动画 |
| `lucide-react` | 0.x | 图标（备选） |
| `fuse.js` | 7.x | 模糊搜索 |
| `date-fns` | 3.x | 日期格式化 |

---

## 六、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| **Vibrancy 在非 macOS 上不可用** | 视觉降级 | 检测平台，回退到半透明背景 |
| **SF Symbols 版本不兼容** | 图标缺失 | 提供 SVG 回退 |
| **React 学习曲线** | 开发效率 | 提供详细文档、代码模板 |
| **状态迁移复杂** | 功能丢失 | 逐步迁移，保持向后兼容 |
| **性能下降** | 响应变慢 | 使用 React.memo、useMemo、虚拟列表 |

---

## 七、成功指标

| 指标 | 当前 | 目标 | 测量方法 |
|------|------|------|----------|
| **启动时间** | 未知 | <100ms | Performance API |
| **搜索响应** | 无 | <50ms | 模糊搜索延迟 |
| **深色模式** | 无 | 系统跟随 | 用户测试 |
| **键盘导航** | 部分 | 100% 可达 | 无障碍审计 |
| **无障碍评分** | 0 | 90+ | axe-core |
| **用户满意度** | 未知 | 4.5/5 | 用户调研 |

---

## 八、结论

SlimIt 当前 UI/UX 与现代 macOS 应用存在**显著差距**（总分 120/500）。主要问题：

1. **无 UI 框架** — 单文件 891 行，无法维护
2. **无深色模式** — 不符合 macOS 标准
3. **无键盘导航** — 无法无障碍使用
4. **无加载状态** — 用户不知道是否卡死
5. **无错误处理** — 异常崩溃

**建议立即启动阶段 1（4 周）**，引入 React + TypeScript + Tailwind，实现基础 UI 框架。这是所有后续改进的基础。

**长期目标**：达到 Linear/Raycast 交互水平，成为"属于 macOS"的专业工具。
