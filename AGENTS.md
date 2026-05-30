# Voidnix

macOS 效率启动器。Tauri 2 + Rust 后端 + Vue 3 前端。核心架构：应用作为基底框架和功能扩展完全解耦各自独立。

## 开发命令

```bash
bun install                  # 安装依赖
bun run tauri:dev            # 开发模式（sync → 格式化 + lint → 启动）
./deploy.sh                  # 打包部署（tauri build → 替换 .app → 嵌入 Finder 扩展）
```

内部命令（由 tauri 自动调用或组合使用）：

```bash
bun run dev                  # 仅启动 Vite 开发服务器（tauri.conf.json beforeDevCommand）
bun run build                # sync → 格式化 + lint → 类型检查 → 前端构建（tauri.conf.json beforeBuildCommand）
bun run lint                 # Prettier 格式化 + ESLint 修复（含 UnoCSS class 排序）
bun run sync:extensions      # 同步扩展注册
```

## 扩展系统

### 目录约定

前端文件平铺在扩展根目录，Rust 后端放在 `native/` 子目录：

```
extensions/
└── <扩展名称>/
    ├── index.ts              # Vue 模块入口，元数据 + 运行时逻辑一体
    ├── ...                   # Vue 组件、composables 等前端文件平铺
    └── native/
        └── mod.rs            # Rust 后端入口
```

### 注册机制

`main.ts` glob 自动注册前端模块（`@ext/*/index.ts`）。Vue 组件用 `defineAsyncComponent` 懒加载，仅在首次激活时下载。

`scripts/sync-extensions.ts` 扫描 `extensions/*/native/mod.rs` 和 `src-tauri/src/*.rs`（核心模块），提取 `#[tauri::command]` 和 `pub fn init()`，生成 `src-tauri/src/extensions/mod.rs`：

- `#[path]` 直接引用源文件，**不复制**
- `configure_app!` 宏统一注册所有命令和插件
- 核心模块（shortcut、window）通过 `crate::模块名` 引用，扩展通过 `crate::extensions::模块名`
- 模块名含连字符（如 `finder-ext`）时，Rust 端自动映射为下划线（`finder_ext`）

### 前端示例

```typescript
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'

const calculatorModule: AppModule = {
  id: 'calculator',
  name: '计算器',
  description: '支持数学表达式计算及历史记录',
  icon: 'i-ri-calculator-line',
  keywords: ['calc', 'calculator', 'math', '计算器', '数学'],
  order: 2,
  onSearch: async (query: string) => {
    return []
  },
}

registerModule(calculatorModule)
```

### 后端示例

```rust
// #[tauri::command] 自动注册到 invoke_handler
#[tauri::command]
pub fn eval(expression: String) -> Result<String, String> {
    Ok("42".to_string())
}

// init() 自动注册为 Tauri Plugin
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("calculator").build()
}
```

### 新增扩展

```bash
# 1. 在 extensions/ 下新建文件夹
mkdir extensions/my-ext
# 2. 写代码（index.ts + native/mod.rs）
# 3. 运行同步
bun run sync:extensions
# 4. 开发
bun run tauri dev
```

### 注意事项

- 扩展名不要与现有扩展冲突
- `native/mod.rs` 中的 `#[tauri::command]` 函数会被自动发现并注册
- `pub fn init()` 会被自动注册为 Tauri Plugin
- 目前扩展的 native 由单文件 `mod.rs` 组成；如需多文件，后续可扩展为支持子目录
- 核心能力（shortcut、window 等）放在 `src-tauri/src/core/` 下，不在 extensions/ 中

## 核心模块 vs 扩展

| 类别     | 位置                         | 说明                                                  |
| -------- | ---------------------------- | ----------------------------------------------------- |
| 核心模块 | `src-tauri/src/core/*.rs`    | 应用基础设施：快捷键、窗口管理等，与 App Shell 紧耦合 |
| 扩展     | `extensions/*/native/mod.rs` | 独立功能模块：搜索、翻译、截屏等，通过 `#[path]` 引用 |

运行时字段：`onSearch` / `onModuleSearch` / `onExecute` / `onInit` / `onActivate` / `onDeactivate` / `onOpenPanel` / `onSearchInput` / `layout` / `panel` / `globalShortcuts` / `windowViews`

**模块向 App Shell 贡献的 UI 槽位**（仅这些，不增不减）：

- `layout.view`：内容区，主视图
- `layout.header`：视图上方，标签栏等 chrome
- `layout.footer`：视图下方，操作栏等 chrome
- `layout.searchBarAccessory`：搜索栏右侧，附属区域（选择器、状态标签、按钮组等，内容不限）
- `panel`：内容区（占满，隐藏主视图 chrome），模块二级面板（配置页 / 功能结果页）

槽位组件命名以 `Actions` / `Header` / `Footer` 后缀对应位置。模块视图内部的私有 UI（如截图标注调色板）**禁止**使用 `Toolbar` / `Header` / `Footer` 等会与槽位混淆的命名，应使用语义明确的名字如 `AnnotationPalette` / `MessageComposer` / `HistoryFilter`。

```typescript
SearchResult {
  id: string; title: string; module: string;       // 必填
  description?: string; icon?: string; score?: number; shortcut?: string
  data?: { path?: string; kind?: string; icon?: string | null; [key: string]: unknown }
}
```

`kind` 权重：`application` > `folder`/`file` > `module` > `clipboard`

## 架构要点

**前后端通信**：前端优先用 `src/bindings.ts` 导出的类型安全命令函数（由 `tauri-specta` 从 Rust 自动生成；修改 Rust 结构体后运行 `bun run sync:extensions && cd src-tauri && cargo test --features specta export_bindings -- --nocapture` 重新生成）；流式/事件类命令仍用裸 `invoke()`。Rust 用 `app.emit()`；所有 Command 须在 `configure_app!` 宏注册。

**模块面板**：`open_module_panel(moduleId, payload)` 为通用命令，Rust 显示主窗口后发送 `open-module-panel` 事件；App.vue 接收事件，激活模块、显示面板，并调用模块注册的 `onOpenPanel(payload)`。模块通过 `panel` 槽位声明面板组件，通过 `onOpenPanel` 解析 payload 更新内部状态。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock；`activateIgnoringOtherApps:YES` 抢焦点；失焦自动隐藏。WKWebView 驯化（`src-tauri/src/macos/webkit_tuning/`）：隐藏时 `alphaValue=0` 不真隐藏以防节流，唤起时等待首帧呈现再显示，`VOIDNIX_DISABLE_WEBKIT_TUNING=1` 可关闭。

**全局快捷键**：Rust 核心模块监听（`src-tauri/src/core/shortcut.rs`），四槽位：`main` / `clipboard` / `translate` / `chat`。

**搜索引擎**：`mdfind` + `nucleo-matcher` + 拼音首字母；权重：使用频率(≤800) + 应用(+2000) > 文件夹(+1000) > 文件。

**弹窗**：`BaseDialog`，`variant: confirm|form`；`appStore.showConfirm()` 返回 `Promise<boolean>`。

**斜杠命令**：`/` 触发列表，`Backspace`（空词）或 `Escape` 回退。

## Rust 后端结构

```
src-tauri/src/
├── lib.rs              # 入口
├── main.rs             # macOS 入口
├── extensions.rs       # 自动生成的扩展注册（#[path] 引用 extensions/*/native/mod.rs）
├── core/               # 核心模块（Tauri 命令 + init() 插件）
│   ├── mod.rs
│   ├── shortcut.rs     # 全局快捷键
│   └── window.rs       # 窗口管理命令
├── infra/              # 基础设施模块（无 Tauri 命令，跨平台通用）
│   ├── mod.rs
│   ├── db.rs           # SQLite 数据库
│   ├── http.rs         # HTTP 客户端
│   └── sse.rs          # SSE 流式请求
├── macos/              # macOS 原生桥接模块
│   ├── mod.rs
│   ├── mac_utils.rs        # 窗口焦点、选词
│   ├── click_monitor.rs    # 点击外部监听
│   ├── clipboard_monitor.rs
│   ├── text_selection.rs   # AX 文本选中 + 剪贴板注入
│   ├── skylight.rs         # Space 迁移（私有 API）
│   └── webkit_tuning/      # WKWebView 驯化（节流抑制、首帧同步、Frame_Animator、Emoji_Warmer）
└── type_gen.rs         # tauri-specta 类型导出（cargo test --features specta export_bindings）

extensions/             # 所有功能扩展
├── <name>/
│   ├── index.ts            # Vue 模块入口
│   ├── ...                 # 前端文件平铺（组件、composables 等）
│   └── native/
│       └── mod.rs          # Rust 命令 + init()
```

## UI 规范

只用 `@/components/ui/` 原子组件，禁止手写底层标签。

**原子组件**：`BaseButton` `BaseDialog` `BaseEmptyState` `BaseInput` `BaseList` `BaseListItem` `BaseSelect` `BaseSlider` `BaseTextarea` `ShortcutInput`

**布局**：`MainView` / `ContentView`

**样式**：主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint → disabled`；工具类 `ui-ctrl` `ui-ring` `ui-focus-within` `ui-disabled` `ui-active`

## 共享工具

前端共享工具集中在 `src/utils/` 和 `src/core/` 下，扩展中禁止重复实现：

| 模块                         | 导出                                                                                                                  | 用途                                   |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| `src/utils/events.ts`        | `useScroll()`, `onKeyStroke()`                                                                                        | 滚动位置追踪、键盘事件监听（自动清理） |
| `src/utils/clipboard.ts`     | `copyAndHide(value)`                                                                                                  | 复制到剪贴板并隐藏窗口                 |
| `src/utils/tauri.ts`         | `isTauri`, `hideWindow(auto?)`, `toSearchResults()`, `cacheIconFromResult()`                                          | Tauri 环境判断、窗口隐藏、搜索结果转换 |
| `src/utils/provider.ts`      | `providerLabelFromUrl(url, fallback)`                                                                                 | 从 API URL 提取提供商标签              |
| `src/utils/error.ts`         | `toErrorMessage(e, fallback?)`                                                                                        | 统一 Error → 字符串                    |
| `src/utils/dom.ts`           | `getFocusableElements()`, `isComposing()`, `isFormControl()`, `cycleFocus()`, `trapFocus()`, `wrapIndex()`            | DOM 查询、键盘事件、焦点管理           |
| `src/core/module-helpers.ts` | `moduleSelfResult()`, `getVisibleModules()`, `moduleToSearchResult()`, `keywordModuleSearch()`, `makeToggleHandler()` | 模块搜索结果构建、快捷键 toggle 处理   |

Composables：`useSearchCommand` `useScrollPosition` `useInputControl` `useSettingsInput` `useTauriListener` `useShortcutConfig`

**settingsStore 内部工具**：`parseActiveConfig()` `createConfigManager()` `createSyncedSetter()` `loadSetting()`

## 状态管理

`appStore`（窗口/弹窗）/ `settingsStore`（持久化）

## Rust 共享工具

| 模块                         | 导出                                                         | 用途                               |
| ---------------------------- | ------------------------------------------------------------ | ---------------------------------- |
| `infra::db::Database`        | `conn()`                                                     | 封装 Mutex lock + poison recovery  |
| `infra::sse`                 | `validate_ai_request(endpoint, model, api_key)`              | AI 请求端点/模型/密钥统一校验      |
| `webkit_tuning::FailCounter` | `new(limit)`, `is_disabled()`, `record_failure()`, `reset()` | 原子失败计数器，替代重复 static+fn |

## 约定

- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`，未使用变量导致构建失败
- `isTauri()` 判断环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- 干净简洁的代码，精神洁癖，强迫症。
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，不主动执行 git 操作
