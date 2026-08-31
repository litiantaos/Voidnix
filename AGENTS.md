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
bun run check:extensions     # CI 校验（extensions.rs 同步）
bun run check:commands       # CI 校验（Rust #[tauri::command] ↔ commands.ts 双向差集）
bun run check:agent-bounds   # CI 校验（agent 资源上限 policy.rs ↔ config.ts BOUNDS 双向一致）
bun run check:wm-bounds      # CI 校验（window-manager mod.rs ↔ config.ts BOUNDS 双向一致）
bun run check:extension-orders # CI 校验（非 hidden 扩展 meta.order 唯一）
python3 scripts/smoke-test.py  # 全功能回归测试（含应用自测 + 系统冒烟）
bun run smoke-test             # 同上（package.json 别名）
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

## 全功能回归测试（标准化冒烟测试）

`scripts/smoke-test.py` — 全链路回归门禁，覆盖全部 24 扩展 + 框架行为 + 性能指标，防止"修 A 坏 B"连锁回归。仅用户明确要求时运行。

**三层架构**：

- **Layer 1（应用自测）**：`src/runtime/self-test.ts`，在真实 app 内部运行（环境变量 `VOIDNIX_SELF_TEST=1` 触发），直接调用 `searchEngine.search()` / `getAllExtensions()` / `invoke()` 等真实 API 做断言。覆盖：扩展注册完整性（24 扩展 / id 无重复 / order 唯一）、搜索引擎正确性（calculator 算式 / base64 解码 / keyword 入口 / 空查询 / 无结果）、扩展视图渲染冒烟（逐个激活 17 个 mainView，检查 console.error 无关键异常）、Tauri 命令可达性（无副作用探测调用）、窗口管理运行时启用（主题已初始化下 `setWindowManagerEnabled` disable→enable toggle，8s 超时检测 Mutex 重入死锁）、扩展功能正确性（clipboard 历史查询结构 / system-status 快照 / proxy 核心状态 / homebrew 状态 / video 核心状态 / awake·clean-mode 状态查询 / ip·time·uuid·currency 即时答案；网络依赖项失败 skip 不 fail）、搜索延迟基线（空查询 / keyword / calculator / base64 / 应用搜索代表性 query 耗时断言）。报告经 plugin-store 写到 `config/test-report.json`。
- **Layer 2（系统冒烟）**：CGEvent 驱动真实 UI，验证窗口显隐 / 全局快捷键 / snap-panel 全链路 / 搜索 UI / 扩展视图渲染。每步返回结构化 `TestResult`（pass/fail/skip），汇总为统一报告。逐阶段内存采样输出趋势（非仅终点）。
- **Layer 3（性能压测，`--perf [N]`）**：N 轮全场景工作负载循环（全局搜索 / 工具列表 / 快捷键 / 扩展视图 / hide/show），每轮逐阶段采内存快照，输出多轮趋势表 + drift 分析，定位 compositing layer 累积与回收。工作负载顺序刻意安排：快捷键在扩展视图之前（快捷键含 hide_window，若此时 FP 已超 350M 阈值会触发 navigate 重载，重载期间 WKWebView 不可交互）。合并自原 `wk-mem-test.py`。**内存结论只看 release 模式**：dev 模式下 Vite HMR / UnoCSS 开发态样式注入使每次视图变更重建整页合成树，WebContent graphics 可虚高至 GB 级（实测 5 轮 1.6G），release 同负载零累积（FP/graphics 全程持平甚至净降）——`--dev --perf` 只用于功能/时序验证，内存数据无效。

```bash
python3 scripts/smoke-test.py --self-test-only   # 仅 Layer 1（~30s，无需独占屏幕）
python3 scripts/smoke-test.py                     # 标准（Layer 1 + 2 + 逐阶段内存趋势）
python3 scripts/smoke-test.py --perf              # 标准 + 5 轮内存压测趋势
python3 scripts/smoke-test.py --dev               # dev 构建
python3 scripts/smoke-test.py --build             # 含 release 构建（同 deploy.sh 加载 .env 签名凭证，防 adhoc 退化）
python3 scripts/smoke-test.py --no-cgevent        # 跳过 Layer 2（CI/headless 友好）
```

CGEvent 基础设施（键盘映射 / 窗口检测 / 鼠标操作 / 内存测量）提取到 `scripts/voidnix_test_lib.py`。修饰键仅通过 `CGEventSetFlags` 设在 event 上（flags-only），不发独立 modifier key-down/up 事件——IOHIDSystem 永远不记录 Option/Cmd 被按下，后续 `type_text` 的 flags=0 字符不可能被系统叠加 Option flag 误判为 Alt+key 触发随机扩展快捷键。Event source 用 `kCGEventSourceStateHIDSystemState`（模拟裸硬件输入，modifier 状态独立于 session 切换）。

自测触发机制：`runtime/test.rs::is_self_test_mode` 命令读 `VOIDNIX_SELF_TEST` 环境变量 + AtomicBool **一次性守卫**（`main.ts` 在扩展 setup 完成后检查，true 则动态 import `self-test.ts` 运行，动态 import 不进生产初始 chunk）。一次性守卫防止 WebContent 内存超 350M 触发 navigate 重载后 `main.ts` 重新执行导致自测**二次触发**——环境变量是进程级的不随页面重载消失，若无守卫二次触发的自测与外部 CGEvent 测试脚本并发争抢同一 Vue store / 窗口状态，导致第 2 轮起 UI 乱跳。

**内存基线持久化**：首次运行采集 footprint / graphics 后写入 `scripts/smoke-baselines.json`（提交到仓库，团队共享参考基线）。后续运行改用基线值 + drift 容忍度（footprint +25% / graphics +50%）对比，比硬编码绝对上限更灵敏地检测回归。drift 超容忍度时不更新基线（防 GC 抖动峰值固化）。报告 `scripts/smoke-test-report.md` gitignore，基线文件提交。

## 本地门禁

**每次 `git commit` 前必须先跑 `bun run precommit` 并全绿**，否则不得提交（AI agent 同样遵守，不得跳过）：

```bash
bun run precommit   # 提交前门禁（不含 e2e）：lint（写盘修复）→ cargo fmt（写盘）→ typecheck → cargo clippy --lib -D warnings → check:drift → test → cargo test --lib
```

precommit 会自动修复格式（`prettier --write` + `cargo fmt`），跑完后 `git diff` 检查是否有非预期格式化，确认后一起提交。

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

## 发布管道与代码签名

正式版发布走 `.github/workflows/release.yml`（`git tag v*` 触发，`tauri-action` 打包）。CI 与本地 `deploy.sh` 必须用同一 Apple 证书签名——adhoc 签名的 cdhash 每次编译都变，TCC 按其匹配系统权限会导致每次更新权限失效。CI 签名凭证走 GitHub secrets（`APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` / `APPLE_SIGNING_IDENTITY`，同 `.env`），本地 `deploy.sh` 已内置 adhoc 拦截（断言 `TeamIdentifier=27869WH3RZ`）。

## Prod 资源监控（长期采样）

LaunchAgent 常驻方案，监控 release 构建主进程 + 扩展子进程的 RSS/CPU/线程/数据目录，用于长期跟踪内存与占用趋势、定位泄漏；主进程瞬时 CPU 超阈值时自动抓调用栈快照，定位高占用根因。**仅监控 prod（release）进程**，dev/debug 不采样。

**脚本**（`scripts/`）：

- `voidnix-monitor.sh` — 采样器：launchd 每 60s 调用，Voidnix 未运行时 <10ms 退出零开销，运行时用 `top` 采主进程 Physical footprint（含被内核压缩的内存页；`ps rss` 不含致严重低估——WKWebView WebContent 进程 ps 报 47M 实际 footprint 175M）+ WebKit XPC 子进程 footprint 合计（按启动时间关联主进程 ±10s 的 `com.apple.WebKit.*` 进程），扩展子进程识别用 `ps -A` 全表扫描（按可执行路径 `comm` 匹配 `com.litiantao.voidnix/extensions/<id>/`，按扩展分组；不依赖 PPID 链——root 子进程如 mihomo 由 launchd LaunchDaemon 托管 PPID=1；用 `comm` 而非 `command`，从根上排除 grep/osascript 等仅在参数里引用该路径的进程，也避免采样器自身 fork 的 shell 被误匹配）。主进程 CPU >80% 时用 `sample <pid> 2 -mayDie -file` 抓 2 秒调用栈快照（5 分钟冷却防刷屏），快照存 `stacks/cpu-<时间戳>.txt`、主日志追加 `# [stack]` 关联行。日志与快照自动保留 30 天。
- `voidnix-monitor-install.sh install|uninstall` — 安装/卸载 LaunchAgent（`com.litiantao.voidnix.monitor`，登录后自动生效）
- `voidnix-analyze.sh [天数]` — 分析器：按天聚合主进程 footprint 区间/漂移/CPU 峰值/线程数/数据目录（漂移 >20MB 告警）、WebKit XPC 合计 footprint 区间/漂移（漂移 >50MB 告警 compositing layer 累积），并按扩展聚合子进程采样数/RSS 区间/CPU 峰值

**日志**：`~/Library/Logs/Voidnix/monitor-YYYY-MM-DD.log`，主进程行（`time fp cpu threads data`）后跟 `& webkit fp_total`（WebKit XPC 合计，按启动时间关联）+ `@ ext/bin rss cpu vsz`（扩展子进程）；抓栈触发时追加 `# [stack] time cpu= -> stacks/cpu-时间戳.txt`（注释行，analyze 跳过），快照存 `~/Library/Logs/Voidnix/stacks/`

**状态**：LaunchAgent 已安装运行，主进程已有 5 天数据；子进程采样维度已上线，CPU 阈值抓栈已上线。曾暴露两个问题：(1) proxy/mihomo CPU 持续 100%——根因为 gvisor TUN 栈在睡眠唤醒后的连接风暴中泄漏 dial goroutine 进入 busy-loop，已将 TUN stack 切 system 栈彻底解决（见 [proxy.md](docs/extensions/proxy.md)）；(2) 主进程 CPU 100% 反馈环——blur → hideWindow → resignKeyWindow → 派生 blur 失控循环，已加 `hide_window` 的 `is_window_visible()` 幂等守卫断环。WKWebView 内存累积——搜索 compositing layer tiles 随搜索累积（PURGE=N 不可回收），已加 `ContentView.clearCache` 在窗口隐藏时卸载 KeepAlive 释放扩展视图 layer backing + toggle `content-visibility:hidden` 释放结果列表 tile backing（结果列表 DOM 保留以消除唤起空态闪烁），由 WebContent 350M 阈值 navigate 兜底。执行 `bash scripts/voidnix-analyze.sh` 分析趋势，`python3 scripts/smoke-test.py --perf` 做内存累积压测（内存结论只看 release；dev 模式 graphics 虚高见冒烟测试节）。

## 开发扩展

所有扩展同构（`extensions/<id>/index.ts` + 可选 `config.ts` + 可选 `native/`），详见 [docs/extensions.md](docs/extensions.md)。

含 native/（16）：clipboard、screenshot、video、awake、clean-mode、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search、proxy、system-status、ai-providers、image、homebrew

纯 TS（8）：calculator、settings、ip、base64、time、uuid、currency、notes

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)、[agent](docs/extensions/agent.md)、[ai-providers](docs/extensions/ai-providers.md)、[clean-mode](docs/extensions/clean-mode.md)、[proxy](docs/extensions/proxy.md)、[video](docs/extensions/video.md)、[image](docs/extensions/image.md)、[finder-ext](docs/extensions/finder-ext.md)、[window-manager](docs/extensions/window-manager.md)、[homebrew](docs/extensions/homebrew.md)。

## 架构要点

### 前端 ↔ Rust 端通信

- 命令名常量集中 `src/commands.ts`（`CMD.xxx`），**禁止裸 `invoke('xxx')`**，统一走 `invoke<T>(CMD.xxx, {...})` + 手写类型（`types/` 与各扩展）
- CI 双向差集校验：`scripts/check-commands.ts` 对 Rust `#[tauri::command]` 名集合 ↔ `commands.ts` 常量
- 扩展 Command 与框架 Command 统一在 `configure_app!` 的 `generate_handler!` 全局注册（sync-extensions 扫描生成，前端裸名 invoke）
- 流式/事件用 `app.emit()` 或 `tauri::ipc::Channel<T>`（agent 用后者）
- 含动态 JSON 的 Command（如 agent_run 的 `Channel<AgentEvent>`）手写 TS 类型（`src/types/agent.ts`）

### 扩展接口

`Extension`（`src/runtime/types.ts`）= `meta` + **13 槽**（10 能力槽 + 3 行为槽，按需声明，均有真实消费者）+ `setup` 生命周期。槽位语义与消费者计数详见 [docs/extensions.md](docs/extensions.md)。

### 搜索引擎

`src/runtime/search-engine.ts`：流式增量召回（消除快结果等慢结果的 barrier）→ 一次预算 finalScore → keyword 合流 → dedupe → groupAndSort。每个扩展 `emit`/`resolve` 都同步触发增量重排，`onUpdate` 经 rAF 批量合帧回调（同帧多扩展结果合并为一次渲染；全部同帧 resolve 时 rAF 被 cancel、结果经 return 值投递），应用缓存秒出、内存索引文件结果随打随出。

**两种模式共用 `search()`**：

- **全局模式**：并行调所有扩展 dynamic → finalScore 排序（`fuzzy + boost`）→ keyword 合流 → 分组排序
- **扩展模式**（`setActiveExtension` 切换）：只调激活扩展 dynamic，bypass groupAndSort 保留扩展返回序；同样受 timeout/abort 保护

**模式快照**：`search()` 入口捕获 `activeExtension`，await 期间切换不影响本次后处理。

**超时**：每扩展独立 child `AbortSignal`，超时只 abort 该扩展（不牵连其它），父 signal abort 时同步取消。

**过滤规则**：

- 空 query：默认列表按 `finalScore>0`（boost>0，主要是应用）
- 非空 query：查找型结果需 `fuzzy>0`，extension 类即时答案靠 `finalScore>0` 穿透

**扩展入口打分**：keyword / `/` 工具列表共用 `scoreExtensionEntry`（name/id/description 正向 + keywords 双向）。

`SearchContext.extensionMode` 供扩展区分场景：全局即时答案 calculator / currency / base64（base64 仅解码，设 minLength 门槛过滤短词误触）；ip / time / uuid 等仅扩展内响应。详见 [docs/extensions.md](docs/extensions.md)。

### 窗口

`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。

**样式**：`platform/window.rs::apply_main_window_style`（setup 内一次性）= `apply_mica_material(ns, 16)` + `setHasShadow(true)` + `convert_to_panel`。即 Mica + contentView 圆角 16（`radius-window`）+ 原生阴影 + 冷雾 tint（见 [设计系统](docs/design.md)）。snap-panel 经 `apply_mica_material(ns, 10)` 对齐 `radius-panel`。appearance 跟随主题（`apply_window_appearance`：auto=None 跟随系统并驱动 WKWebView prefers-color-scheme，light/dark 强制覆盖）。**跨窗口**：`set_window_appearance` 是全局副作用命令（一次应用所有窗口），仅由 main 的 `theme.ts` 驱动；invisible 创建的子窗口（screenshot/snap-panel）由 Rust 经 `apply_cached_appearance` 设原生 appearance；pin 窗口 visible 创建不可设（setAppearance 在刚 build 的 WKWebView 上触发 prefers-color-scheme 重算死锁主线程），改由前端读 `get_cached_appearance` 命令拿 main 缓存值直接设 DOM data-theme。

**panel 转换**：`platform/panel::convert_to_panel` 转 `NonactivatePanel`（点击/makeKey 不自动激活）。

**show 策略**：

- 不 `activate_app`（保持原前台 active，避免聚焦视图/菜单栏突变；代价是 macOS 26 上偶发下层 hover 穿透——产品优先不打断）
- hit-test 靠 `capture_mouse_events` + SkyLight event shape；`present_on_cursor_screen` 中 **先 `capture_mouse_events` 再 `setAlphaValue`**（避免窗口可见但仍 ignoresMouseEvents 的间隙导致滚动穿透），`orderFront` 后重设 event shape；`show_main` 末尾延迟 150ms 再刷新一次（兜底菜单栏关闭后窗口服务器 hit-test 滞后）
- `present_on_cursor_screen`：光标屏居中并写 `PLACEMENT_VIS`
- `animate_frame` 在 `PLACEMENT_VIS` 内改尺寸，保留用户拖动后的水平位置（宽度变化以当前中心为轴），跨屏异常才复位居中
- **窗口拖动**：chrome 带（搜索栏周围空白间隙）设透明拖动层（z-5，搜索栏 z-10 之下），手动 `startDragging()` 替代 `data-tauri-drag-region`——后者 macOS 双击触发 `internal_toggle_maximize`，对 resizable:false 无标题栏窗口会直接填满全屏；拖动仅影响当前显示期间，每次 show 经 `present_on_cursor_screen` 自动复位
- 截图 overlay 等独占场景才显式 `activate_app`
- **剪贴板填充**：主快捷键从隐藏唤起时派发 `window-invoked`（DOM 事件），`useSearchInput` 查剪贴板最新记录，文本类且 3 秒内则填充搜索框并 select（`disableSearchInput` 扩展跳过）

**hide 策略**：

- 不 orderOut（仅 alpha=0 + ignoresMouse + 去阴影）——orderOut 后副屏二次 show 坐标对也不绘
- 主窗 Space 只 Add；collectionBehavior = `CanJoinAllSpaces|FullScreenAuxiliary`（勿并 MoveToActiveSpace）
- `hide_window` 命令入口幂等守卫 `is_window_visible()`——blur → hideWindow → resignKeyWindow → 派生 blur 反馈环在首轮 hide 后断开（原 auto 防抖 500ms 无法断环，窗口可见时间通常远超 500ms）
- 隐藏时 `ContentView` 监听 `window-hiding` 事件将 KeepAlive 卸载重建（`keepAliveActive` 置 false → `nextTick` → forced layout flush → 置 true），释放扩展视图缓存 DOM + WKWebView compositing layer tiles（IOSurface backing store，PURGE=N 不可回收）；同期 toggle `content-visibility:hidden` on contentRef 释放结果列表 tile backing（DOM 保留，show 时 compositor 同步恢复 pending 变更不闪烁）；**不清空 results**——主快捷键由 Rust 直接 show 窗口（前端 IPC 回调在 show 之后），清空 results 会导致第一帧渲染空态产生闪烁，保留 DOM 使唤起时列表立即可见，`focusHandler` 后台 `loadDefaultResults` 刷新补增量
- KeepAlive `max=3`（日常高频 agent/settings/proxy 不超过 3 个同时活跃），隐藏时全量清空
- **WebContent 内存阈值重载**：`hide_window` 后 detached OS thread（不占 tokio worker）异步查 `platform/mem.rs::webcontent_footprint`（`proc_pid_rusage` 读 WebContent XPC 的 physical footprint，按启动时间下限关联主进程——不设上限，覆盖 navigate 重载/crash 恢复后创建的新进程），超 350M 时 Rust 直接 `navigate("about:blank")` → 100ms → `navigate(原 URL)`——`reload()` 不释放 tile backing（IOSurface），必须先 blank 销毁旧 layer tree 再重建（等同 Safari 内存压力 tab 重建）。纯 Rust 闭环，无 command 注册、无前端事件。超阈值先等 3s 复测（agent 流式的易失性合成面峰值会自行回收，不该触发进程重建）且窗口仍隐藏才重载（hide → show → 重载竞态守卫）；重载守卫（`window::register_reload_guard`，扩展 setup 注册，agent 消费——run 进行中否决重载直到 run 结束，隐藏不打断流式输出）+ hide 代次计数（多次 hide 的等待线程只有最新者执行重载）。navigate 重载后 `main.ts` 重新执行，自测的二次触发由 `test.rs` 的 AtomicBool 一次性守卫防御；重载清零全部 JS 模块单例，需跨重载存活的扩展状态须自行落盘（agent 会话：messages/sessionId 落扩展 config，boot 回填后恢复并 abort 孤儿 run，见 [agent.md](docs/extensions/agent.md)）；激活扩展由 app store 经 sessionStorage 恢复（同浏览会话跨导航存活，冷启动新会话不恢复），重载后回到隐藏前视图
- `hide_main` 走 `restore_captured()` 交还 first responder（`PREV_FRONT_PID` 唯一源在 `platform/focus.rs`）

**焦点管理**——`is_app_active()` 四道判定：

1. NSApp keyWindow 非空 → 焦点在我们
2. frontmost bundle 路径 `/System/` 开头（授权弹窗、keychain 对话框等）→ 交互流未中断
3. frontmost 为 Voidnix 自身（WKWebView 聚焦可编辑元素触发的自我激活瞬态）→ 交互流在自己身上，不触发 blur hide
4. `OSASCRIPT_RUNNING` 标志（osascript 授权后续 shell 命令执行期间 frontmost 已还给原 app 但仍抑制 blur hide）

焦点恢复细节：

- `restore_captured()` 还原前查 frontmost：第三方已接管（系统弹窗/用户切到其他 app）则不抢回
- 系统弹窗关闭后由 `platform/frontmost_watcher`（NSWorkspace 激活通知观察器，随 show/hide 生命周期 add/remove，回调转主线程执行）处理：
  - frontmost == 原前台 PID → `makeKeyWindow` 恢复
  - frontmost == Voidnix 自身 → 激活事务可能短暂夺走 panel key，`makeKeyWindow` 恢复（置于 is_app_active 守卫之前，自我激活时守卫恒真）
  - frontmost != 原前台 PID → 用户主动切换 → emit `frontmost-changed` → 前端 dismiss
- **WKWebView 可编辑元素聚焦会激活应用**：应用未激活时对 textarea 等执行 focus() 触发 `activateIgnoringOtherApps` 抢走前台（违背 show 不抢 active）；激活事务的 key 重评估偶发夺走 panel key → 派生 blur 藏窗（agent 快捷键直开后约 0.5~1.5s 自行隐藏的根因，曾误报为「点击输入框隐藏」）。三重防线：`is_app_active` 判定 3 拦截自伤 blur 的藏窗；watcher 自身激活分支恢复 key；disableSearchInput 扩展（agent/notes/translate）不在窗口隐藏时聚焦（mount 时 `document.hasFocus()` 为假跳过，`window-focused` 事件补聚焦；补聚焦不设 hasFocus 守卫——事件先于 WebKit 页面焦点状态翻转到达，查则快捷键唤起路径永久错过，事件语义即窗口已 key 聚焦安全）。配套 `capture_frontmost()` 遇 frontmost=自身时保留上次记录（不写 0），防 prev 失效误判

**窗口高度**——扩展声明 `windowHeight`（`number` 固定 / `'auto'` 自适应 / 未声明默认 480），subview 可经 `subviewHeights` 覆盖：

- `useExtensionHeight`（MainView 全局唯一调用）读 `activeExtension` + `activeSubview` 解析模式
- **adjust 可见性守卫**：`windowVisible`（focus/blur 驱动，初始 false）为 false 时 adjust 跳过 `set_main_frame`——不可见时提前改高度会让 WKWebView viewport 与 NSWindow frame 不匹配（present 后 footer 仍按旧 viewport 底部定位、悬在窗口中间）。守卫后 present 用上次稳定高度（viewport 匹配），show 后 focus 触发 adjust，渐进 animate 到目标高度——animator `display:YES` 逐帧驱动 NSView resize，WKWebView viewport 有时间每帧跟随同步，footer 始终贴底（视觉连续的撑大动画，而非瞬间跳变）。adjust 读 `outerSize` 实际高度，fixed/default 已等于目标则跳过 invoke（回填 `lastApplied`），消除多余 reflow
- 一次 invoke 触发 Rust `set_main_frame` → `animate_frame` 用 `NSAnimationContext` + `animator setFrame:display:animate:` 系统级动画（CoreAnimation 接管，非 JS rAF 逐帧）
- **动画后延迟重刷 event capture**：animator 扩高（顶边固定向下生长）后窗口服务器 hit-test 表可能停留在动画前矩形——新增的底部区域点击穿透到下层应用，激活对方触发 blur 藏窗（agent 快捷键直开后点击输入框隐藏窗口的根因）。`set_main_frame` 自 invoke 起 400ms（动画 0.26s + 余量）后经 `refresh_event_capture_if_visible` 重设 ignoresMouseEvents + event shape 对齐最终 frame
- `auto` 模式：ResizeObserver 监听 `contentRef`，窗口高 = `CHROME_HEIGHT`（搜索栏 + 间距）+ 内容高，clamp `[DEFAULT_HEIGHT, 屏幕高 90%]`
- 屏幕尺寸走 `currentMonitor`（WKWebView 下 `window.screen` 仅返回 webview 视口）
- 底部将出屏（含 40px 间距）则同步上移；离开 auto 还原进入前位置

### 全局快捷键

`runtime/shortcut.rs`，快捷键 id 驱动（前端传 id + shortcut，Rust 自管注册表 + 录制模态 + 扩展钩子）。

- 默认 Option 基：`Option+Space` 呼出，`Option+C/S/T/A/F/N` = 剪贴板 / 截屏 / 翻译 / Agent / 访达工具 / 记事本
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

### 国际化（i18n）

`runtime/i18n.ts`：零依赖自研轻量 i18n，基于 Vue 3 原生响应式（与 `theme.ts` 同范式）。两语言 zh-CN / en，无复数/日期格式化需求。

**核心机制**：模块级 `locale: Ref<Locale>` + `t(key, params?)` 函数。`t()` 内部读 `locale.value`，在模板/computed 中调用时自动建立响应式依赖——切换语言时所有 `t()` 调用点自动重渲染。

**API**：

- `t(key, params?)` — 翻译，回退 zh-CN → key 本身；`{param}` 占位符插值
- `resolveLocalized(text)` — 解析 `LocalizedText`（`string` 通用 / `Partial<Record<Locale, string>>` 按语言区分），回退 zh-CN → 首个可用值
- `registerMessages(msgs)` — 注册文案（合并），与 `defineExtension()` 同范式——import side-effect 注册
- `initI18n()` — main.ts 调用（与 `initTheme()` 同范式），从 settings 读取语言 + watch 驱动；子窗口读 localStorage + 监听 storage 事件

**文案组织**：

- **框架文案**：`src/locales/zh-CN.ts` + `en.ts`（搜索/设置/对话框/通用 toast/空态/更新弹窗等 ~90 条），`index.ts` 启动注册
- **扩展文案**：每个扩展目录 `locales.ts`，`index.ts` 顶部 `import './locales'` 注册（import side-effect，与扩展注册同范式）
- **扩展元数据**：`meta.name` / `meta.description` / `placeholder` 类型为 `LocalizedText`（纯 string 向后兼容）

**语言切换**：`settings.ts` 的 `language: Locale` 字段（默认 zh-CN），设置页 select 切换。切换即生效——所有 `t()` 调用点响应式重渲染。

**新增文案 key 约定**：框架级加到 `src/locales/`，扩展级加到自身 `locales.ts`。命名空间约定：`clipboard.empty`、`agent.placeholder` 等。复用 `common.*` 框架通用 key（cancel/confirm/copied/save/loading/noResults 等）。

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

### 扩展视图加载（切换性能 + 内存）

- 扩展 View（mainView/subviews/searchBarAccessory）**静态 import** 进主 bundle（用户高频、固定集合，首次进入零卡顿）
- **独立窗口多入口**：screenshot / snap-panel / pin 窗口各自有独立 HTML 入口（`screenshot.html` / `snap-panel.html` / `pin.html`）+ 轻量 TS 入口（`src/entries/`），只加载自身组件 + vendor + 主题 CSS，**不加载**扩展注册表 / pinyin / markdown / 搜索引擎。每个 WebContent 进程 JS 从 ~700K 降至 ~86-142K
- **窗口按需创建**：screenshot 窗口首次截图时懒创建（`setup` 内 `WebviewWindowBuilder`，WKWebView 预加载页面）；snap-panel 窗口在 `set_window_manager_enabled(true)` 时懒创建，分两步避免主线程死锁：`build_snap_panel`（仅 `WebviewWindowBuilder::build`）在 worker 线程调用（build 内部 dispatch 到空闲主线程；**禁止**放进 `run_on_main_thread` 同步闭包——build 的 dispatch + 同步等待与执行闭包的主线程死锁，导致 UI 无响应），`configure_snap_panel`（`apply_mica_material` 需 `MainThreadMarker`）在 `run_on_main_thread` 闭包内调用；`false` 时**不销毁窗口**——WKWebView teardown 抛 C++ foreign exception（非 ObjC NSException，`objc2::exception::catch` 无法拦截），前端 `close()`/`destroy()` 均 abort 进程，改为仅停 drag monitor + 隐藏窗口（alpha=0），窗口保持存活，重新启用时 `build_snap_panel` 幂等跳过 + `configure_snap_panel` 重配置）；pin 窗口每次钉图创建、关闭时销毁
- **子窗口主题**：独立入口窗口用 `runtime/child-theme.ts::initChildTheme`（无 Pinia 依赖，读 `get_cached_appearance` + 监听 `appearance-changed`），不初始化扩展系统
- **vendor 分包 + pinyin 延迟加载**：`manualChunks` 拆 vendor(vue) / markdown(marked+dompurify) / pinyin 独立 chunk；pinyin-pro（拼音字典 289KB）改为首次 CJK 查询时 `import()` 异步加载，首屏零开销
- `ContentView` 用 `KeepAlive`（max=3，LRU 驱逐）缓存已访问扩展，切换走 activate/deactivate 而非重挂载

### LLM 基础设施

`runtime/llm/`，agent + translate 扩展共享：

- `types.rs`：LlmMessage
- `client.rs`：StreamConfig / `stream_openai_request` + SSRF 防护 `validate_ai_request` + 消息截断 + 请求管道常量 + SSE 断流检测（服务端错误负载提取——GLM 内容审查 1301 等以裸 JSON 行下发，无 data: 前缀无终止空行，须从事件体 / EOF 残留 buffer 提取真实原因上抛；流 EOF 且无 `[DONE]` 无 `finish_reason` 才判 premature，不把截断输出当正常完成）
- `parser.rs`：tool_calls 解析

### AI 凭证中枢

`src/runtime/ai-providers.ts` → `config/ai-providers.json`：只存 URL/Key/模型，**无「使用中」**。

- **选用自管**：agent（`providerModelKey`）/ translate（`selections`）各自持久化选用
- **同步机制**：`isCredentialSelectionValid` + 读时 effective（不写回）+ 启动/写入冷 prune
- **env 输出**：写 `ai.env`（`VOIDNIX_ZHIPU_*` / `VOIDNIX_DEEPSEEK_*` 等私有名，`*_BASE_URL` / 可选 `*_RESPONSES_URL` 按提供商去重）；仅 release 注入 shell，debug 只写文件
- **Shell rc 注入**统一走 `runtime/shell_rc`（`# voidnix <scope>`），见 [shell-rc.md](docs/shell-rc.md)
- 详见 [ai-providers.md](docs/extensions/ai-providers.md)

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口（lib.rs：自定义 tokio 运行时 4 worker + setup 内含启动埋点，debug 打印 [boot] 各阶段耗时 + <100ms 判定）
├── extensions.rs       # 自动生成（configure_app! + register_all 生命周期 + generate_handler! + mod 声明）
├── http.rs             # HTTP 客户端 + http_get 命令
├── runtime/            # 运行时核心（平台无关）
│   ├── autostart.rs   # 开机自启命令薄壳（SMAppService Login Item 注册/查询）
│   ├── window.rs       # 主窗口 show/hide
│   ├── shortcut.rs     # 快捷键 + 录制
│   ├── menubar.rs      # 聚合菜单栏托盘（框架唯一图标 + 扩展贡献段注册）
│   ├── storage.rs      # TempHandle RAII + ext_data_dir + save_png_safely
│   ├── test.rs         # 自测模式判定（环境变量 VOIDNIX_SELF_TEST + AtomicBool 一次性守卫防 navigate 重载后二次触发）
│   ├── permission.rs   # 系统权限命令薄壳（同步；screen_recording 走 preflight 不截屏）
│   ├── registry.rs     # Extension trait + ExtensionRegistry（concurrent bootstrap；单扩展 setup 失败隔离；阻塞 I/O 扩展自管 spawn_blocking）
│   ├── pasteboard.rs   # 框架命令薄壳（write_text / paste_text；原语在 platform/pasteboard）
│   ├── binary_fetch.rs # 外部 binary 下载管线（流式落盘 + sha256 + 多 URL 回退；proxy/video 共用）
│   ├── speech.rs       # 语音朗读命令（say CLI 封装；translate 消费）
│   ├── shell_rc.rs     # .zshrc 注入约定（# voidnix <scope> marker）
│   └── llm/            # LLM 基础设施（types / client / parser）
└── platform/           # macOS 原生桥（零业务语义）
    ├── autostart.rs    # SMAppService（macOS 13+）注册主 app 为系统 Login Item（objc2 调用）
    ├── panel.rs        # NSPanel 转换
    ├── skylight.rs     # Space 迁移（私有 API）
    ├── focus.rs        # 焦点管理（PREV_FRONT_PID + is_app_active + restore_captured）
    ├── input.rs        # CGEvent 键盘注入（post_key + post_combo）
    ├── mem.rs          # 进程内存查询（proc_pid_rusage → WebContent physical footprint）
    ├── pasteboard.rs   # NSPasteboard 原语统一
    ├── selection.rs    # AX 选中文本提取 + poll_clipboard
    ├── click_monitor.rs
    ├── frontmost_watcher.rs  # NSWorkspace 激活观察器（系统弹窗后恢复焦点）
    ├── distributed.rs  # NSDistributedNotificationCenter 跨进程事件总线（proxy TUN 让渡即时对账）
    ├── permission.rs
    ├── window_list.rs  # CGWindowList 共享封装（screenshot / window-manager 共用）
    ├── window.rs       # 主窗口原生操作（NSWindow + 圆角 + NSOpenPanel + appearance 缓存）
    └── path_guard.rs   # 统一路径校验
```

`http.rs` 细节：`HTTP_CLIENT` 整体 120s 超时；`STREAM_CLIENT` 无整体超时、仅建连 30s + 读间隙 120s（总时长不可控的长流共用：LLM 流式响应 + 大文件下载；读间隙超时兜底 stalled 连接）；`http_get` 命令含浏览器 UA 伪装 + SSRF 防护 + 重定向限制 + 共享 `parse_scheme_host`/`is_blocked_host` 原语（ip/currency 等纯 TS 扩展消费）。

```
src/
├── main.ts             # 入口（import.meta.glob eager 扫描扩展 + 注册文案 + 并行 setup）
├── entries/            # 子窗口独立入口（screenshot/snap-panel/pin，只加载自身组件，不经扩展系统）
├── commands.ts         # 命令名常量（CMD.xxx，禁止裸 invoke）
├── locales/            # 框架级文案（zh-CN.ts + en.ts + index.ts 注册）
├── runtime/            # 前端运行时
│   ├── types.ts        # Extension / SearchProvider / SearchResult（13 槽：10 能力 + 3 行为）
│   ├── constants.ts    # 语义常量单一源（SEARCH.WEIGHTS / GROUP_ORDER / KEYWORD_EXTENSION_BOOST + LIMITS）
│   ├── i18n.ts         # 轻量 i18n（locale ref + t() + resolveLocalized() + registerMessages + initI18n）
│   ├── storage.ts      # defineConfig（reactive + watch 自动持久化 + race 保护 + 类型守卫 + 退出 flush）
│   ├── self-test.ts    # 应用自测模块（环境变量驱动，搜索/扩展/命令/视图冒烟断言）
│   ├── extension-registry.ts  # defineExtension + getAllExtensions + getExtension
│   ├── search-engine.ts       # dynamic 单通道 + keyword 合流 + dedupe + groupAndSort
│   ├── ai-providers.ts        # 统一 AI 提供商/Key 中枢（agent/translate 消费）
│   ├── theme.ts               # 主题运行时（appearance 持久化 + 系统外观跟随 + 原生窗口同步）
│   └── child-theme.ts         # 子窗口主题（无 Pinia 依赖，读缓存 + 监听事件）
├── components/
│   ├── ui/             # 原子组件（只用这些，禁止手写底层标签）
│   └── layout/         # MainView / ContentView / ResultItem（kind 分支内聚）/ ResultIcon / ResultActionPanel
├── composables/
│   ├── useAppLifecycle.ts     # 主窗口生命周期（快捷键/失焦隐藏/扩展事件；主快捷键唤起派发 window-invoked）
│   ├── useSearchInput.ts      # 搜索编排（全局 searchEngine + 搜索型扩展 dynamic + web 搜索 + 默认结果 + 唤起剪贴板填充）
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

独立 Astro 子项目（单页落地页），中英双语（`/` 中文 / `/en/` 英文，URL 前缀路由），token 自动同步产品 `theme.css` 全量 `:root`。首页 Hero 内嵌实时动画演示（`DemoStage.astro` 组件，非视频——6 个独立 demo 段（各 160–350 帧可变）JS 驱动的拟物 macOS 桌面舞台，统一入场节奏可单段切换或连续拼接；启动器窗口忠实复刻真实应用界面 720×480。首页与 `/demo` 预览页共用）。i18n 字典 `src/i18n/translations.ts`（页面级，zh 类型源 en 同构校验）+ `src/i18n/demo.ts`（动画级），各组件接收 `lang` prop。文档：[site/README.md](site/README.md)（概览 / i18n / 开发 / 部署）+ [site/demo.md](site/demo.md)（分段架构、统一节奏、启动器复刻细节、控制栏、可选视频导出）。

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
- 语义 `danger|warning|success`（soft 浅色 12% / 深色 16%）
- **禁止**裸 hex 结构色、`black/*`、状态用 red-500（`mask-smoke`、标注调色板、文件类型 palette、图片预览叠加标识（image 序号徽标 `text-white bg-black/40`，主题无关的内容叠色）除外）

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

- **标准进出场**：动作面板 / 下拉 / 标注浮层等单元素走 `<Transition name="ui-popup">`（全局类在 `theme.css`，进 `--duration-fast` `--ease-out` / 退 `100ms` `--ease-in`，位移 8px + 缩放 .95）。`ui-popup` 用 `transform` 属性做动画，与 UnoCSS `translate-*`/`scale-*`（Wind4 落独立属性）正交叠加，带 `-translate-x-1/2` 居中定位也不冲突
- **toast 堆叠**：`TransitionGroup name="toast"`（专用过渡，enter 8px+scale.95 弹入 / leave 4px+scale.98 柔和退场让 opacity 主导 + FLIP `move` 平滑重排 + `leave-active` 脱离文档流），容器 `items-end` 使每条 toast 保持自身宽度不互相拉伸，消除多条不同长度时离场的布局抖动；`.toast-move` 源码序必须在 enter/leave-active 之前（transition 是 shorthand，后定义的 leave-active 须覆盖 move 的 transition 以保留 opacity 过渡）；`leave-active` 用 `position: fixed`（非 absolute）+ `@before-leave` 钩子写入视口坐标 `left/top/width` 锁定原位（容器 `fixed bottom` 从底向上缩短，absolute 元素的静态位置会漂移到流尾）
- **方向变体**（如 `BaseSelect` 上下展开）直接用 `transition`（UnoCSS 默认 property 列表已含 `translate,scale,opacity,transform`，覆盖 Wind4 独立属性的 from/to）；**禁止** `transition-[a,b,c]` 方括号多值语法——Wind4 不生成该规则，类为空致无过渡瞬时跳变。单属性可用 `transition-[opacity]`
- **数值走基元**：自定义过渡的时长 / 曲线一律 `var(--duration-*)` / `var(--ease-*)`（`--duration-fastest` 100 / `--duration-fast` 150 / `--duration-normal` 200 / `--duration-slow` 300；UnoCSS 工具类用 `duration-[var(--duration-*)]` 方括号形式——圆括号变量简写 `duration-(--x)` 在 presetWind4 下不生成规则、静默失效），禁止裸 `0.2s` / `duration-300` / `cubic-bezier(...)`。布局伸缩等 CSS 无法表达的用 JS hooks（`image` 扩展 `expandHooks`）
- **仅 GPU 合成属性**：过渡只用 `transform` / `opacity` / `translate` / `scale`（独立属性），禁用 `box-shadow` / `background-color` 等 paint 类属性（每帧重绘致顿）。`ui-popup` 用 `transform` 属性——与 UnoCSS `translate-*`/`scale-*`（Wind4 落独立属性）正交叠加，带 `-translate-x-1/2` 居中定位也不冲突
- **入场动画 fill-mode 用 `backwards` 禁 `both`**：`both` 把结束帧 transform 永久驻留，「有 transform」即合成层，列表/消息级元素会每项常驻一个 IOSurface（agent 会话曾因此 +80MB/十条消息）；`backwards` 延迟期显起始帧、结束后零驻留，层随动画结束降级
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
    ├── notes/config.json             # 记事本内容（content key，自动暂存）
    ├── zsh-autosuggestions/{bin/, index.zsh, signals.log, bin.version, config.json}  # zsh 补全
    ├── awake/{Display Wakelock, config.json}   # awake binary + 配置
    ├── screenshot/config.json
    ├── window-manager/config.json
    ├── translate/config.json
    ├── agent/config.json             # 资源上限 + systemPrompt + 搜索 Provider + 会话消息/ sessionId（AI Key 见 ai-providers.json）
    ├── video/{ffmpeg,ffprobe,ffmpeg.version,config.json}  # 按需下载的静态 ffmpeg/ffprobe + 配置
    ├── image/config.json             # 输出目录配置（移除背景/拼接结果）
    └── proxy/{mihomo, mihomo.log, mihomo-daemon.plist, geoip.metadb, geosite.dat, config.yaml, subs/, config.json}  # TUN 模式 launchd 托管（plist 装于 /Library/LaunchDaemons）
```

icon 缓存纯内存（首次提取后按 bundle mtime 增量复用，零磁盘文件）。dev 镜像 `com.litiantao.voidnix.dev` 同构。

所有 config.json 均走 `defineConfig`（`src/runtime/storage.ts`）：reactive + watch + 300ms 防抖（持续变更 2s 强制落盘防饿死）+ 深克隆 + race 保护 + 类型守卫 + 退出 flush。不订阅 plugin-store `onChange`（set 会向本进程回放 `store://change` 无来源标识，实测复现：回灌旧快照覆盖 emit 到达前已 mutate 的新值）；所有 config 仅在 main 窗口持有，无跨窗口同步需求。schema 变更优先删磁盘 config.json；AI 中枢对旧 agent/translate 凭证字段做一次性 best-effort 导入（见 ai-providers）。

## 约定

- 开发环境基线：macOS 26（Apple Silicon，arm64），基于当前系统版本开发——优先采用现代 API，禁止为旧 macOS 写兼容分支或降级逻辑（私有 API / 私有 framework 直接用，不裹版本探测）
- 环境：`isTauri` 判断环境（常量，非函数），非 Tauri 跳过原生调用
- UnoCSS Attributify：不确定的工具类语法先用 context7 查 UnoCSS 文档确认，勿靠翻 dist 源码或试错猜语法
- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`，自定义 tokio 运行时 4 worker（默认按逻辑核心数）
- Git commit：`<type>(<scope>): <中文描述>`，描述力求最简，不写详情，不主动执行 git 操作；**提交前必须先跑 `bun run precommit` 且全绿**
- 语言：注释和回复用中文，禁止在任何地方使用 emoji
- 文档：不用表格，言简意赅，修改代码后必须同步更新 AGENTS.md 或对应 docs/ 文档中相关描述
