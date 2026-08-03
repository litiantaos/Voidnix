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
bun run check:extensions     # CI 校验（extensions.rs 同步 + windowViews 漂移；动态窗口 screenshot/snap-panel 放行）
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

测试报告：`bun run test` 跑完经自定义 reporter 自动写 `.test-report.md`（gitignore）到项目根，含价值分层（回归/并发/边界/正向）与失败明细，可看可不看。

E2E 对 Vite dev server（CI 自动执行 `bunx playwright install` + `bun run test:e2e`）。原生窗口行为（快捷键/焦点/隐藏）仍需人工验证。

## 本地门禁

**每次 `git commit` 前必须先跑 `bun run precheck` 并全绿**，否则不得提交（AI agent 同样遵守，不得跳过）：

```bash
bun run precheck   # CI 同款全量门禁（不含 e2e）：lint:check → cargo fmt --check → typecheck → cargo clippy --lib -D warnings → check:drift → test → cargo test --lib
```

e2e（`bun run test:e2e`，需起 Vite dev server + 浏览器）不在本地门禁，交 CI 兜底。

## CI 门禁

`.github/workflows/ci.yml` 在 push（main/refactor/v2）与任意 PR 触发，依次执行：

1. 前端 lint:check（Prettier + ESLint）
2. Rust `cargo fmt --check`
3. `bun run typecheck`（vue-tsc 严格）
4. Rust `cargo clippy --lib -- -D warnings`
5. 漂移校验：`check:extensions` / `check:commands` / `check:agent-bounds` / `check:wm-bounds` / `check:extension-orders`
6. 单测：`bun run test`（Vitest）+ `cargo test --lib`
7. E2E：`bun run test:e2e`（Playwright，含浏览器安装）

## Prod 资源监控（长期采样）

LaunchAgent 常驻方案，监控 release 构建主进程 + 扩展子进程的 RSS/CPU/线程/数据目录，用于长期跟踪内存与占用趋势、定位泄漏。**仅监控 prod（release）进程**，dev/debug 不采样。

**三个脚本**（`scripts/`）：

- `voidnix-monitor.sh` — 采样器：launchd 每 60s 调用，Voidnix 未运行时 <10ms 退出零开销，运行时单次 `ps` 采主进程 + 一次 `ps -A` 全表扫描识别扩展子进程（按可执行路径 `comm` 匹配 `com.litiantao.voidnix/extensions/<id>/`，按扩展分组；不依赖 PPID 链——root 子进程如 mihomo 已 reparent 到 launchd；用 `comm` 而非 `command`，从根上排除 grep/osascript 等仅在参数里引用该路径的进程，也避免采样器自身 fork 的 shell 被误匹配）。日志自动保留 30 天。
- `voidnix-monitor-install.sh install|uninstall` — 安装/卸载 LaunchAgent（`com.litiantao.voidnix.monitor`，登录后自动生效）
- `voidnix-analyze.sh [天数]` — 分析器：按天聚合主进程 RSS 区间/漂移/CPU 峰值/线程数/数据目录（漂移 >20MB 自动告警），并按扩展聚合子进程采样数/RSS 区间/CPU 峰值

**日志**：`~/Library/Logs/Voidnix/monitor-YYYY-MM-DD.log`，`@ ext/bin rss cpu vsz` 子进程行紧随主进程行（`time rss cpu threads vsz data`），无子进程则无 `@` 行

**状态**：LaunchAgent 已安装运行，主进程已有 2 天数据；子进程采样维度已上线。曾暴露 proxy/mihomo CPU 持续 100% 问题——根因为 gvisor TUN 栈在睡眠唤醒后的连接风暴中泄漏 dial goroutine 进入 busy-loop，已将 TUN stack 切 system 栈彻底解决（见 [proxy.md](docs/extensions/proxy.md)）。执行 `bash scripts/voidnix-analyze.sh` 分析趋势。

## 开发扩展

所有扩展同构（`extensions/<id>/index.ts` + 可选 `config.ts` + 可选 `native/`），详见 [docs/extensions.md](docs/extensions.md)。

含 native/（16）：clipboard、screenshot、video、awake、clean-mode、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search、proxy、system-status、ai-providers、image、homebrew

纯 TS（7）：calculator、settings、ip、base64、time、uuid、currency

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)、[agent](docs/extensions/agent.md)、[ai-providers](docs/extensions/ai-providers.md)、[clean-mode](docs/extensions/clean-mode.md)、[proxy](docs/extensions/proxy.md)、[video](docs/extensions/video.md)、[image](docs/extensions/image.md)、[finder-ext](docs/extensions/finder-ext.md)、[window-manager](docs/extensions/window-manager.md)、[homebrew](docs/extensions/homebrew.md)。

## 架构要点

### 前端 ↔ Rust 端通信

- 命令名常量集中 `src/commands.ts`（`CMD.xxx`），**禁止裸 `invoke('xxx')`**，统一走 `invoke<T>(CMD.xxx, {...})` + 手写类型（`types/` 与各扩展）
- CI 双向差集校验：`scripts/check-commands.ts` 对 Rust `#[tauri::command]` 名集合 ↔ `commands.ts` 常量
- 扩展 Command 与框架 Command 统一在 `configure_app!` 的 `generate_handler!` 全局注册（sync-extensions 扫描生成，前端裸名 invoke）
- 流式/事件用 `app.emit()` 或 `tauri::ipc::Channel<T>`（agent 用后者）
- 含动态 JSON 的 Command（如 agent_run 的 `Channel<AgentEvent>`）手写 TS 类型（`src/types/agent.ts`）

### 扩展接口

`Extension`（`src/runtime/types.ts`）= `meta` + **14 槽**（11 能力槽 + 3 行为槽，按需声明，均有真实消费者）+ `setup` 生命周期。槽位语义与消费者计数详见 [docs/extensions.md](docs/extensions.md)。

### 搜索引擎

`src/runtime/search-engine.ts`：流式增量召回（消除快结果等慢结果的 barrier）→ 一次预算 finalScore → keyword 合流 → dedupe → groupAndSort。每个扩展 `emit`/`resolve` 都触发增量重排 + 回调 `onUpdate`，应用缓存秒出、内存索引文件结果随打随出。

**两种模式共用 `search()`**：

- **全局模式**：并行调所有扩展 dynamic → finalScore 排序（`fuzzy + boost`）→ keyword 合流 → 分组排序
- **扩展模式**（`setActiveExtension` 切换）：只调激活扩展 dynamic，bypass groupAndSort 保留扩展返回序；同样受 timeout/abort 保护

**模式快照**：`search()` 入口捕获 `activeExtension`，await 期间切换不影响本次后处理。

**超时**：每扩展独立 child `AbortSignal`，超时只 abort 该扩展（不牵连其它），父 signal abort 时同步取消。

**过滤规则**：

- 空 query：默认列表按 `finalScore>0`（boost>0，主要是应用）
- 非空 query：查找型结果需 `fuzzy>0`，extension 类即时答案靠 `finalScore>0` 穿透

**扩展入口打分**：keyword / `/` 工具列表共用 `scoreExtensionEntry`（name/id/description 正向 + keywords 双向）。

`SearchContext.extensionMode` 供扩展区分场景：全局即时答案仅 calculator / currency；ip / time / uuid / base64 等仅扩展内响应。详见 [docs/extensions.md](docs/extensions.md)。

### 窗口

`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。

**样式**：`platform/window.rs::apply_main_window_style`（setup 内一次性）= `apply_mica_material(ns, 16)` + `setHasShadow(true)` + `convert_to_panel`。即 Mica + contentView 圆角 16（`radius-window`）+ 原生阴影 + 冷雾 tint（见 [设计系统](docs/design.md)）。snap-panel 经 `apply_mica_material(ns, 10)` 对齐 `radius-panel`。appearance 跟随主题（`apply_window_appearance`：auto=None 跟随系统并驱动 WKWebView prefers-color-scheme，light/dark 强制覆盖）。**跨窗口**：`set_window_appearance` 是全局副作用命令（一次应用所有窗口），仅由 main 的 `theme.ts` 驱动；invisible 创建的子窗口（screenshot/snap-panel）由 Rust 经 `apply_cached_appearance` 设原生 appearance；pin 窗口 visible 创建不可设（setAppearance 在刚 build 的 WKWebView 上触发 prefers-color-scheme 重算死锁主线程），改由前端读 `get_cached_appearance` 命令拿 main 缓存值直接设 DOM data-theme。

**panel 转换**：`platform/panel::convert_to_panel` 转 `NonactivatePanel`（点击/makeKey 不自动激活）。

**show 策略**：

- 不 `activate_app`（保持原前台 active，避免聚焦视图/菜单栏突变；代价是 macOS 26 上偶发下层 hover 穿透——产品优先不打断）
- hit-test 靠 `capture_mouse_events` + SkyLight event shape
- `present_on_cursor_screen`：光标屏居中并写 `PLACEMENT_VIS`
- `animate_frame` 只在 `PLACEMENT_VIS` 内改尺寸（忽略前端 x/y，高度立即生效）
- 截图 overlay 等独占场景才显式 `activate_app`

**hide 策略**：

- 不 orderOut（仅 alpha=0 + ignoresMouse + 去阴影）——orderOut 后副屏二次 show 坐标对也不绘
- 主窗 Space 只 Add；collectionBehavior = `CanJoinAllSpaces|FullScreenAuxiliary`（勿并 MoveToActiveSpace）
- `hide_main` 走 `restore_captured()` 交还 first responder（`PREV_FRONT_PID` 唯一源在 `platform/focus.rs`）

**焦点管理**——`is_app_active()` 三道判定：

1. NSApp keyWindow 非空 → 焦点在我们
2. frontmost bundle 路径 `/System/` 开头（授权弹窗、keychain 对话框等）→ 交互流未中断
3. `OSASCRIPT_RUNNING` 标志（osascript 授权后续 shell 命令执行期间 frontmost 已还给原 app 但仍抑制 blur hide）

焦点恢复细节：

- `restore_captured()` 还原前查 frontmost：第三方已接管（系统弹窗/用户切到其他 app）则不抢回
- 系统弹窗关闭后由 `platform/frontmost_watcher`（NSWorkspace 激活通知观察器，随 show/hide 生命周期 add/remove）处理：
  - frontmost == 原前台 PID → `makeKeyWindow` 恢复
  - frontmost != 原前台 PID → 用户主动切换 → emit `frontmost-changed` → 前端 dismiss

**窗口高度**——扩展声明 `windowHeight`（`number` 固定 / `'auto'` 自适应 / 未声明默认 480），subview 可经 `subviewHeights` 覆盖：

- `useExtensionHeight`（MainView 全局唯一调用）读 `activeExtension` + `activeSubview` 解析模式
- 一次 invoke 触发 Rust `set_main_frame` → `animate_frame` 用 `NSAnimationContext` + `animator setFrame:display:animate:` 系统级动画（CoreAnimation 接管，非 JS rAF 逐帧）
- `auto` 模式：ResizeObserver 监听 `contentRef`，窗口高 = `CHROME_HEIGHT`（搜索栏 + 间距）+ 内容高，clamp `[DEFAULT_HEIGHT, 屏幕高 90%]`
- 屏幕尺寸走 `currentMonitor`（WKWebView 下 `window.screen` 仅返回 webview 视口）
- 底部将出屏（含 40px 间距）则同步上移；离开 auto 还原进入前位置

### 全局快捷键

`runtime/shortcut.rs`，快捷键 id 驱动（前端传 id + shortcut，Rust 自管注册表 + 录制模态 + 扩展钩子）。

- 默认 Option 基：`Option+Space` 呼出，`Option+C/S/T/A/F` = 剪贴板 / 截屏 / 翻译 / Agent / 访达工具
- dev 构建（debug）注册时经 `cfg!(debug_assertions)` 自动叠加 `Shift`，与 prod（release）区分且可并存
- dev/prod 数据目录按 bundle id 隔离，配置默认值一致

### 菜单栏

`runtime/menubar.rs`，框架唯一托盘图标（`public/bar_icon.png` + `icon_as_template` 深浅色自适应），左键弹聚合菜单。

**扩展贡献**：含 native/ 的扩展在 Rust `setup` 内 `menubar::register(MenuBarContribution{ title, build, on_event })`：

- `title`：分组标题（disabled 项渲染）
- `build`：返回 `Vec<MenuEntry>` 快照（`Item`/`CheckItem`/`Submenu`/`Separator`）
- `on_event`：收点击 id 自行过滤
- 状态变更后调 `menubar::refresh(&app)` 触发重建

**渲染规则**：菜单按扩展 `title` 分组（每段前插 disabled 标题项，段间分隔线）。

**可见性**：`Σ build() 项数 > 0`（空快照 = 该扩展当前不贡献；扩展全关图标自动隐藏）。

**实现范式**：镜像 `shortcut.rs`（`LazyLock<Mutex<Vec>>` + free function，`Arc<dyn Fn>` 锁外调用防 `on_event→refresh` 重入死锁）。Rust 侧能力（非 TS `Extension` 槽——菜单构建依赖 Rust State，纯 TS 扩展无此需求）。

**消费者**（2 个）：

- **awake**：打开扩展 + 启用开关 + 显示模式二级菜单
- **proxy**：打开扩展 + 已连接状态 CheckItem 可点断开「已连接：节点」；断开后图标隐藏，重连走扩展视图（详见 [proxy.md](docs/extensions/proxy.md)）

### Agent 引擎

`extensions/agent/native/engine/`：tool calling loop，服务 agent 扩展。prompt/max_turns/资源上限由扩展 config 注入（非框架硬编码）。

**模块分工**：

- `loop_runner.rs`：主循环 `run_loop`（调 LLM → 解析 tool_calls → 执行 → 回灌 → 下一轮）
- `cancellation.rs`：`SessionRegistry`（per-session CancellationToken；loop 结束 unregister，abort 走 cancel）
- `trim.rs`：历史消息裁剪（下沉自 runtime/llm）
- `secret_scrub.rs`：gitleaks 风格正则打码
- `tool_registry.rs`：`AgentTool` trait + `ToolRegistry`

**命令执行**：无审批、无白/黑名单，所有命令直接放行。

- `extensions/agent/native/policy.rs` 是资源上限 floor/cap 权威源（CPU/内存/文件描述符/超时/输出/轮次 clamp）
- `agent_run` 入口强制 clamp（不信任前端传值）
- `run_command` 保留 `rm -rf /` 断路器兜底
- TS 端 `config.ts` 的 `BOUNDS` 仅 CI 镜像（无 Settings UI）
- 详见 [agent.md](docs/extensions/agent.md)

### 搜索打分

`src/utils/fuzzy.ts::scoreFields()`（[pinyin-pro](https://github.com/zh-lx/pinyin-pro)，三开关锁死中文缩写/全拼/ü→v 语义），权重读 `runtime/constants.ts::SEARCH.WEIGHTS`。

- 扩展入口用 `scoreExtensionEntry()`（name/id/description + `keywordMatch` 双向），全局 keyword 与 `/` 列表共用
- **抑制规则**：dynamic 产出相关 tool 型结果（kind=extension，finalScore > 0）的扩展抑制其 keyword 入口（即时答案优先；clipboard 等 kind≠extension 不抑制）
- `kind` 枚举：`application | folder | file | extension | clipboard | web`
- 组间序 `GROUP_ORDER`：`application > extension > file > clipboard > web`
- 组内限流：`LIMITS.maxGroupResults`（非 file）/ `maxFileResults`

### toast 提示

自研轻量浮层（`composables/useToast.ts` + `components/ui/ToastOverlay.vue`）。

- 按 `kind` 切图标/色：`success` accent 对勾 / `error` danger 语义色警告（错误反馈必须传 `kind: 'error'`）
- 堆叠上限 3 条、默认 2000ms 自动清除
- 窗口隐藏时立即清空（`hideWindow` 内调 `clearToasts`，规避 macOS 隐藏 WebView 节流 setTimeout 致残留）
- 扩展通过 `copyAndHide`（`stores/app.ts`：写剪贴板 + showStatus 反馈 + 延迟隐藏窗口）自动获得「已复制」反馈
- `showStatus(msg, opts?)` 委托 `showToast`（调用点零改动）
- 搜索栏 placeholder：扩展模式显示搜索说明（`在 X 中搜索` 或扩展自声明 `placeholder`），全局模式保留搜索说明

### 浮层组件

- **`BaseDropdownItems`**（`components/ui/`）：通用行渲染器，4 行类型 `item | header | divider | meta`（`selectableIndices` 仅算 item，键盘导航天然跳过其余）；消费者传 `PanelItem[]` + `activeIndex`，emit `select/hover`
- **`ResultActionPanel`**（`components/layout/`）：全局模式对 application/file/folder 结果的合并面板（上方详情 meta 行 + 下方动作 item 行——在 Finder 中显示 / 复制路径）；`Cmd+Enter` 与结果项右键双触发，经 `useActionPanel` 统一的 `toggleOpen`（已开则关），打开即默认选中首项可连续 Enter 触发
- **Markdown 渲染**：`utils/markdown.ts`（`renderMarkdown`：marked + 自定义 renderer + DOMPurify）+ 全局 `styles/markdown.css`（`.markdown-body` 容器类），agent / ai-providers 等扩展共用

### 扩展视图加载（切换性能）

- 扩展 View（mainView/subviews/searchBarAccessory）**静态 import** 进主 bundle（用户高频、固定集合，首次进入零卡顿）
- 仅**独立窗口**（screenshot 标注 host/pin、window-manager snap 面板，`windowViews`）保留 `defineAsyncComponent` 真按需——不截图/不分屏不加载，省稳态占用（gzip ~20KB）
- **窗口按需创建**：screenshot / snap-panel 窗口从 `tauri.conf.json` 移除静态声明，改在扩展 `setup` 中 `WebviewWindowBuilder` 代码创建（WKWebView 需启动时预加载页面，快捷键/鼠标触发时才能即时响应）
- **vendor 分包 + pinyin 延迟加载**：`manualChunks` 拆 vendor(vue+pinia) / markdown(marked+dompurify) / pinyin 独立 chunk；pinyin-pro（拼音字典 289KB）改为首次 CJK 查询时 `import()` 异步加载，首屏零开销
- `ContentView` 用 `KeepAlive`（max 覆盖全部视图 key）缓存已访问扩展，切换走 activate/deactivate 而非重挂载

### LLM 基础设施

`runtime/llm/`，agent + translate 扩展共享：

- `types.rs`：LlmMessage
- `client.rs`：StreamConfig / `stream_openai_request` + SSRF 防护 `validate_ai_request` + 消息截断 + 请求管道常量
- `parser.rs`：tool_calls 解析

### AI 凭证中枢

`src/runtime/ai-providers.ts` → `config/ai-providers.json`：只存 URL/Key/模型，**无「使用中」**。

- **选用自管**：agent（`providerModelKey`）/ translate（`selections`）各自持久化选用
- **同步机制**：`isCredentialSelectionValid` + 读时 effective（不写回）+ 启动/写入冷 prune
- **env 输出**：写 `ai.env`（`VOIDNIX_ZHIPU_*` / `VOIDNIX_DEEPSEEK_*` 等私有名，`*_BASE_URL` 按提供商去重）；仅 release 注入 shell，debug 只写文件
- **Shell rc 注入**统一走 `runtime/shell_rc`（`# voidnix <scope>`），见 [shell-rc.md](docs/shell-rc.md)
- 详见 [ai-providers.md](docs/extensions/ai-providers.md)

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口（lib.rs setup 内含启动埋点，debug 打印 [boot] 各阶段耗时 + <100ms 判定）
├── extensions.rs       # 自动生成（configure_app! + register_all 生命周期 + generate_handler! + mod 声明）
├── http.rs             # HTTP 客户端 + http_get 命令
├── runtime/            # 运行时核心（平台无关）
│   ├── autostart.rs   # 开机自启命令薄壳（SMAppService Login Item 注册/查询）
│   ├── window.rs       # 主窗口 show/hide
│   ├── shortcut.rs     # 快捷键 + 录制
│   ├── menubar.rs      # 聚合菜单栏托盘（框架唯一图标 + 扩展贡献段注册）
│   ├── storage.rs      # TempHandle RAII + ext_data_dir + save_png_safely
│   ├── permission.rs   # 系统权限命令薄壳（同步；screen_recording 走 preflight 不截屏）
│   ├── registry.rs     # Extension trait + ExtensionRegistry（concurrent bootstrap；单扩展 setup 失败隔离；阻塞 I/O 扩展自管 spawn_blocking）
│   ├── pasteboard.rs   # 框架命令薄壳（write_text / paste_text；原语在 platform/pasteboard）
│   ├── shell_rc.rs     # .zshrc 注入约定（# voidnix <scope> marker）
│   └── llm/            # LLM 基础设施（types / client / parser）
└── platform/           # macOS 原生桥（零业务语义）
    ├── autostart.rs    # SMAppService（macOS 13+）注册主 app 为系统 Login Item（objc2 调用）
    ├── panel.rs        # NSPanel 转换
    ├── skylight.rs     # Space 迁移（私有 API）
    ├── focus.rs        # 焦点管理（PREV_FRONT_PID + is_app_active + restore_captured）
    ├── input.rs        # CGEvent 键盘注入（post_key + post_combo）
    ├── pasteboard.rs   # NSPasteboard 原语统一
    ├── selection.rs    # AX 选中文本提取 + poll_clipboard
    ├── click_monitor.rs
    ├── frontmost_watcher.rs  # NSWorkspace 激活观察器（系统弹窗后恢复焦点）
    ├── permission.rs
    ├── window.rs       # 主窗口原生操作（NSWindow + 圆角 + NSOpenPanel + appearance 缓存）
    └── path_guard.rs   # 统一路径校验
```

`http.rs` 细节：`HTTP_CLIENT` 整体 120s 超时；`DOWNLOAD_CLIENT` 无整体超时仅建连 30s（供流式大文件下载）；`http_get` 命令含浏览器 UA 伪装 + SSRF 防护 + 重定向限制 + 共享 `parse_scheme_host`/`is_blocked_host` 原语（ip/currency 等纯 TS 扩展消费）。

```
src/
├── main.ts             # 入口（import.meta.glob eager 扫描扩展 + 并行 setup）
├── commands.ts         # 命令名常量（CMD.xxx，禁止裸 invoke）
├── runtime/            # 前端运行时
│   ├── types.ts        # Extension / SearchProvider / SearchResult（14 槽：11 能力 + 3 行为）
│   ├── constants.ts    # 语义常量单一源（SEARCH.WEIGHTS / GROUP_ORDER / GROUP_TITLES / KEYWORD_EXTENSION_BOOST + LIMITS）
│   ├── storage.ts      # defineConfig（reactive + watch 自动持久化 + race 保护 + 类型守卫 + 退出 flush）
│   ├── extension-registry.ts  # defineExtension + getAllExtensions + getExtension
│   ├── search-engine.ts       # dynamic 单通道 + keyword 合流 + dedupe + groupAndSort
│   ├── ai-providers.ts        # 统一 AI 提供商/Key 中枢（agent/translate 消费）
│   └── theme.ts               # 主题运行时（appearance 持久化 + 系统外观跟随 + 原生窗口同步）
├── components/
│   ├── ui/             # 原子组件（只用这些，禁止手写底层标签）
│   └── layout/         # MainView / ContentView / ResultItem（kind 分支内聚）/ ResultIcon / ResultActionPanel
├── composables/
│   ├── useAppLifecycle.ts     # 主窗口生命周期（快捷键/失焦隐藏/扩展事件）
│   ├── useSearchInput.ts      # 搜索编排（全局 searchEngine + 搜索型扩展 dynamic + web 搜索 + 默认结果）
│   ├── useResultNavigation.ts # 结果键盘导航 + 执行分派 + Escape 统一退出
│   ├── useExtensionHeight.ts   # 主窗口高度统一管理
│   ├── useActionPanel.ts      # Cmd+Enter / 右键 动作浮层（toggleOpen 统一入口 + 键盘导航 + 外点关闭）
│   ├── useFloating.ts / useScrollPosition.ts / useTauriListener.ts / useToast.ts
│   └── events.ts / useInputControl.ts / useShortcutConfig.ts
├── stores/             # app / settings（仅框架级）/ system（权限+开机自启预查缓存）/ update
├── types/              # agent（手写 LLM/Agent 类型）+ settings（SettingItem 类型）
└── utils/
```

新增文件按所属目录归位，勿新建顶层分类。

### 官网（site/）

独立 Astro 子项目（单页落地页），token 自动同步产品 `theme.css` 全量 `:root`。首页 Hero 内嵌实时动画演示（`DemoStage.astro` 组件，非视频——840 帧 JS 驱动的拟物 macOS 桌面舞台，首页与 `/demo` 预览页共用）。文档：[site/README.md](site/README.md)（概览 / 开发 / 部署）+ [site/demo.md](site/demo.md)（DemoStage 组件架构、动画系统、分镜时间线、可选视频导出）。

## UI 规范

浅色 / 深色双轨（默认跟随系统，设置中可切自动 / 浅色 / 深色）。完整约定见 [docs/design.md](docs/design.md)；数值真相 `theme.css`（`:root` 浅色默认 + `:root[data-theme="dark"]` 深色覆盖），组合 `uno.config.ts`。**视觉已定型，改实现/文档不得无意改观感。**

### 分层

- 容器 **soft-surface** · 抬升卡 **soft-card**（助手消息 / system-status）· 控件 **soft-chip**（1px 冷灰 solid border，无 elevation；focus 改边框色）· **ext-tag** · **ui-active** · **ui-btn-\***（BaseButton variant 面类：primary/ghost/danger）· 弹窗 **dialog-\***（标题/底栏浮层渐变）· 实底填充 **fill-ctrl**
- 玻璃只做壳；可点控件走 chip

### 基元与 elevation

`theme.css`：先改基元，业务禁堆相近零散值。

- 基元：`--cool` / `--shadow-ink` / ease·duration / `--space`
- 阴影仅：bar / panel / dialog / card / float\*
- accent 浅染：`--accent-wash*` / `--accent-line*`（markdown 等）

### 颜色

- canvas=surface / accent / 文本阶；fill 阶派生 cool
- 语义 `danger|warning|success`（soft 统一 12%）
- **禁止**裸 hex 结构色、`black/*`、状态用 red-500（`mask-smoke`、标注调色板、文件类型 palette 除外）

### 材质

- Mica：主窗 16 / snap 10 + `mica-shell`（获焦雾一轮）
- 搜索栏 `acrylic-bar`（拆层）；浮层 `dropdown-panel`；soft-surface **saturate 单轨 1.35**
- chrome-fade 顶/底配方可不同；圆角：ctrl 6 · panel 10 · window 16

### 组件

- 只用 `@/components/ui/` 原子组件，**禁止手写底层标签**
- `ui-ctrl` + `soft-chip`：默认控件（outline 与 default 同面）
- `ui-field`：大输入（`BaseTextarea`；`BaseInput panel` 仍 soft-surface 白边）
- 外框 / 填充 / 圆角一律 token（见 design.md）

### 容器边距

- 全局统一 `p-3`（12px）：搜索栏 `inset-x-3 top-3`、列表 `p-x-3 pb-3`、扩展内容根、textarea、浮层 `bottom-3 right-3`、floating 避让、设置页 `flex-col-full-pb`
- 搜索栏 top/height/gap 为 `constants.ts` 文件内常量（和为 `CHROME_HEIGHT`，与 MainView `top-3`/`h-13`/`p-3` 同步）
- **例外**：`BaseDialog` 内容区水平 `16px`，顶/底由浮层 chrome 预留
- **元素间距**（集中定义，按值从大到小）：
  - `gap-3`（区块）：纵向段落 / 表单段、列表项内部主分区、grid 列间
  - `gap-2`（同级控件 / 离散标签块）：按钮组（搜索栏 accessory / 分组标题操作项 / 弹窗底栏）、卡片标题行（icon+label）、数值行（number+unit）
  - `gap-1.5`（控件内 / 密集信息流 / 列表项间）：单控件内部子元素（icon+text）、flex-wrap 连续文本片段、`BaseList` 纵向列间距
  - `gap-1`：agent 步骤缩进（极密）
  - `gap-0.5`：刻度柱（微密）

### 弹窗

- `BaseDialog` Teleport 到 body；标题/底栏为绝对定位浮层 + 透明渐变，内容通铺可滚入
- KeepAlive 切扩展时 `onDeactivated` 以 `dismiss` 关窗（父级 `@cancel` 卸 v-if）；全局 `showConfirm` 由 `setActiveExtension` 按取消收束
- 控件 focus 只改边框色 `--focus-ring-color`（`BaseInput` 挂 `ui-input` 走 `:focus-within`，无 active 按下态；有 suffix 时 `pr-1`）

### 浮层范式

- 右下角浮层统一 `fixed bottom-3 right-3 z-50`（离边缘 12px）+ `dropdown-panel` + 同款进出场动画（`ease-out` 进 / `ease-in` 离）
- **toast**（`ToastOverlay`）用 `z-9999`，高于 `BaseDialog`（z-100）与动作面板（z-50）
- `BaseDropdownItems` 通用行渲染器（4 行类型 `item | header | divider | meta`，meta = label:value 详情行不可选）
- screenshot 标注选区 / clean-mode 为功能性覆盖层不加材质；工具条 / 色板走 `acrylic-bar`；贴图悬停条走 `mica-bar`；选区阶段快捷键提示 `mica-panel`

### 过渡动效

- **标准进出场**：toast / 动作面板 / 下拉 / 标注浮层统一走 `<Transition name="ui-popup">`（全局类在 `theme.css`，进 `--duration-fast` `--ease-out` / 退 `100ms` `--ease-in`，位移 8px + 缩放 .95）。`ui-popup` 用 `transform` 属性做动画，与 UnoCSS `translate-*`/`scale-*`（Wind4 落独立属性）正交叠加，带 `-translate-x-1/2` 居中定位也不冲突
- **方向变体**（如 `BaseSelect` 上下展开）直接用 `transition`（UnoCSS 默认 property 列表已含 `translate,scale,opacity,transform`，覆盖 Wind4 独立属性的 from/to）；**禁止** `transition-[a,b,c]` 方括号多值语法——Wind4 不生成该规则，类为空致无过渡瞬时跳变。单属性可用 `transition-[opacity]`
- **数值走基元**：自定义过渡的时长 / 曲线一律 `var(--duration-*)` / `var(--ease-*)`（`--duration-fast` 150 / `--duration-normal` 200），禁止裸 `0.2s` / `cubic-bezier(...)`。布局伸缩等 CSS 无法表达的用 JS hooks（`image` 扩展 `expandHooks`）
- **仅 GPU 合成属性**：过渡只用 `transform` / `opacity` / `translate` / `scale`（独立属性），禁用 `box-shadow` / `background-color` 等 paint 类属性（每帧重绘致顿）。`ui-popup` 用 `transform` 属性——与 UnoCSS `translate-*`/`scale-*`（Wind4 落独立属性）正交叠加，带 `-translate-x-1/2` 居中定位也不冲突
- **玻璃材质 + opacity**：`opacity` 落在带 `backdrop-filter` 元素**自身**安全（毛玻璃先采样背景再整体降透明）；落在**祖先**则形成 group opacity 隔断背景采样、材质失效（进出场先透明后跳变）。故 `mica-bar`/`acrylic-bar` 等玻璃控件的淡入走自身 `opacity`，根层只走 `transform`（见 `PinWindow`）

### 写法

- 原生 HTML 元素用 Attributify 模式（`<div text="sm primary" p="3">`），Vue 组件 props 保持 `class`
- `animate` 等与 DOM 原生属性同名的禁用 Attributify，必须用 `class="animate-spin"`

## 存储结构

扩展自管数据一律放各自 `extensions/<id>/`，无共享 `data/` 目录。

```
~/Library/Application Support/com.litiantao.voidnix/
├── config/settings.json              # 框架级配置（全局快捷键 + 外观模式，defineConfig 扁平 schema）
├── config/ai-providers.json          # 统一 AI 提供商/Key（agent/translate/外部工具共用）
└── extensions/
    ├── clipboard/{clipboard.db, clipboard.db-wal, config.json}   # SQLite WAL（写入达 200 触发 wal_checkpoint）+ 配置
    ├── calculator/config.json        # 计算器历史（history key，10 条上限）
    ├── zsh-autosuggestions/{bin/, index.zsh, signals.log, bin.version, config.json}  # zsh 补全
    ├── awake/{Display Wakelock, config.json}   # awake binary + 配置
    ├── screenshot/config.json
    ├── window-manager/config.json
    ├── translate/config.json
    ├── agent/config.json             # 资源上限 + systemPrompt + 搜索 Provider（AI Key 见 ai-providers.json）
    ├── video/{ffmpeg,ffprobe,ffmpeg.version,config.json}  # 按需下载的静态 ffmpeg/ffprobe + 配置
    ├── image/config.json             # 输出目录配置（移除背景/拼接结果）
    └── proxy/{mihomo, mihomo.pid, mihomo.log, geoip.metadb, geosite.dat, config.yaml, subs/, config.json}  # TUN 模式 root 常驻
```

icon 缓存已消除（实时提取，零磁盘文件）。dev 镜像 `com.litiantao.voidnix.dev` 同构。

所有 config.json 均走 `defineConfig`（`src/runtime/storage.ts`）：reactive + watch + 300ms 防抖 + 深克隆 + race 保护 + 类型守卫 + 退出 flush。不订阅 plugin-store `onChange`（set 会向本进程回放 `store://change` 无来源标识，实测复现：回灌旧快照覆盖 emit 到达前已 mutate 的新值）；所有 config 仅在 main 窗口持有，无跨窗口同步需求。schema 变更优先删磁盘 config.json；AI 中枢对旧 agent/translate 凭证字段做一次性 best-effort 导入（见 ai-providers）。

## 约定

- 开发环境基线：macOS 26（Apple Silicon，arm64），基于当前系统版本开发——优先采用现代 API，禁止为旧 macOS 写兼容分支或降级逻辑（私有 API / 私有 framework 直接用，不裹版本探测）
- 环境：`isTauri` 判断环境（常量，非函数），非 Tauri 跳过原生调用
- UnoCSS Attributify：不确定的工具类语法先用 context7 查 UnoCSS 文档确认，勿靠翻 dist 源码或试错猜语法
- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，描述力求最简，不写详情，不主动执行 git 操作；**提交前必须先跑 `bun run precheck` 且全绿**
- 语言：注释和回复用中文，禁止在任何地方使用 emoji
- 文档：不用表格，言简意赅，修改代码后必须同步更新 AGENTS.md 或对应 docs/ 文档中相关描述
