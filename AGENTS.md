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
bun run test                       # 前端（Vitest + happy-dom，src/ + extensions/）
bun run test:watch                 # 前端监听
bun run test:e2e                   # E2E（Playwright）
cd src-tauri && cargo test --lib   # Rust
```

E2E 对 Vite dev server。原生窗口行为（快捷键/焦点/隐藏）仍需人工验证。

## 开发扩展

所有扩展同构：`extensions/<id>/index.ts`（`registerModule`）+ 可选 `config.ts`（`defineConfig`）+ 可选 `native/mod.rs`（Rust 后端）。

含 native/ 的扩展（9 个）：clipboard、screenshot、awake、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search

纯 TS 扩展（7 个）：calculator、settings、ip、base64、time、uuid、currency

**Rust 端注册**：双注册——`init() -> TauriPlugin<Wry>`（编译期，各扩展 `native/mod.rs` 内 `invoke_handler(generate_handler![...])` 局部注册命令 + plugin `.setup` 管理命令执行 State）+ `Extension` trait（运行时 `setup`/`teardown` 异步钩子，并行 bootstrap via `join_all`，在 `lib.rs` 的 `ExtensionRegistry` 注册）。框架命令（permission/shortcut/window/pasteboard 共 14 个）在 `lib.rs` 手写 `generate_handler!`，不参与 sync-extensions 扫描。`configure_app!` 宏仅 `.plugin()` 链，零 `generate_handler!`。

**扩展配置**：每个扩展通过 `config.ts` 的 `defineConfig('extId', { ...defaults })` 自管配置，`reactive()` + `watch()` 自动持久化至 `extensions/<id>/config.json`。框架级配置（全局快捷键 + AI Provider）在 `stores/settings.ts`。

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)、[agent](docs/extensions/agent.md)。

## 架构要点

**前后端通信**：前端用裸 `invoke()` + 手写类型（`src/bindings.ts` 提供命令名常量）。流式/事件用 `app.emit()` 或 `tauri::ipc::Channel<T>`（agent 用后者）。扩展 Command 在各 `init()` 局部注册；框架 Command 在 `lib.rs` 手写 `generate_handler!`。含动态 JSON 的 Command（如 agent_run 的 `Channel<AgentEvent>`）手写 TS 类型（`src/types/agent.ts`）。

**模块接口**：`AppModule` 拆分为 5 组合接口——`ModuleMeta`（元数据）、`ModuleUI`（视图槽位）、`ModuleSearch`（搜索能力）、`ModuleLifecycle`（生命周期钩子）、`ModuleHints`（状态栏提示）。扩展按需组合。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。`platform/panel::convert_to_panel` 转 `NonactivatingPanel`，显示不抢 NSApp active，关闭时 `platform/focus::restore_captured()` 还给原应用（PREV_FRONT_PID 唯一源在 `platform/focus.rs`）。

**全局快捷键**：`runtime/shortcut.rs`，快捷键 id 驱动（前端传 id + shortcut，Rust 自管注册表 + 录制模态 + 扩展钩子）。

**Agent 引擎**（`extensions/agent/native/engine/`）：tool calling loop，服务 agent 扩展。prompt/max_turns/trustedCommands/forbiddenCommands/blockedArgs/资源上限由扩展 config 注入（非框架硬编码）。

- `loop_runner.rs`：主循环 `run_loop`：调 LLM → 解析 tool_calls → 审批 → 执行 → 回灌 → 下一轮
- `approval.rs`：`ApprovalManager`（全局 State，oneshot channel）
- `cancellation.rs`：`SessionRegistry`（per-session CancellationToken）
- `trim.rs`：历史消息裁剪（下沉自 runtime/llm）
- `secret_scrub.rs`：gitleaks 风格正则打码
- `tool_registry.rs`：`AgentTool` trait + `ToolRegistry`

Agent 安全防线（命令执行 9 层纵深防御）：`extensions/agent/native/policy.rs` 是 floor/cap 权威源（FORBIDDEN_FLOOR 31 项 / DENIED_ARG_FLOOR 15 项 / 资源上限 clamp），`agent_run` 入口强制 clamp/并集（不信任前端传值）；TS 端 `config.ts` 的 `BOUNDS` 仅 UI 镜像。详见 `docs/extensions/agent.md`。

**搜索**：Rust 端只做数据召回，过滤排序统一在前端走 `src/utils/fuzzy.ts::scoreFields()`（基于 [pinyin-pro](https://github.com/zh-lx/pinyin-pro)，三开关锁死中文缩写/全拼/ü→v 语义）。

```typescript
SearchResult { id, title, module; description?; icon?; score?; shortcut?; data?: { path?, kind?, icon?, ... } }
```

`kind` 权重：`application` > `folder`/`file` > `module` > `clipboard`

**UI 槽位**：`view`（主视图）、`searchBarAccessory`（搜索栏右侧）、`subviews`（命名子视图）。槽位组件 `Actions` 后缀。

**状态栏**：框架层全局组件 `StatusBar`。扩展通过 `copyAndHide` / `copyAndShow`（`src/utils/clipboard.ts`）自动获得「已复制」反馈。模块可通过 `ModuleHints.enterHint` / `multiSelectHint` / `deleteHint` 自定义快捷键提示。

**LLM 基础设施**（`runtime/llm/`）：agent + translate 扩展共享。`types.rs`（LlmMessage）、`client.rs`（StreamConfig/stream_openai_request + SSRF 防护 validate_ai_request + 消息截断 + 请求管道常量）、`parser.rs`（tool_calls 解析）。

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口
├── extensions.rs       # 自动生成（仅 .plugin() 链 + mod 声明，零 generate_handler!）
├── http.rs             # 全局 HTTP 客户端
├── runtime/            # 运行时核心
│   ├── constants.rs    # 空壳（目标删除，见 RV §3.1；语义常量仅前端 constants.ts）
│   ├── window.rs       # 主窗口 show/hide
│   ├── shortcut.rs     # 快捷键 + 录制
│   ├── storage.rs      # TempHandle RAII 临时文件管理（Drop 自动清理 + 全局注册表）
│   ├── permission.rs   # 系统权限薄壳
│   ├── registry.rs     # Extension trait + ExtensionRegistry（并行 bootstrap）
│   └── llm/            # LLM 基础设施（types / security / client / parser）
└── platform/           # macOS 原生桥
    ├── panel.rs        # NSPanel 转换
    ├── skylight.rs     # Space 迁移（私有 API）
    ├── focus.rs        # 焦点管理（PREV_FRONT_PID 唯一源）
    ├── input.rs        # CGEvent 键盘注入（统一 post_key/inject_copy/paste_global）
    ├── pasteboard.rs   # NSPasteboard 统一（read_text/read_file_url/read_png/write_text/clear/set_string/set_file_url/set_png/set_custom/snapshot/restore + pasteboard_write_text 命令）
    ├── selection.rs    # AX 选中文本提取 + poll_clipboard
    ├── click_monitor.rs
    ├── permission.rs
    └── path_guard.rs   # 统一路径校验

src/
├── runtime/
│   └── storage.ts      # defineConfig + defineExtensionConfig（扩展自管配置）
├── components/
│   ├── ui/             # 原子组件（只用这些，禁止手写底层标签）
│   └── layout/         # MainView / ContentView / StatusBar / ResultIcon
├── composables/
├── core/               # module-registry / module-helpers / async-view
├── stores/             # app / settings（仅框架级）/ update
├── types/              # module（5 组合接口）/ agent
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
├── config/settings.json              # 框架级配置（快捷键 + AI Provider）
└── extensions/
    ├── clipboard/clipboard.db        # 剪贴板历史（SQLite WAL）
    ├── clipboard/config.json         # clipboard 扩展配置（defineConfig）
    ├── calculator/calc_history.json  # 计算器历史
    ├── finder-ext/commands/          # Finder 扩展 IPC
    ├── zsh-autosuggestions/          # zsh 补全（bin/ index.cache signals.log enabled）
    ├── awake/                        # awake binary + config.json
    ├── screenshot/config.json        # screenshot 扩展配置
    ├── window-manager/config.json    # window-manager 扩展配置
    ├── translate/config.json         # translate 扩展配置
    ├── agent/config.json             # agent 扩展配置
    ├── zsh-autosuggestions/config.json
    └── finder-ext/config.json
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
