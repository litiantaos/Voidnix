# Voidnix

macOS 效率启动器。Tauri 2 + Rust + Vue 3。统一扩展架构：

- **框架**（`src-tauri/src/runtime/` + `platform/`）：运行时核心 + macOS 原生桥，零业务语义
- **扩展**（`extensions/<name>/`）：全部一等公民，是否含 `native/` 子目录区分实现方式（Rust vs 纯 TS），不构成分类

## 原则

- 自开发自用，追求极致的结构清晰、代码统一、优雅、轻量、高性能、低占用
- 极简主义、强迫症、精神洁癖、第一性推导、一步到位，摒弃历史包袱与旧版兼容
- **两层正交**：`runtime/`（平台无关的调度与生命周期核心）与 `platform/`（macOS 原语、无业务语义）严格分离——换 `platform` 实现即可跨平台，不混称「内核」
- **机制最少化**：新增机制（接口字段/扩展点/生命周期钩子）前先回答「现有机制能否覆盖」。优先扩展已有机制参数，而非新增并列机制
- **扩展自治**：扩展 = 元数据 + 能力供给 + 生命周期，声明供给什么能力框架按需消费（未供给即不支持，零默认值）；每个扩展目录是自治单元，零跨扩展 import、零框架业务泄漏

## 开发命令

```bash
bun install                  # 安装依赖
bun run dev                  # 仅启动 Vite dev server（前端独立调试，非 Tauri）
bun run tauri:dev            # 开发模式（build:zsh-bin → sync:extensions → check:drift → 启动）
bun run build                # 生产构建（check:drift → lint → typecheck → vite build）
./deploy.sh                  # 打包部署
bun run build:zsh-bin        # 单独编译 zsh-autosuggestions binary（产物 target/debug/）
bun run lint                 # Prettier + ESLint（含 UnoCSS class 排序）
bun run lint:check           # 只读校验（CI 用，不写）
bun run typecheck            # vue-tsc 严格类型检查
bun run sync:extensions      # 同步扩展注册（扫描 → 生成 extensions.rs）
bun run check:drift          # 漂移校验聚合（= check:extensions + commands + agent-bounds + wm-bounds）
bun run check:extensions     # CI 校验（extensions.rs 同步 + windowViews 漂移）
bun run check:commands       # CI 校验（Rust #[tauri::command] ↔ commands.ts 双向差集）
bun run check:agent-bounds   # CI 校验（agent policy.rs ↔ config.ts BOUNDS 双向一致）
bun run check:wm-bounds      # CI 校验（window-manager mod.rs ↔ config.ts BOUNDS 双向一致）
```

Rust 端代码质量：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check    # 格式检查（CI 门禁）
cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings   # lint（CI 门禁）
# zsh binary 是独立 crate（与 Voidnix package 分离 + 独立 target 目录，避免 Voidnix 编译截断 binary）
cargo clippy --manifest-path extensions/zsh-autosuggestions/native/Cargo.toml -- -D warnings
```

内部命令（tauri.conf.json 自动调用）：`bun run dev`（Vite）、`bun run build`（check:drift → lint → typecheck → vite build）。
`bun run tauri:dev` 前置 `build:zsh-bin + sync:extensions + check:drift`，命令名/安全边界漂移在 dev 即暴露（风格校验由 CI `lint:check` 门禁，dev 不写盘）。

## 自动化测试

Co-location：`*.test.ts` 同目录；Rust `#[cfg(test)]` 内联。

```bash
bun run test                       # 前端（Vitest + happy-dom，src/ + extensions/）
bun run test:watch                 # 前端监听
bun run test:e2e                   # E2E（Playwright）
cd src-tauri && cargo test --lib   # Rust
```

E2E 对 Vite dev server（CI 自动执行 `bunx playwright install` + `bun run test:e2e`）。原生窗口行为（快捷键/焦点/隐藏）仍需人工验证。

## CI 门禁

`.github/workflows/ci.yml` 在 push（main/refactor/v2）与任意 PR 触发，依次执行：

1. 前端 lint:check（Prettier + ESLint）
2. Rust `cargo fmt --check`
3. `bun run typecheck`（vue-tsc 严格）
4. Rust `cargo clippy --lib -- -D warnings`
5. 漂移校验四件套：`check:extensions` / `check:commands` / `check:agent-bounds` / `check:wm-bounds`
6. 单测：`bun run test`（Vitest）+ `cargo test --lib`
7. E2E：`bun run test:e2e`（Playwright，含浏览器安装）

## 开发扩展

所有扩展同构（`extensions/<id>/index.ts` + 可选 `config.ts` + 可选 `native/`），详见 [docs/extensions.md](docs/extensions.md)。

含 native/（9）：clipboard、screenshot、awake、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search
纯 TS（7）：calculator、settings、ip、base64、time、uuid、currency

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)、[agent](docs/extensions/agent.md)。

## 架构要点

**前后端通信**：前端命令名常量集中 `src/commands.ts`（`CMD.xxx`），**禁止裸 `invoke('xxx')`**，统一走 `invoke<T>(CMD.xxx, {...})` + 手写类型（`types/` 与各扩展）。`scripts/check-commands.ts` CI 对 Rust `#[tauri::command]` 名集合 ↔ `commands.ts` 常量作双向差集校验。流式/事件用 `app.emit()` 或 `tauri::ipc::Channel<T>`（agent 用后者）。扩展 Command 与框架 Command 统一在 `configure_app!` 的 `generate_handler!` 全局注册（sync-extensions 扫描生成，前端裸名 invoke）。含动态 JSON 的 Command（如 agent_run 的 `Channel<AgentEvent>`）手写 TS 类型（`src/types/agent.ts`）。

**扩展接口**：`Extension`（`src/runtime/types.ts`）= `meta` + 14 槽（11 能力槽 + 3 行为槽，按需声明，均有真实消费者）+ `setup` 生命周期。槽位语义与消费者计数详见 [docs/extensions.md](docs/extensions.md)。

**搜索引擎**（`src/runtime/search-engine.ts`）：单通道 dynamic 并行召回 + keyword 合流 + dedupe + groupAndSort。全局模式聚合所有扩展按 `finalScore = fuzzy + boost` 过滤排序；模块模式只调激活扩展且保留原序。`SearchContext.moduleMode` 区分两种模式（网络型扩展据此在全局空 query 跳过网络）。搜索集成细节详见 [docs/extensions.md](docs/extensions.md)。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。`platform/panel::convert_to_panel` 转 `NonactivatingPanel`，显示不抢 NSApp active，关闭时 `platform/focus::restore_captured()` 还给原应用（PREV_FRONT_PID 唯一源在 `platform/focus.rs`）。

**全局快捷键**：`runtime/shortcut.rs`，快捷键 id 驱动（前端传 id + shortcut，Rust 自管注册表 + 录制模态 + 扩展钩子）。

**Agent 引擎**（`extensions/agent/native/engine/`）：tool calling loop，服务 agent 扩展。prompt/max_turns/trustedCommands/forbiddenCommands/blockedArgs/资源上限由扩展 config 注入（非框架硬编码）。

- `loop_runner.rs`：主循环 `run_loop`：调 LLM → 解析 tool_calls → 审批 → 执行 → 回灌 → 下一轮
- `approval.rs`：`ApprovalManager`（全局 State，oneshot channel）
- `cancellation.rs`：`SessionRegistry`（per-session CancellationToken）
- `trim.rs`：历史消息裁剪（下沉自 runtime/llm）
- `secret_scrub.rs`：gitleaks 风格正则打码
- `tool_registry.rs`：`AgentTool` trait + `ToolRegistry`

Agent 安全防线（命令执行 9 层纵深防御）：`extensions/agent/native/policy.rs` 是 floor/cap 权威源（FORBIDDEN_FLOOR 31 项 / DENIED_ARG_FLOOR 19 项 / TRUSTED_DENYLIST 11 项强制剔除 / 资源上限 clamp），`agent_run` 入口强制 clamp/并集/剔除（不信任前端传值——老用户 config.json 已落盘的 find/awk/... 也会被剔除）；TS 端 `config.ts` 的 `BOUNDS` 仅 UI 镜像。详见 `docs/extensions/agent.md`。

**搜索打分**：`src/utils/fuzzy.ts::scoreFields()`（[pinyin-pro](https://github.com/zh-lx/pinyin-pro)，三开关锁死中文缩写/全拼/ü→v 语义），权重读 `runtime/constants.ts::SEARCH.WEIGHTS`。`kind` 枚举 `application | folder | file | module | clipboard | web`（folder/file 同组），组间序 `GROUP_ORDER`：`application > file > module > clipboard > web`。

**状态栏**：框架层全局组件 `StatusBar`。扩展通过 `copyAndHide`（`stores/app.ts`，app 行为：写剪贴板 + showStatus 反馈 + 延迟隐藏窗口）自动获得「已复制」反馈。`showStatus(msg, opts?)` 支持 `kind: 'success' | 'error'`（默认 success），StatusBar 按 kind 切图标/颜色（对勾 accent / 警告 red-500），错误反馈必须传 `kind: 'error'` 避免绿色对勾的语义错位。扩展可通过 `hints.enter` / `hints.multiSelect` / `hints.delete` 自定义快捷键提示。

**模块视图加载**（切换性能）：模块 View（mainView/subviews/searchBarAccessory）静态 import 进主 bundle（用户高频、固定集合，首次进入零卡顿）；仅**独立窗口**（screenshot 标注 host/pin、window-manager snap 面板，`windowViews`）保留 `defineAsyncComponent` 真按需——不截图/不分屏不加载，省稳态占用（gzip ~20KB）。`ContentView` 用 `KeepAlive`（max 覆盖全部视图 key）缓存已访问模块，切换走 activate/deactivate 而非重挂载。

**LLM 基础设施**（`runtime/llm/`）：agent + translate 扩展共享。`types.rs`（LlmMessage）、`client.rs`（StreamConfig/stream_openai_request + SSRF 防护 validate_ai_request + 消息截断 + 请求管道常量）、`parser.rs`（tool_calls 解析）。

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口（lib.rs setup 内含启动埋点，debug 构建打印 `[boot]` 各阶段耗时 + <100ms 判定）
├── extensions.rs       # 自动生成（configure_app! 含 .plugin() 链 + 全局 generate_handler! + mod 声明）
├── http.rs             # 全局 HTTP 客户端 + http_get 命令（浏览器 UA 伪装 + SSRF 防护 + 重定向限制 + 共享 parse_scheme_host/is_blocked_host 原语；ip/currency 等纯 TS 扩展消费）
├── runtime/            # 运行时核心
│   ├── window.rs       # 主窗口 show/hide
│   ├── shortcut.rs     # 快捷键 + 录制
│   ├── storage.rs      # TempHandle RAII + cleanup_all_voidnix_temps（启动期统一扫 /tmp 残留）+ ext_data_dir 统一扩展数据目录 + save_png_safely（path_guard + write 共用）
│   ├── permission.rs   # 系统权限薄壳
│   ├── registry.rs     # Extension trait + ExtensionRegistry（concurrent bootstrap，join_all 单线程并发交错）
│   ├── pasteboard.rs   # 框架命令薄壳（pasteboard_write_text；原语在 platform/pasteboard）
│   └── llm/            # LLM 基础设施（types / client / parser；security 溶解入 client）
└── platform/           # macOS 原生桥
    ├── panel.rs        # NSPanel 转换
    ├── skylight.rs     # Space 迁移（私有 API）
    ├── focus.rs        # 焦点管理（PREV_FRONT_PID 唯一源）
    ├── input.rs        # CGEvent 键盘注入（post_key 原语 + post_combo 字符串糖；Modifier 枚举 + Option pid）
    ├── pasteboard.rs   # NSPasteboard 原语统一（read_text/read_file_url/read_png/write_text/clear/set_string/set_file_url/set_png/set_custom/snapshot/restore）
    ├── selection.rs    # AX 选中文本提取 + poll_clipboard
    ├── click_monitor.rs
    ├── permission.rs
    ├── window.rs       # 主窗口原生操作（NSWindow show/hide/key + 圆角 + NSOpenPanel）
    └── path_guard.rs   # 统一路径校验

src/
├── main.ts             # 入口（import.meta.glob eager 扫描扩展 + 并行 setup）
├── commands.ts         # 命令名常量（CMD.xxx，禁止裸 invoke）
├── runtime/            # 前端运行时（5 文件）
│   ├── types.ts        # Extension / SearchProvider / SearchResult（12 槽：9 能力 + 3 行为）
│   ├── constants.ts    # 语义常量单一源（SEARCH.WEIGHTS/GROUP_ORDER/GROUP_TITLES/KEYWORD_MODULE_BOOST + LIMITS）
│   ├── storage.ts      # defineConfig（storePath + defaults；reactive + watch 自动持久化 + 递归 deepEqual race 保护 + 类型守卫 + isLoading 抑制 + 退出 flush + 跨窗口 onChange 同步 + store 实例缓存）
│   ├── extension-registry.ts  # defineExtension + getAllExtensions + getExtension
│   └── search-engine.ts       # dynamic 单通道 + keyword 合流 + dedupe + groupAndSort
├── components/
│   ├── ui/             # 原子组件（只用这些，禁止手写底层标签）
│   └── layout/         # MainView / ContentView / StatusBar / ResultIcon
├── composables/
│   ├── useAppLifecycle.ts     # 主窗口生命周期（快捷键注册/失焦隐藏/模块事件，抽自 App.vue）
│   ├── useSearchInput.ts      # 搜索编排（全局 searchEngine + 搜索型模块 dynamic + web 搜索// + 工具列表/ + 默认结果）
│   ├── useResultNavigation.ts # 结果键盘导航 + 执行分派
│   ├── useFloating.ts / useScrollPosition.ts / useTauriListener.ts  # 通用工具
│   └── events.ts / useInputControl.ts / useShortcutConfig.ts / useSettingsInput.ts
├── stores/             # app / settings（仅框架级）/ update
├── types/              # agent（手写 LLM/Agent 类型）
└── utils/
```

新增文件按所属模块归位，勿新建顶层分类。

## UI 规范

- **总体要求**：UnoCSS + TailwindCSS 最佳实践，遵循官方规范。

- **原子组件**：只用 `@/components/ui/` 原子组件，禁止手写底层标签。主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint`。

- **慎用 arbitrary 值**：class 中务必使用 Tailwind 预设值或 Uno 主题值，非必要禁止使用 `[10px]`、`[#ff3b30]` 等方括号任意值，除非是单一特殊场景。无合适预设时在 `uno.config.ts` theme 中定义。

- **写法规范**：原生 HTML 元素使用 Attributify 模式，Vue 组件 props 保持 `class`。

- **Attributify 禁用属性**：`animate` 等与 DOM 原生属性同名的特性禁止用 Attributify，必须用 `class="animate-spin"`。

- **Shortcuts**：`ui-ctrl`、`ui-disabled`、`ui-active`、`flex-center`、`flex-col-full`、`flex-col-full-pb`、`form-label`、`input-base`、`action-footer`、`form-field`、`group-header`、`overlay-abs`

## 存储结构

扩展自管数据一律放各自 `extensions/<id>/`，无共享 `data/` 目录。

```
~/Library/Application Support/com.litiantao.voidnix/
├── config/settings.json              # 框架级配置（快捷键 + AI Provider，defineConfig 扁平 schema）
└── extensions/
    ├── clipboard/{clipboard.db, clipboard.db-wal, config.json}   # 剪贴板历史（SQLite WAL，写入计数达 200 触发 wal_checkpoint(TRUNCATE)）+ 配置
    ├── calculator/config.json        # 计算器历史（history key，10 条上限；走 defineConfig）
    ├── finder-ext/{commands/, config.json}     # Finder 扩展 IPC 目录 + 配置
    ├── zsh-autosuggestions/{bin/, index.zsh, signals.log, bin.version, config.json}  # zsh 补全
    ├── awake/{Display Wakelock, config.json}   # awake binary + 配置
    ├── screenshot/config.json        # screenshot 扩展配置
    ├── window-manager/config.json    # window-manager 扩展配置
    ├── translate/config.json         # translate 扩展配置
    └── agent/config.json             # agent 扩展配置
```

icon 缓存已消除（实时提取，零磁盘文件）。dev 镜像 `com.litiantao.voidnix.dev` 同构。

所有 config.json 均走 `defineConfig`（`src/runtime/storage.ts`）：reactive + watch + 300ms 防抖 + 深克隆 + race 保护 + 类型守卫 + 跨窗口 onChange 同步 + 退出 flush。schema 变更时手动删磁盘 config.json 即可（自开发自用，不维护迁移）。

## 约定

- 环境：`isTauri()` 判断环境，非 Tauri 跳过原生调用
- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，描述力求最简，不写详情，不主动执行 git 操作
- 语言：注释和回复用中文
- 文档：不用表格，言简意赅，修改代码后必须同步更新 AGENTS.md 或对应 docs/ 文档中相关描述
