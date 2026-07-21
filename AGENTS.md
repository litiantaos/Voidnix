# Voidnix

macOS 效率启动器。Tauri 2 + Rust + Vue 3。统一扩展架构：

- **框架**（`src-tauri/src/runtime/` + `platform/`）：运行时核心 + macOS 原生桥，零业务语义
- **扩展**（`extensions/<name>/`）：全部一等公民，是否含 `native/` 子目录区分实现方式（Rust vs 纯 TS），不构成分类

## 原则

- 自开发自用，追求极致的结构清晰、代码统一、优雅、轻量、高性能、低占用
- 极简主义、强迫症、精神洁癖、第一性推导、一步到位，不考虑历史包袱与旧版兼容，拒绝心智负担
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
bun run check:drift          # 漂移校验聚合（extensions + commands + agent-bounds + wm-bounds + extension-orders）
bun run check:extensions     # CI 校验（extensions.rs 同步 + windowViews 漂移）
bun run check:commands       # CI 校验（Rust #[tauri::command] ↔ commands.ts 双向差集）
bun run check:agent-bounds   # CI 校验（agent 资源上限 policy.rs ↔ config.ts BOUNDS 双向一致）
bun run check:wm-bounds      # CI 校验（window-manager mod.rs ↔ config.ts BOUNDS 双向一致）
bun run check:extension-orders # CI 校验（非 hidden 扩展 meta.order 唯一）
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
5. 漂移校验：`check:extensions` / `check:commands` / `check:agent-bounds` / `check:wm-bounds` / `check:extension-orders`
6. 单测：`bun run test`（Vitest）+ `cargo test --lib`
7. E2E：`bun run test:e2e`（Playwright，含浏览器安装）

## 开发扩展

所有扩展同构（`extensions/<id>/index.ts` + 可选 `config.ts` + 可选 `native/`），详见 [docs/extensions.md](docs/extensions.md)。

含 native/（14）：clipboard、screenshot、video、awake、clean-mode、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search、proxy、system-status、ai-providers
纯 TS（7）：calculator、settings、ip、base64、time、uuid、currency

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)、[agent](docs/extensions/agent.md)、[ai-providers](docs/extensions/ai-providers.md)、[clean-mode](docs/extensions/clean-mode.md)、[proxy](docs/extensions/proxy.md)、[video](docs/extensions/video.md)、[finder-ext](docs/extensions/finder-ext.md)、[window-manager](docs/extensions/window-manager.md)。

## 架构要点

**前后端通信**：前端命令名常量集中 `src/commands.ts`（`CMD.xxx`），**禁止裸 `invoke('xxx')`**，统一走 `invoke<T>(CMD.xxx, {...})` + 手写类型（`types/` 与各扩展）。`scripts/check-commands.ts` CI 对 Rust `#[tauri::command]` 名集合 ↔ `commands.ts` 常量作双向差集校验。流式/事件用 `app.emit()` 或 `tauri::ipc::Channel<T>`（agent 用后者）。扩展 Command 与框架 Command 统一在 `configure_app!` 的 `generate_handler!` 全局注册（sync-extensions 扫描生成，前端裸名 invoke）。含动态 JSON 的 Command（如 agent_run 的 `Channel<AgentEvent>`）手写 TS 类型（`src/types/agent.ts`）。

**扩展接口**：`Extension`（`src/runtime/types.ts`）= `meta` + 14 槽（11 能力槽 + 3 行为槽，按需声明，均有真实消费者）+ `setup` 生命周期。槽位语义与消费者计数详见 [docs/extensions.md](docs/extensions.md)。

**搜索引擎**（`src/runtime/search-engine.ts`）：单通道 dynamic 并行召回 + 一次预算 finalScore + keyword 合流 + dedupe + groupAndSort。**全局与搜索型模块共用 `search()`**（`setActiveModule` 切换模式；模块模式只调激活扩展、保留原序、同样 timeout/abort）。**模式快照**：`search()` 入口捕获 `activeModule`，await 期间切换不影响本次后处理。**超时**：每扩展独立 child `AbortSignal`，超时只 abort 该扩展（不牵连其它），父 signal abort 时同步取消。全局模式按 `finalScore = fuzzy + boost` 排序；**过滤规则**：空 query 默认列表按 `finalScore>0`（boost>0，主要是应用）显示；非空 query 查找型结果需 `fuzzy>0`，module 类即时答案靠 `finalScore>0` 穿透。keyword / `/` 工具列表模块入口打分共用 `scoreModuleEntry`（name/id/description 正向 + keywords 双向）。`SearchContext.moduleMode` 供扩展区分场景：全局即时答案仅 calculator / currency；ip / time / uuid / base64 等仅模块内响应。详见 [docs/extensions.md](docs/extensions.md)。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。`platform/window.rs::apply_main_window_style`（= `apply_mica_material(ns, 16)` + `setHasShadow(true)` + `convert_to_panel`）在 setup 内一次性配置主窗口样式（Mica + contentView 圆角 16 = `radius-window` + 原生阴影 + 冷雾 tint；见 [设计系统](docs/design.md)）。snap-panel 经 `apply_mica_material(ns, 10)` 对齐 `radius-panel`。`platform/panel::convert_to_panel` 转 `NonactivatingPanel`（点击/makeKey 不自动激活）；主窗 show **不** `activate_app`（保持原前台 active，避免聚焦视图/菜单栏突变；代价是 macOS 26 上偶发下层 hover 穿透——产品优先不打断），hit-test 仍靠 `capture_mouse_events` + SkyLight event shape；**show 时 `present_on_cursor_screen`**：光标屏居中并写 `PLACEMENT_VIS`；`animate_frame` 只在 `PLACEMENT_VIS` 内改尺寸（忽略前端 x/y，高度立即生效）。**hide 不 orderOut**（仅 alpha=0 + ignoresMouse + 去阴影）——orderOut 后副屏二次 show 坐标对也不绘；主窗 Space 只 Add；collectionBehavior=`CanJoinAllSpaces|FullScreenAuxiliary`（勿并 MoveToActiveSpace）；`hide_main` 走 `restore_captured()` 交还 first responder（PREV_FRONT_PID 唯一源在 `platform/focus.rs`）。截图 overlay 等独占场景才显式 `activate_app`。`is_app_active()` 判焦点归属：NSApp keyWindow 非空 → 焦点在我们；否则查 frontmost bundle 路径——`/System/` 开头（授权弹窗、keychain 对话框等）视为交互流未中断；第三道兜底 `OSASCRIPT_RUNNING` 标志（osascript 授权后续 shell 命令执行期间 frontmost 已还给原 app 但仍抑制 blur hide），抑制 blur hide。`restore_captured()` 还原前查 frontmost：第三方已接管（系统弹窗/用户切到其他 app）则不抢回。系统弹窗关闭后焦点恢复由 `platform/frontmost_watcher`（NSWorkspace 激活通知观察器，随 show/hide 生命周期 add/remove）处理：frontmost 变更时若 panel 可见但丢 key，frontmost == 原前台 PID → makeKeyWindow 恢复；frontmost != 原前台 PID → 用户主动切换 → emit `frontmost-changed` → 前端 dismiss。窗口高度统一机制：扩展声明 `windowHeight`（`number` 固定 / `'auto'` 自适应 / 未声明默认 480），subview 可经 `subviewHeights` 覆盖；`useModuleHeight`（MainView 全局唯一调用）读 `activeModule` + `activeSubview` 解析模式，一次 invoke 触发 Rust 端 `set_main_frame` → `platform/window.rs::animate_frame` 用 `NSAnimationContext` + `animator setFrame:display:animate:` 系统级动画（CoreAnimation 接管插值，非 JS rAF 逐帧，不每帧阻塞主线程/触发 WebView 重排）；`auto` 模式 ResizeObserver 监听 ContentView 的 `contentRef`（自然高量真实内容高），窗口高 = `CHROME_HEIGHT`（搜索栏高度 + 间距，scrollContainer paddingTop）+ 内容高，clamp `[DEFAULT_HEIGHT, 屏幕高 90%]`（屏幕尺寸走 `currentMonitor` 因 WKWebView 下 `window.screen` 仅返回 webview 视口）；底部将出屏（含 40px 间距）则同步上移保证完整可见，离开 auto 还原进入前位置。

**全局快捷键**：`runtime/shortcut.rs`，快捷键 id 驱动（前端传 id + shortcut，Rust 自管注册表 + 录制模态 + 扩展钩子）。默认 Option 基（`Option+Space` 呼出，`Option+C/S/T/A/F`：剪贴板 / 截屏 / 翻译 / Agent / 访达工具）；dev 构建（debug）注册时经 `cfg!(debug_assertions)` 自动叠加 `Shift`，与 prod（release）区分且可并存（dev/prod 数据目录仍按 bundle id 隔离，配置默认值一致）。

**菜单栏**：`runtime/menubar.rs`，框架唯一托盘图标（`public/bar_icon.png` + `icon_as_template` 深浅色自适应），左键弹聚合菜单。扩展在 Rust `setup` 内 `menubar::register(MenuBarContribution{ title, build, on_event })` 声明贡献段：`title`（分组标题，disabled 项渲染）、`build` 闭包返回 `Vec<MenuEntry>` 快照（`Item`/`CheckItem`/`Submenu`/`Separator`）、`on_event` 闭包收点击 id 自行过滤；状态变更后调 `menubar::refresh(&app)` 触发重建。镜像 `shortcut.rs` 的 hook 范式（`LazyLock<Mutex<Vec>>` + free function，`Arc<dyn Fn>` 锁外调用防 `on_event→refresh` 重入死锁）。**菜单按扩展 `title` 分组**（每段前插 disabled 标题项，段间分隔线）。**可见性 = Σ build() 项数 > 0**（空快照 = 该扩展当前不贡献；扩展全关图标自动隐藏）。Rust 侧能力（非 TS `Extension` 槽——菜单构建依赖 Rust State，纯 TS 扩展无此需求）。现 2 消费者：awake（保持系统唤醒：打开扩展 + 启用开关 + 显示模式二级菜单）、proxy（代理：打开扩展 + 已连接状态 CheckItem 可点断开「已连接：节点」；断开后图标隐藏，重连走扩展面板，其余控制全部在面板，统一 TUN 模式，详见 [proxy.md](docs/extensions/proxy.md)）。

**Agent 引擎**（`extensions/agent/native/engine/`）：tool calling loop，服务 agent 扩展。prompt/max_turns/资源上限由扩展 config 注入（非框架硬编码）。

- `loop_runner.rs`：主循环 `run_loop`：调 LLM → 解析 tool_calls → 执行 → 回灌 → 下一轮
- `cancellation.rs`：`SessionRegistry`（per-session CancellationToken；loop 结束 unregister，abort 走 cancel）
- `trim.rs`：历史消息裁剪（下沉自 runtime/llm）
- `secret_scrub.rs`：gitleaks 风格正则打码
- `tool_registry.rs`：`AgentTool` trait + `ToolRegistry`

Agent 命令执行：无审批、无白/黑名单，所有命令直接放行；`extensions/agent/native/policy.rs` 是资源上限 floor/cap 权威源（CPU/内存/文件描述符/超时/输出/轮次 clamp），`agent_run` 入口强制 clamp（不信任前端传值）；`run_command` 保留 `rm -rf /` 断路器兜底；TS 端 `config.ts` 的 `BOUNDS` 仅 CI 镜像（无 Settings UI）。详见 `docs/extensions/agent.md`。

**搜索打分**：`src/utils/fuzzy.ts::scoreFields()`（[pinyin-pro](https://github.com/zh-lx/pinyin-pro)，三开关锁死中文缩写/全拼/ü→v 语义），权重读 `runtime/constants.ts::SEARCH.WEIGHTS`。模块入口用 `scoreModuleEntry()`（name/id/description + `keywordMatch` 双向），全局 keyword 与 `/` 列表共用；**dynamic 产出相关 tool 型结果（kind=module，finalScore > 0）的扩展抑制其 keyword 入口**（即时答案优先；clipboard 等 kind≠module 不抑制）。`kind` 枚举 `application | folder | file | module | clipboard | web`，组间序 `GROUP_ORDER`：`application > module > file > clipboard > web`。组内限流：`LIMITS.maxGroupResults`（非 file）/ `maxFileResults`。

**toast 提示**：自研轻量浮层（`composables/useToast.ts` + `components/ui/ToastOverlay.vue`），按 `kind` 切图标/色（`success` accent 对勾 / `error` danger 语义色警告，错误反馈必须传 `kind: 'error'`）；堆叠上限 3 条、默认 2000ms 自动清除，窗口隐藏时立即清空（`hideWindow` 内调 `clearToasts`，规避 macOS 隐藏 WebView 节流 setTimeout 致残留）。扩展通过 `copyAndHide`（`stores/app.ts`，写剪贴板 + showStatus 反馈 + 延迟隐藏窗口）自动获得「已复制」反馈；`showStatus(msg, opts?)` 委托 `showToast`（调用点零改动）。搜索栏 placeholder：模块模式显示搜索说明（`在 X 中搜索` 或扩展自声明 `placeholder`），全局模式保留搜索说明。

**浮层组件**：`BaseDropdownItems`（`components/ui/`）通用行渲染器，4 行类型 `item | header | divider | meta`（`selectableIndices` 仅算 item，键盘导航天然跳过其余）；消费者传 `PanelItem[]` + `activeIndex`，emit `select/hover`。`ResultActionPanel`（`components/layout/`）：全局模式 `Cmd+Enter` 对 application/file/folder 结果的合并面板（上方详情 meta 行 + 下方动作 item 行——在 Finder 中显示 / 复制路径）；capture-phase keydown 劫持 + 外点关闭，打开即默认选中首项可连续 Enter 触发。

**模块视图加载**（切换性能）：模块 View（mainView/subviews/searchBarAccessory）静态 import 进主 bundle（用户高频、固定集合，首次进入零卡顿）；仅**独立窗口**（screenshot 标注 host/pin、window-manager snap 面板，`windowViews`）保留 `defineAsyncComponent` 真按需——不截图/不分屏不加载，省稳态占用（gzip ~20KB）。`ContentView` 用 `KeepAlive`（max 覆盖全部视图 key）缓存已访问模块，切换走 activate/deactivate 而非重挂载。

**LLM 基础设施**（`runtime/llm/`）：agent + translate 扩展共享。`types.rs`（LlmMessage）、`client.rs`（StreamConfig/stream_openai_request + SSRF 防护 validate_ai_request + 消息截断 + 请求管道常量）、`parser.rs`（tool_calls 解析）。

**AI 凭证中枢**（`src/runtime/ai-providers.ts` → `config/ai-providers.json`）：只存 URL/Key/模型，**无「使用中」**；选用由 agent（`providerModelKey`）/ translate（`selections`）自管。与中枢同步：`isCredentialSelectionValid` + 读时 effective（不写回）+ 启动/写入冷 prune；写 `ai.env`（`ZHIPU_*` / `DEEPSEEK_*` / `ANTHROPIC_*` 等）+ `shell_rc` 钩子。详见 [ai-providers.md](docs/extensions/ai-providers.md)。**Shell rc 注入**统一走 `runtime/shell_rc`（`# voidnix <scope>`），见 [shell-rc.md](docs/shell-rc.md)。

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口（lib.rs setup 内含启动埋点，debug 构建打印 `[boot]` 各阶段耗时 + <100ms 判定）
├── extensions.rs       # 自动生成（configure_app! + register_all 生命周期 + generate_handler! + mod 声明）
├── http.rs             # HTTP 客户端（HTTP_CLIENT 整体 120s 超时 + DOWNLOAD_CLIENT 无整体超时仅建连 30s 供流式大文件下载）+ http_get 命令（浏览器 UA 伪装 + SSRF 防护 + 重定向限制 + 共享 parse_scheme_host/is_blocked_host 原语；ip/currency 等纯 TS 扩展消费）
├── runtime/            # 运行时核心
│   ├── window.rs       # 主窗口 show/hide
│   ├── shortcut.rs     # 快捷键 + 录制
│   ├── menubar.rs      # 聚合菜单栏托盘（框架唯一图标 + 扩展贡献段注册 + 可见性 = Σ build() 项数 > 0）
│   ├── storage.rs      # TempHandle RAII + cleanup_all_voidnix_temps（启动期统一扫 /tmp 残留）+ ext_data_dir 统一扩展数据目录 + save_png_safely（path_guard + write 共用）
│   ├── permission.rs   # 系统权限薄壳
│   ├── registry.rs     # Extension trait + ExtensionRegistry（concurrent bootstrap；单扩展 setup 失败隔离）
│   ├── pasteboard.rs   # 框架命令薄壳（write_text / paste_text 写剪贴板并 Cmd+V；原语在 platform/pasteboard）
│   ├── shell_rc.rs     # .zshrc 等注入约定（# voidnix <scope> marker；见 docs/shell-rc.md）
│   └── llm/            # LLM 基础设施（types / client / parser；security 溶解入 client）
└── platform/           # macOS 原生桥
    ├── panel.rs        # NSPanel 转换
    ├── skylight.rs     # Space 迁移（私有 API）
    ├── focus.rs        # 焦点管理（PREV_FRONT_PID 唯一源 + is_app_active 系统弹窗/OSASCRIPT_RUNNING 检测 + restore_captured 第三方守卫）
    ├── input.rs        # CGEvent 键盘注入（post_key 原语 + post_combo 字符串糖；Modifier 枚举 + Option pid）
    ├── pasteboard.rs   # NSPasteboard 原语统一（read_text/read_file_urls/read_png(max)/read_tiff_as_png(max)/encode_image_to_png/set_png_bytes/write_text/clear/set_string/set_file_url/set_file_urls(marker?)/set_custom/has_type/change_count/snapshot/restore）
    ├── selection.rs    # AX 选中文本提取 + poll_clipboard
    ├── click_monitor.rs
    ├── frontmost_watcher.rs  # NSWorkspace 激活通知观察器（系统弹窗关闭后恢复焦点，随 show/hide 生命周期）
    ├── permission.rs
    ├── window.rs       # 主窗口原生操作（NSWindow show/hide/key + 圆角 + NSOpenPanel）
    └── path_guard.rs   # 统一路径校验

src/
├── main.ts             # 入口（import.meta.glob eager 扫描扩展 + 并行 setup）
├── commands.ts         # 命令名常量（CMD.xxx，禁止裸 invoke）
├── runtime/            # 前端运行时
│   ├── types.ts        # Extension / SearchProvider / SearchResult（14 槽：11 能力 + 3 行为）
│   ├── constants.ts    # 语义常量单一源（SEARCH.WEIGHTS/GROUP_ORDER/GROUP_TITLES/KEYWORD_MODULE_BOOST + LIMITS）
│   ├── storage.ts      # defineConfig（storePath + defaults；reactive + watch 自动持久化 + 递归 deepEqual race 保护 + 类型守卫 + isLoading 抑制 + 退出 flush + 跨窗口 onChange 同步 + store 实例缓存 + whenConfigReady）
│   ├── extension-registry.ts  # defineExtension + getAllExtensions + getExtension
│   ├── search-engine.ts       # dynamic 单通道 + keyword 合流 + dedupe + groupAndSort
│   └── ai-providers.ts        # 统一 AI 提供商/Key 中枢（config/ai-providers.json；agent/translate 消费）
├── components/
│   ├── ui/             # 原子组件（只用这些，禁止手写底层标签）
│   └── layout/         # MainView / ContentView / ResultItem（kind 分支内聚）/ ResultIcon（纯图标）/ ResultActionPanel
├── composables/
│   ├── useAppLifecycle.ts     # 主窗口生命周期（快捷键注册/失焦隐藏/模块事件，抽自 App.vue）
│   ├── useSearchInput.ts      # 搜索编排（全局 searchEngine + 搜索型模块 dynamic + web 搜索// + 工具列表/ + 默认结果）
│   ├── useResultNavigation.ts # 结果键盘导航 + 执行分派 + Escape 统一退出当前层
│   ├── useModuleHeight.ts    # 主窗口高度统一管理（number 固定 / 'auto' 自适应 / 默认），系统 animator 动画
│   ├── useActionPanel.ts      # Cmd+Enter 动作浮层通用逻辑（键盘导航 + 外点关闭 + capture-phase 劫持）
│   ├── useFloating.ts / useScrollPosition.ts / useTauriListener.ts / useToast.ts  # 通用工具
│   └── events.ts / useInputControl.ts / useShortcutConfig.ts
├── stores/             # app / settings（仅框架级）/ update
├── types/              # agent（手写 LLM/Agent 类型）+ settings（SettingItem 类型）
└── utils/
```

新增文件按所属模块归位，勿新建顶层分类。

## UI 规范

仅浅色。完整约定见 [docs/design.md](docs/design.md)；数值真相 `theme.css`，组合 `uno.config.ts`。**视觉已定型，改实现/文档不得无意改观感。**

**分层**：容器 **soft-surface** · 抬升卡 **soft-card**（助手消息 / system-status）· 控件 **soft-chip**（1px 冷灰 solid border，无 elevation；focus 改边框色）· **module-tag** · **ui-active** · **ui-btn-***（BaseButton variant 面类：primary/ghost/danger） · 弹窗 **dialog-\***（标题/底栏浮层渐变）· 实底填充 **fill-ctrl**。玻璃只做壳；可点控件走 chip。

**基元与 elevation**（`theme.css`：先改基元，业务禁堆相近零散值）：`--cool` / `--shadow-ink` / ease·duration / `--space`；阴影仅 **bar / panel / dialog / card / float\***；accent 浅染 **`--accent-wash*` / `--accent-line*`**（markdown 等）。

**颜色**：canvas=surface / accent / 文本阶；fill 阶派生 cool；语义 `danger|warning|success`（soft 统一 12%）。**禁止裸 hex 结构色、`black/*`、状态用 red-500**（`mask-smoke`、标注调色板、文件类型 palette 除外）。

**材质**：

- Mica：主窗 16 / snap 10 + `mica-shell`（获焦雾一轮）
- 搜索栏 `acrylic-bar`（拆层）；浮层 `dropdown-panel`；soft-surface **saturate 单轨 1.35**
- chrome-fade 顶/底配方可不同；圆角 ctrl 6 · panel 10 · window 16

**组件**：只用 `@/components/ui/` 原子组件，**禁止手写底层标签**。`ui-ctrl`+`soft-chip` 默认控件（outline 与 default 同面）；`ui-field` 大输入（`BaseTextarea`；`BaseInput panel` 仍 soft-surface 白边）；外框 / 填充 / 圆角一律 token（见 design.md）。

**容器边距**：全局统一 `p-3`（12px）——搜索栏 `inset-x-3 top-3`、列表 `p-x-3 pb-3`、模块内容根、textarea、浮层 `bottom-3 right-3`、floating 避让、设置页 `flex-col-full-pb`；搜索栏 top/height/gap 为 `constants.ts` 模块内常量（和为 `CHROME_HEIGHT`，与 MainView `top-3`/`h-13`/`p-3` 同步）。**例外**：`BaseDialog` 内容区水平 `16px`，顶/底由浮层 chrome 预留（标题/底栏渐变浮层）。**元素间距**三档：`gap-1.5`（控件内）/ `gap-2`（默认）/ `gap-3`（区块）；`gap-0.5` 仅微密 UI。

**弹窗**：`BaseDialog` Teleport 到 body；标题/底栏为绝对定位浮层 + 透明渐变，内容通铺可滚入。KeepAlive 切扩展时 `onDeactivated` 以 `dismiss` 关窗（父级 `@cancel` 卸 v-if）；全局 `showConfirm` 由 `setActiveModule` 按取消收束。控件 focus 只改边框色 `--focus-ring-color`（`BaseInput` 挂 `ui-input` 走 `:focus-within`，无 active 按下态；有 suffix 时 `pr-1` 让眼睛图标贴右）。

**浮层范式**：右下角浮层统一 `fixed bottom-3 right-3 z-50`（离边缘 12px，与全局 p-3 一致）+ `dropdown-panel` + 同款进出场动画（`ease-out` 进 / `ease-in` 离）。**toast**（`ToastOverlay`）用 `z-9999`，高于 `BaseDialog`（z-100）与动作面板（z-50），保证错误/成功反馈始终顶层可见。`BaseDropdownItems` 通用行渲染器（4 行类型 `item | header | divider | meta`，meta = label:value 详情行不可选）。screenshot 标注选区 / clean-mode 为功能性覆盖层不加材质；工具条 / 色板走 `acrylic-bar`（与主搜索框同款 soft-surface + `--shadow-bar`）；贴图悬停条走 `mica-bar`；选区阶段快捷键提示 `mica-panel`。

**写法**：原生 HTML 元素用 Attributify 模式（`<div text="sm primary" p="3">`），Vue 组件 props 保持 `class`；`animate` 等与 DOM 原生属性同名的禁用 Attributify，必须用 `class="animate-spin"`。

## 存储结构

扩展自管数据一律放各自 `extensions/<id>/`，无共享 `data/` 目录。

```
~/Library/Application Support/com.litiantao.voidnix/
├── config/settings.json              # 框架级配置（全局快捷键，defineConfig 扁平 schema）
├── config/ai-providers.json          # 统一 AI 提供商/Key（agent/translate/外部工具共用）
└── extensions/
    ├── clipboard/{clipboard.db, clipboard.db-wal, config.json}   # 剪贴板历史（SQLite WAL，写入计数达 200 触发 wal_checkpoint(TRUNCATE)）+ 配置
    ├── calculator/config.json        # 计算器历史（history key，10 条上限；走 defineConfig）
    ├── zsh-autosuggestions/{bin/, index.zsh, signals.log, bin.version, config.json}  # zsh 补全
    ├── awake/{Display Wakelock, config.json}   # awake binary + 配置
    ├── screenshot/config.json        # screenshot 扩展配置
    ├── window-manager/config.json    # window-manager 扩展配置
    ├── translate/config.json         # translate 扩展配置
    ├── agent/config.json             # agent 扩展配置（资源上限 + systemPrompt + 搜索 Provider；AI Key 见 config/ai-providers.json）
    ├── video/{ffmpeg,ffprobe,ffmpeg.version,config.json}  # 视频处理：按需下载的静态 ffmpeg/ffprobe + 配置（系统 PATH 有则优先用系统）
    └── proxy/{mihomo, mihomo.pid, mihomo.log, geoip.metadb, geosite.dat, config.yaml, subs/, config.json}  # 代理：mihomo 核心（TUN 模式 root 常驻）+ root 进程 PID + 运行日志 + Geo 数据库 + 运行配置 + 订阅 YAML + 配置
```

icon 缓存已消除（实时提取，零磁盘文件）。dev 镜像 `com.litiantao.voidnix.dev` 同构。

所有 config.json 均走 `defineConfig`（`src/runtime/storage.ts`）：reactive + watch + 300ms 防抖 + 深克隆 + race 保护 + 类型守卫 + 跨窗口 onChange 同步 + 退出 flush。schema 变更优先删磁盘 config.json；AI 中枢对旧 agent/translate 凭证字段做一次性 best-effort 导入（见 ai-providers）。

## 约定

- 环境：`isTauri` 判断环境（常量，非函数），非 Tauri 跳过原生调用
- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，描述力求最简，不写详情，不主动执行 git 操作
- 语言：注释和回复用中文，禁止在任何地方使用 emoji
- 文档：不用表格，言简意赅，修改代码后必须同步更新 AGENTS.md 或对应 docs/ 文档中相关描述
