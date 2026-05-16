# Voidnix

macOS 效率启动器。Tauri 2 + Rust 后端 + Vue 3 前端。

## 开发命令

```bash
bun install                  # 安装依赖
bun run tauri dev            # 开发模式
bun run build                # 类型检查 + 前端构建（失败即中断）
bun run lint                 # ESLint + UnoCSS 自动修复
bun run gen:bindings         # 重新生成 src/bindings.ts（修改 Rust 结构体后执行）
cd src-tauri && cargo check  # Rust 编译检查
./deploy.sh                  # 一键：lint → build → cargo check → tauri build → 替换 → 嵌入 Finder 扩展
```

## 模块系统

`src/modules/<name>/index.ts` 实现 `AppModule` 接口，`main.ts` glob 自动注册。Vue 组件用 `defineAsyncComponent` 懒加载，仅在首次激活时下载。

关键字段：`onSearch` / `onModuleSearch` / `onExecute` / `onInit` / `layout` / `useSearchInput` / `multiline` / `hidden` / `order` / `settings`

**模块向 App Shell 贡献的 UI 槽位**（仅这些，不增不减）：

| 槽位 | 位置 | 用途 |
|---|---|---|
| `layout.view` | 内容区 | 主视图 |
| `layout.header` | 视图上方 | 标签栏等 chrome |
| `layout.footer` | 视图下方 | 操作栏等 chrome |
| `layout.searchBarAccessory` | 搜索栏右侧 | 附属区域（选择器、状态标签、按钮组等，内容不限） |
| `settings` | 内容区（设置模式占满） | 模块设置面板 |

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

**前后端通信**：前端优先用 `src/bindings.ts` 导出的类型安全命令函数（由 `tauri-specta` 从 Rust 自动生成）；流式/事件类命令仍用裸 `invoke()`。Rust 用 `app.emit()`；所有 Command 须在 `lib.rs` 的 `invoke_handler` 注册。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock；`activateIgnoringOtherApps:YES` 抢焦点；失焦自动隐藏。WKWebView 驯化（`src-tauri/src/webkit_tuning/`）：隐藏时 `alphaValue=0` 不真隐藏以防节流，唤起时等待首帧呈现再显示，`VOIDNIX_DISABLE_WEBKIT_TUNING=1` 可关闭。

**全局快捷键**：Rust 进程监听（`commands/shortcut.rs`），四槽位：`main` / `clipboard` / `translate` / `chat`。

**搜索引擎**：`mdfind` + `nucleo-matcher` + 拼音首字母；权重：使用频率(≤800) + 应用(+2000) > 文件夹(+1000) > 文件。

**弹窗**：`BaseDialog`，`variant: confirm|form`；`appStore.showConfirm()` 返回 `Promise<boolean>`。

**斜杠命令**：`/` 触发列表，`Backspace`（空词）或 `Escape` 回退。

## Rust 后端结构

```
src-tauri/src/
├── lib.rs              # 入口
├── mac_utils.rs        # 窗口焦点、选词
├── webkit_tuning/      # WKWebView 驯化（节流抑制、首帧同步、Frame_Animator、Emoji_Warmer）
├── clipboard_monitor.rs / http.rs / text_selection.rs
├── commands/           # search, clipboard, shortcut, translate, chat, awake, ip, finder_ext
├── type_gen.rs         # tauri-specta 类型导出（cargo test --features specta export_bindings）
└── db/                 # SQLite
```

## UI 规范

只用 `@/components/ui/` 原子组件，禁止手写底层标签。

**原子组件**：`BaseButton` `BaseDialog` `BaseEmptyState` `BaseInput` `BaseList` `BaseListItem` `BaseSelect` `BaseTextarea` `ShortcutInput`

**布局**：`MainView` / `ContentView`

**样式**：主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint → disabled`；工具类 `ui-ctrl` `ui-ring` `ui-focus-within` `ui-disabled` `ui-hover` `ui-active`

## 状态管理

`appStore`（窗口/弹窗）/ `modulesStore`（模块列表）/ `settingsStore`（持久化）；Composables：`useSearchCommand` `useScrollPosition` `useInputControl` `useSettingsInput`

## 约定

- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`，未使用变量导致构建失败
- `isTauri()` 判断环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- 干净简洁的代码，精神洁癖，强迫症。
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，不主动执行 git 操作
