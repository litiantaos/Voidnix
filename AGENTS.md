# Voidnix

macOS 效率启动器。Tauri 2 + Rust + Vue 3。统一扩展架构：

- **框架**（`src-tauri/src/runtime/` + `platform/`）：运行时核心 + macOS 原生桥，零业务语义
- **扩展**（`extensions/<name>/`）：全部一等公民，是否含 `native/` 子目录区分实现方式（Rust vs 纯 TS），不构成分类

## 开发命令

```bash
bun install                  # 安装依赖
bun run tauri:dev            # 开发模式（sync → lint → 启动）
./deploy.sh                  # 打包部署
bun run sync:extensions      # 同步扩展注册（扫描 → 生成 extensions.rs）
bun run check:extensions     # CI 校验（extensions.rs 是否已同步）
bun run lint                 # Prettier + ESLint（含 UnoCSS class 排序）
bun run typecheck            # vue-tsc 严格类型检查
```

内部命令（tauri.conf.json 自动调用）：`bun run dev`（Vite）、`bun run build`（sync → lint → typecheck → vite build）

## 自动化测试

Co-location：`*.test.ts` 同目录；Rust `#[cfg(test)]` 内联。

```bash
bun run test                       # 前端（Vitest + happy-dom）
bun run test:watch                 # 前端监听
bun run test:e2e                   # E2E（Playwright）
cd src-tauri && cargo test --lib   # Rust
```

E2E 对 Vite dev server。原生窗口行为（快捷键/焦点/隐藏）仍需人工验证。

## 开发扩展

所有扩展同构：`extensions/<id>/index.ts`（`registerModule`）+ 可选 `native/mod.rs`（Rust 后端）。

含 native/ 的扩展（10 个）：clipboard、screenshot、awake、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search、ip

纯 TS 扩展（2 个）：calculator、settings

**Rust 端注册**：双注册——`configure_app!` 宏（编译期命令注册，sync-extensions 自动扫描 `#[tauri::command]`）+ `Extension` trait（运行时 `on_setup`/`on_teardown` 钩子，在 `lib.rs` 的 `ExtensionRegistry` 注册）。

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)、[agent](docs/extensions/agent.md)。

## 架构要点

**前后端通信**：前端用裸 `invoke()` + 手写类型（`src/bindings.ts` 提供命令名常量）。流式/事件用 `app.emit()` 或 `tauri::ipc::Channel<T>`（agent 用后者）。所有 Command 须在 `configure_app!` 注册（sync-extensions 自动扫描）。含动态 JSON 的 Command（如 agent_run 的 `Channel<AgentEvent>`）手写 TS 类型（`src/types/agent.ts`）。

**模块子视图**：`open_module_subview(moduleId, subviewId, payload)` → Rust 显示主窗口 + 发 `open-module-subview` 事件 → App.vue 激活模块、调用 `onOpenSubview`。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。`platform/panel::convert_to_panel` 转 `NonactivatingPanel`，显示不抢 NSApp active，关闭时 `platform/focus::restore_captured()` 还给原应用（PREV_FRONT_PID 唯一源在 `platform/focus.rs`）。

**全局快捷键**：`runtime/shortcut.rs`，快捷键 id 驱动（前端传 id + shortcut，Rust 自管注册表 + 录制模态 + 扩展钩子）。

**Agent 引擎**（`extensions/agent/native/engine/`）：tool calling loop，服务 agent 扩展。prompt/max_turns 由扩展 config 注入（非框架硬编码）。

- `loop_runner.rs`：主循环 `run_loop`：调 LLM → 解析 tool_calls → 审批 → 执行 → 回灌 → 下一轮
- `approval.rs`：`ApprovalManager`（全局 State，oneshot channel）
- `cancellation.rs`：`SessionRegistry`（per-session CancellationToken）
- `secret_scrub.rs`：gitleaks 风格正则打码
- `tool_registry.rs`：`AgentTool` trait + `ToolRegistry`

Agent 安全防线（命令执行 9 层纵深防御）详见 `docs/extensions/agent.md`。

**搜索**：Rust 端只做数据召回，过滤排序统一在前端走 `src/utils/fuzzy.ts::scoreFields()`（基于 [pinyin-pro](https://github.com/zh-lx/pinyin-pro)，三开关锁死中文缩写/全拼/ü→v 语义）。

```typescript
SearchResult { id, title, module; description?; icon?; score?; shortcut?; data?: { path?, kind?, icon?, ... } }
```

`kind` 权重：`application` > `folder`/`file` > `module` > `clipboard`

**UI 槽位**：`view`（主视图）、`searchBarAccessory`（搜索栏右侧）、`subviews`（命名子视图）。槽位组件 `Actions` 后缀。

**状态栏**：框架层全局组件 `StatusBar`。扩展通过 `copyAndHide` / `copyAndShow`（`src/utils/clipboard.ts`）自动获得「已复制」反馈。模块可通过 `AppModule.enterHint` / `multiSelectHint` / `deleteHint` 自定义快捷键提示。

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口
├── extensions.rs       # 自动生成（configure_app! 宏，勿手改）
├── http.rs             # 全局 HTTP 客户端
├── runtime/            # 运行时核心
│   ├── constants.rs    # 语义常量（搜索权重等）
│   ├── window.rs       # 主窗口 show/hide
│   ├── shortcut.rs     # 快捷键 + 录制
│   ├── storage.rs      # 存储路径常量
│   ├── permission.rs   # 系统权限薄壳
│   ├── registry.rs     # Extension trait + ExtensionRegistry
│   └── llm/            # LLM 基础设施（types / security / client / parser）
└── platform/           # macOS 原生桥
    ├── panel.rs        # NSPanel 转换
    ├── skylight.rs     # Space 迁移（私有 API）
    ├── focus.rs        # 焦点管理（PREV_FRONT_PID 唯一源）
    ├── input.rs        # CGEvent 键盘注入（统一）
    ├── pasteboard.rs   # NSPasteboard 操作（统一）
    ├── selection.rs    # AX 选中文本提取
    ├── click_monitor.rs
    ├── permission.rs
    └── path_guard.rs   # 统一路径校验

src/
├── components/
│   ├── ui/             # 原子组件（只用这些，禁止手写底层标签）
│   └── layout/         # MainView / ContentView / StatusBar
├── composables/
├── core/               # module-registry / module-helpers / async-view
├── stores/             # app / settings / update
├── types/              # module / agent
└── utils/
```

新增文件按所属模块归位，勿新建顶层分类。

## UI 规范

UnoCSS + TailwindCSS 最佳实践，遵循官方规范。

只用 `@/components/ui/` 原子组件，禁止手写底层标签。主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint`。

**禁止 arbitrary 值**：class 中禁止使用 `[10px]`、`[#ff3b30]` 等方括号任意值。颜色用 Tailwind 预设色或主题语义色 + 透明度修饰；尺寸/间距用预设档位。无合适预设时在 `uno.config.ts` theme 中定义。

**写法规范**：原生 HTML 元素使用 Attributify 模式，Vue 组件 props 保持 `class`。

**Attributify 禁用属性**：`animate` 等与 DOM 原生属性同名的特性禁止用 Attributify，必须用 `class="animate-spin"`。

**Shortcuts**：`ui-ctrl`、`ui-disabled`、`ui-active`、`flex-center`、`flex-col-full`、`flex-col-full-pb`、`form-label`、`input-base`、`action-footer`、`form-field`、`group-header`、`overlay-abs`

## 存储结构

扩展自管数据一律放各自 `extensions/<id>/`，无共享 `data/` 目录。

```
~/Library/Application Support/com.litiantao.voidnix/
├── config/settings.json              # 全局配置
└── extensions/
    ├── clipboard/clipboard.db        # 剪贴板历史（SQLite WAL）
    ├── calculator/calc_history.json  # 计算器历史
    ├── finder-ext/commands/          # Finder 扩展 IPC
    ├── zsh-autosuggestions/          # zsh 补全（bin/ index.cache signals.log enabled）
    └── awake/                        # awake binary（app_data_dir，非 /tmp）
```

icon 缓存已消除（实时提取，零磁盘文件）。dev 镜像 `com.litiantao.voidnix.dev` 同构。

## 约定

- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- `isTauri()` 判断环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，不写详情，不主动执行 git 操作
- 文档不用表格，言简意赅
- 修改代码后必须同步更新 AGENTS.md 或对应 docs/ 文档中相关描述
- 自开发自用，极简主义、强迫症、精神洁癖，开发秉承彻底、一步到位的理念，不考虑任何兼容性或历史包袱
