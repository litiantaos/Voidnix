# Voidnix

macOS 效率启动器。Tauri 2 + Rust 后端 + Vue 3 前端。

## 开发命令

```bash
bun install                  # 安装依赖
bun run tauri dev            # 开发模式
bun run build                # 类型检查 + 前端构建（失败即中断）
bun run lint                 # ESLint + UnoCSS 自动修复
cd src-tauri && cargo check  # Rust 编译检查
./deploy.sh                  # 一键：lint → build → cargo check → tauri build → 替换 → 嵌入 Finder 扩展
```

## 模块系统

`src/modules/<name>/index.ts` 实现 `AppModule` 接口，`main.ts` glob 自动注册。

关键字段：`onSearch` / `onModuleSearch` / `onExecute` / `onInit` / `layout.view` / `layout.header|footer` / `useSearchInput` / `multiline` / `hidden` / `order` / `settings`

```typescript
SearchResult {
  id: string; title: string; module: string;       // 必填
  description?: string; icon?: string; score?: number; shortcut?: string
  data?: { path?: string; kind?: string; icon?: string | null; [key: string]: unknown }
}
```

`kind` 权重：`application` > `folder`/`file` > `module` > `clipboard`

## 架构要点

**前后端通信**：前端用 `invoke()`，Rust 用 `app.emit()`；所有 Command 须在 `lib.rs` 的 `invoke_handler` 注册。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock；`activateIgnoringOtherApps:YES` 抢焦点；失焦自动隐藏。

**全局快捷键**：Rust 进程监听（`commands/shortcut.rs`），四槽位：`main` / `clipboard` / `translate` / `chat`。

**搜索引擎**：`mdfind` + `nucleo-matcher` + 拼音首字母；权重：使用频率(≤800) + 应用(+2000) > 文件夹(+1000) > 文件。

**弹窗**：`BaseDialog`，`variant: confirm|form`；`appStore.showConfirm()` 返回 `Promise<boolean>`。

**斜杠命令**：`/` 触发列表，`Backspace`（空词）或 `Escape` 回退。

## Rust 后端结构

```
src-tauri/src/
├── lib.rs              # 入口
├── mac_utils.rs        # 窗口焦点、选词
├── clipboard_monitor.rs / http.rs / text_selection.rs
├── commands/           # search, clipboard, shortcut, translate, chat, awake, ip, finder_ext
└── db/                 # SQLite
```

## UI 规范

只用 `@/components/ui/` 原子组件，禁止手写底层标签。

**原子组件**：`BaseButton` `BaseDialog` `BaseEmptyState` `BaseInput` `BaseList` `BaseListItem` `BaseSelect` `BaseTextarea` `ShortcutInput`

**布局**：`MainView` / `ContentView`

**样式**：主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint → disabled`；工具类 `ui-ctrl` `ui-ring` `ui-focus-within` `ui-disabled` `ui-clickable` `ui-hover` `ui-active`

## 状态管理

`appStore`（窗口/弹窗）/ `modulesStore`（模块列表）/ `settingsStore`（持久化）；Composables：`useSearchCommand` `useScrollPosition` `useInputControl` `useSettingsInput`

## 约定

- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`，未使用变量导致构建失败
- `isTauri()` 判断环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，不主动执行 git 操作
