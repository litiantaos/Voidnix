# Voidnix

macOS 效率启动器。Tauri 2 + Rust 后端 + Vue 3 前端。

## 开发命令

```bash
bun install                # 安装依赖
bun run tauri dev          # 开发模式（Vite + Rust）
bun run build              # 前端构建（vue-tsc --noEmit && vite build）
bun run lint               # ESLint + UnoCSS 排序自动修复
cd src-tauri && cargo check  # Rust 编译检查
```

- 无测试脚本；`bun run build` 含类型检查，失败即中断
- 验证顺序：lint → build → cargo check
- 部署替换：`killall Voidnix FinderExt 2>/dev/null; rm -rf /Applications/Voidnix.app && cp -R src-tauri/target/release/bundle/macos/Voidnix.app /Applications/`
- `tauri build` 不嵌入 Finder 扩展，替换后须执行 `src-tauri/finder-extension/embed.sh /Applications/Voidnix.app`

## TypeScript 严格模式

`noUnusedLocals` + `noUnusedParameters` + `noFallthroughCasesInSwitch`，未使用变量/参数导致构建失败。

## 模块系统

每个功能在 `src/modules/<name>/index.ts` 实现 `AppModule` 接口，`main.ts` 通过 glob 自动扫描注册，无需修改核心代码。

### 关键接口

- `onSearch`：全局搜索回调
- `onModuleSearch`：进入模块后的搜索回调（`StandardModuleView` 渲染）
- `onExecute`：执行结果（打开文件、复制等）
- `onInit`：初始化，只调用一次
- `layout.view`：自定义视图（不设则用 `BaseList`）
- `layout.header` / `layout.footer`：声明式布局
- `useSearchInput`：复用主搜索框
- `multiline`：搜索框退化为标识，模块自管多行输入
- `hidden: true`：不在扩展列表显示，但响应全局搜索
- `order`：升序，控制扩展列表位置
- `settings`：设置视图组件

### 搜索结果结构

```typescript
SearchResult {
  id: string; title: string; module: string;  // 必填
  description?: string; icon?: string; score?: number; shortcut?: string
  data?: { path?: string; kind?: string; icon?: string | null; [key: string]: unknown }
}
```

`kind` 排序权重：`application` > `folder`/`file` > `module` > `clipboard`。

## 架构要点

### 前后端通信

- 前端 → Rust：`invoke('command_name', { args })`
- Rust → 前端：`app.emit('event-name', payload)`（流式响应、快捷键事件等）
- 所有 Commands 须在 `lib.rs` 的 `invoke_handler` 中注册

### 窗口与焦点

- `LSUIElement=true` + `ActivationPolicy::Accessory`：隐藏于 Dock/App Switcher
- `activateIgnoringOtherApps:YES` 强制抢焦点（`mac_utils.rs`）
- 失焦自动隐藏，跳过快捷键触发后 200ms 和弹窗关闭后 300ms

### 全局快捷键

Rust 进程监听（`commands/shortcut.rs`），避免 App Nap 假死。四个槽位：`main`、`clipboard`、`translate`、`chat`。开发模式下 `app.hide()` 被 `not(debug_assertions)` 包裹。

### 弹窗

`BaseDialog`：唯一弹窗组件，`variant` 驱动 `confirm`/`form` 模式。`appStore.showConfirm()` 返回 `Promise<boolean>`。

### 搜索引擎（Rust）

- 应用：`mdfind` + `mdls` 多线程，`notify` 监听 `/Applications` 实时更新
- 模糊匹配：`nucleo-matcher` + 拼音首字母；权重：使用频率（最高 800）+ 实时累加
- 文件：Spotlight 召回 + Nucleo 精排，限用户目录；权重阶梯：应用(+2000) > 文件夹(+1000) > 文件

### 斜杠命令

`/` 触发功能列表，回车进入，`Backspace`（空词时）或 `Escape` 回退。

## Rust 后端结构

```
src-tauri/src/
├── lib.rs              # 入口：插件、Command 注册、setup
├── mac_utils.rs        # macOS 原生（窗口焦点、选词）
├── clipboard_monitor.rs
├── http.rs
├── text_selection.rs   # 辅助功能 API 提取选中文本
├── commands/           # search, clipboard, shortcut, translate, chat, awake, ip, finder_ext
└── db/                 # SQLite
```

## UI 规范

必须使用 `@/components/ui/` 下的原子组件，禁止手写底层标签。

**原子组件**：`BaseButton` `BaseDialog` `BaseEmptyState` `BaseInput` `BaseList` `BaseListItem` `BaseSelect` `BaseTextarea` `ShortcutInput`

**布局**：`MainView`（主窗口框架）、`ContentView`（模块内容区）

### 设计规范

- 主题色 `accent`；圆角：控件 `rounded-md`，面板 `rounded-lg`；高度统一 `h-7`；文字 `text-sm`，辅助 `text-xs`
- 语义色阶：`text-tx-primary` → `secondary` → `subtle` → `muted` → `hint` → `faint` → `disabled`
- 工具类：`ui-ctrl` `ui-ring` `ui-focus-within` `ui-disabled` `ui-clickable` `ui-hover` `ui-active`；CSS 变量：`var(--tx-primary)` 等

## 状态管理（`src/stores/`）

- `appStore`：窗口状态、激活模块、弹窗
- `modulesStore`：模块列表、扩展列表
- `settingsStore`：持久化设置（`tauri-plugin-store`）
- Composables 也在此目录：`useSearchCommand`、`useScrollPosition`、`useInputControl`、`useSettingsInput`

## Git Commit 规范

格式：`<type>(<scope>): <subject>`，subject 用中文，不加句号。

type：`feat` 新功能 / `fix` 修复 / `refactor` 重构 / `style` 样式 / `perf` 性能 / `chore` 杂项 / `docs` 文档 / `revert` 回滚

```
feat(chat): 添加模型选择功能
fix(clipboard): 修复粘贴时丢失换行的问题
chore: 升级 tauri 到 2.x
```

## 其他约定

- `isTauri()`：判断 Tauri 环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
