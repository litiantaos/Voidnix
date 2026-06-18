# Voidnix v2 重构执行蓝本

> 本文档是 Voidnix 应用底层架构彻底重设计的完整执行蓝本。自包含、可复现，新会话仅凭此文档即可推进。所有决策已锁定，按阶段顺序执行即可。

## 执行进度跟踪

分支：`refactor/v2`

### 阶段 0：创建分支 + 提交蓝本 ✅
- commit `6270c28` docs(refactor): v2 重构执行蓝本

### 阶段 1：Rust 内核重建 ✅（含精细化完成）

**已完成**：
- commit `cbf5624` refactor(runtime): Rust 内核目录结构重建
- commit `0b9fa18` refactor(focus): 统一 PREV_FRONT_PID 到 platform/focus 唯一源
- commit `df73a72` refactor(registry): Tier1Extension 改名为 Extension + 加 deps/teardown
- commit `17c14f7` refactor(llm): sse.rs 拆分为 types/security/client 三模块
- commit `d857374` refactor(search): 消除 icon 磁盘缓存，改实时提取
- commit `da46e59` refactor(translate): SELECTED_TEXT 下沉到 translate 扩展自管
- commit `bf17276` refactor(platform): 统一路径校验到 platform/path_guard
- commit `86d2e66` refactor(platform): 拆分 selection.rs → input.rs + pasteboard.rs

**精细化状态**：
- [x] SELECTED_TEXT 下沉 translate
- [x] icon_cache 删除（零磁盘文件）
- [x] sse.rs 拆分为 client/types/security
- [x] selection.rs 拆分为 input（键盘注入统一）+ pasteboard（NSPasteboard 统一）
- [x] path_guard 新建统一路径校验
- [ ] build.rs 扫描化（低优先级，可并入阶段 2）
- [ ] storage.rs 新增 TempHandle（待 clipboard/screenshot 扩展迁移时做）

**编译状态**：`cargo check --lib` 零错误零警告
**测试状态**：`cargo test --lib` 77 passed; 0 failed

### 阶段 2：Rust 扩展迁移 ⬜（未开始）
### 阶段 3：前端运行时重建 ⬜（未开始）
### 阶段 4：扩展迁移 ⬜（未开始）
### 阶段 5：测试 + 工具链 + 文档 ⬜（未开始）
### 阶段 6：验证 ⬜（未开始）

---

## 0. 文档说明

- **目标**：彻底完全重新设计应用底层架构，追求极致的结构清晰、优雅、轻量、高性能、低占用。
- **原则**：极简主义、强迫症、精神洁癖；第一性推导、一步到位、不考虑兼容性与历史包袱。
- **范围**：Rust 后端（src-tauri/src/）、前端（src/）、全部 16 个扩展、构建工具链、文档。
- **执行方式**：在 `refactor/v2` 分支，按 6 阶段顺序一次性完成，每阶段可独立编译验证。
- **当前分支**：开始执行前先 `git checkout -b refactor/v2`。

---

## 1. 项目背景与现状审计

### 1.1 现有架构（v1）

三层扩展架构：

- Tier 0（框架）：编译期 Rust + Vue，`src-tauri/` + `src/`
- Tier 1（内置扩展）：编译期 Rust + Vue，`extensions/<name>/`
- Tier 2（第三方扩展）：运行时纯 JS，Worker 沙箱，`extensions/<name>.vnext/`

代码规模：Rust 框架 3986 行 + Rust 扩展 9421 行 + 前端 7278 行 + Tier2 JS 366 行 ≈ 21k 行。

### 1.2 核心问题清单（重构动机）

按严重度排序：

**架构边界错误**

1. `core/agent/` 788 行过早抽象：唯一消费者是 agent 扩展；`DEFAULT_SYSTEM_PROMPT` 把业务语义（"你有 web_search/run_command 工具"）硬编码进框架层；`MAX_TURNS=10` 不可配置。
2. `stores/settings.ts` 586 行是扩展配置倾倒场：`namespace<T>` 机制设计了却零调用，7 个扩展私有配置全硬塞（clipboard.maxDays、screenshot.savePath、translate.configs、agent.trustedCommands 45 项、wm 尺寸、finder-ext/zsh-as/awake enabled 开关）。
3. `macos/text_selection.rs` 259 行杂物间：AX 选择 + 剪贴板快照 + 键盘注入 + 调试日志四件事混一起，且是 translate 专属却写在框架层。
4. `infra/sse.rs` 539 行名不副实：混了 SSRF 防护 + 消息安全 + LLM 协议类型 + SSE 流四层语义。
5. `infra/path.rs` 的 `icon_cache_dir` + `cleanup_icon_cache` 是 search 扩展的家务却写在框架。
6. `core/shortcut.rs:8 SELECTED_TEXT` 是 translate 扩展的数据中转站，泄漏到核心。
7. lib.rs setup 闭包混入扩展专属配置（snap-panel 窗口、screenshot 禁阴影、icon 缓存淘汰、ax_timeout）。

**重复实现**

8. 三套剪贴板读取：`macos/text_selection.rs::read_clipboard_ns`、`clipboard/monitor.rs`、`translate/mod.rs::pbpaste` 子进程。
9. 三套 CGEvent 键盘注入：`text_selection.rs::inject_copy`（core_graphics crate）、`clipboard/commands.rs::simulate_cmd_v`（裸 extern）、`text_selection.rs::post_key_to_pid`（第三种）。
10. 四处独立 `PREV_FRONT_PID`：`core/shortcut.rs:11`、`core/window.rs:6`、`screenshot/session.rs:330`、`window-manager/window_snap.rs:67`，shortcut 写、window 读、可能不一致。
11. 三处 markdown 正则渲染：DeclarativeMarkdown.vue、DeclarativeDetail.vue、DeclarativeStream.vue。
12. 两套 setup 路径：`init() -> TauriPlugin` 的 `.setup()` 钩子（search）vs `Tier1Extension::on_setup`（clipboard/translate/screenshot）功能重叠。

**横向耦合与扩展性缺陷**

13. `finder-ext/native/mod.rs:408` 直接调 `crate::extensions::screenshot::cleanup_temp_files()`，违反扩展隔离。
14. `BaseList.vue` 原子组件直接 `useAppStore()` 读 `activeModuleId`，业务泄漏进 UI 原子层；`querySelector('[data-settings-control]')` 让通用列表知道设置项控件存在。
15. 扩展零单元测试：screenshot/translate/clipboard/finder-ext/awake/search/settings/calculator/window-manager 全无。
16. 能力槽封闭：搜索机制三套（onSearch/onModuleSearch/searchItems），UI 槽位固定枚举，无扩展点。

**硬编码配置（违背「避免硬编码」）**

17. 搜索权重（模块+500/应用+300/文件夹+80）、分组上限（MAX_APP_RESULTS=30/MAX_FILE_RESULTS=50）、超时（MODULE_SEARCH_TIMEOUT=3000）散落 module-registry.ts。
18. fuzzy.ts 全文权重（prefix 1000/contains 600/衰减 0.85/log2×50/上限 320）硬编码。
19. agent 9 层防御参数（白名单/rlimit 30s/512MB/64fd/timeout/1MiB）全硬编码，用户无法调整信任范围。
20. icon cache 淘汰策略（400 文件/90 天）硬编码 infra/path.rs。
21. clipboard 500ms 轮询、5000 行限制硬编码 monitor.rs。
22. 各种防抖（500ms/300ms）硬编码 App.vue 与 shortcut.rs。

**垃圾文件与冗余**

23. `search/icons/` 400 个独立 PNG 文件（用户 Library 下 inode 浪费）。
24. `finder-ext/commands/cmd_*.json` 每次 IPC 操作产生一个文件，无限累积。
25. `zsh-as/signals.log` 无限增长。
26. `awake` binary 写 `/tmp/com.litiantao.voidnix/`，可被预测路径劫持（安全 bug）。
27. 6 个死依赖：@wdio/\* ×4 + webdriverio + ts-node。
28. 死代码：`viewStates` Map、`useStreamOutput`（3 行单函数叫 composable）、`useShortcutConfig`（9 行）、`useInputControl`（29 行只服务 BaseInput）、`toSearchResults` 桥接。
29. 类型重名：`bindings.ts::SearchResult` 与 `types/module.ts::SearchResult` 同名；`types/agent.ts::SearchProviderConfig` 与 `stores/settings.ts::SearchProviderConfig` 同名不同形。

**工具链痛点**

30. `sync-extensions.ts` 343 行用正则解析 Rust 语法（COMMAND_REGEX/INIT_REGEX），脆弱丑陋。
31. specta 类型生成割裂：仅覆盖 22/~60 命令，38 个裸 invoke 无类型；agent 系（Channel）手写；生成流程不在主构建内。
32. `package.json` 缺 `check:extensions` script，CI release 必失败。
33. Tier1 双注册无联动校验（宏 + trait），lib.rs 手动 register 8 处易漏。

### 1.3 保留的优秀设计（不动）

- `fuzzy.ts` 双通道打分（子串 + 拼音，三开关 `precision:'start'` + `continuous:true` + `v:true`）设计优秀。
- `panel.rs`（NSWindow → NonactivatingPanel swizzle）+ `skylight.rs`（Space 迁移私有 API）是核心机制，纯净保留。
- `configure_app!` macro_rules（保留 Builder 类型推断）+ `#[path]` 外部引用（物理在 extensions/ 逻辑挂 crate）的代码组织方式合理。
- release profile（strip + lto + codegen-units=1 + panic=abort）极致优化保留。
- UnoCSS Attributify + 主题色 + Shortcuts 规范保留。
- specta 的 feature-gated 设计思路保留（虽然删除 specta 本身）。

---

## 2. 第一性原则

从本质重新推导：

**扩展 = 元数据 + 能力供给 + 生命周期**

- 扩展声明自己供给什么能力（search/view/config/服务...），框架按需消费。未供给即不支持，零默认值。
- 配置、设置 UI、命令、快捷键、服务全部声明式，框架自动调度渲染。
- 每个扩展目录是自治单元，零跨扩展 import 依赖、零框架业务泄漏。
- 内核只管「调度与生命周期」（runtime）与「macOS 抽象」（platform），零业务语义。
- 启动、搜索、IO 全并行，冷启动 <100ms 目标。

**三条用户铁律**

- 避免硬编码：三层配置金字塔（语义常量集中 / 框架可调 / 扩展自管）。
- 减少垃圾：icon 零缓存、IPC 零文件、TempHandle 统一清理、gitignore 完善。
- 增强扩展性：固定能力槽 + contributes 开放扩展点 + 服务机制 + 搜索管道 + 自定义配置字段。

---

## 3. 最终架构设计

### 3.1 Rust 后端目录结构

```
src-tauri/src/
├── main.rs                    # 入口（~6 行，不变）
├── lib.rs                     # 装配清单（精简到 ~30 行，仅框架自管 + ExtensionRegistry::bootstrap）
├── build.rs                   # 扫描 extensions/*/native/*.mm 自动编译（函数化）
├── extensions.rs              # 自动生成（仅 .plugin() 链 + mod 声明，<40 行）
│
├── runtime/                   # 运行时核心（原 core/infra 整合）
│   ├── mod.rs
│   ├── constants.rs           # 语义常量单一源（搜索权重、分组优先级、超时默认值）
│   ├── window.rs              # 主窗口 show/hide/move_to_active_space（删 size/pick_dir/get_home）
│   ├── shortcut.rs            # 快捷键 + 录制（删 SELECTED_TEXT/PREV_FRONT_PID 泄漏）
│   ├── storage.rs             # 统一存储（settings.json + per-ext config.json）+ TempHandle 注册表
│   ├── permission.rs          # 系统权限薄壳
│   ├── registry.rs            # Extension trait + Registry（并行 bootstrap + 拓扑排序）
│   └── llm/                   # LLM 基础设施（拆自 infra/sse.rs）
│       ├── mod.rs
│       ├── client.rs          # 流式请求（原 stream_openai_request）
│       ├── types.rs           # LlmMessage/LlmToolCall（原 sse.rs 类型层）
│       ├── security.rs        # SSRF 防护 + secret_scrub（合并原 sse.rs 安全 + agent/secret_scrub.rs）
│       └── parser.rs          # tool_calls 解析（原 infra/tool_calls_parser.rs）
│
├── platform/                  # macOS 原生桥（原 macos/，纯净）
│   ├── mod.rs
│   ├── panel.rs               # NSPanel 转换（不变）
│   ├── skylight.rs            # Space 迁移（不变）
│   ├── focus.rs               # 焦点管理（统一 PREV_FRONT_PID 唯一源）
│   ├── click_monitor.rs       # 点击监听（删 suppress 泄漏）
│   ├── input.rs               # 键盘注入（统一 post_key/inject_copy/simulate_cmd_v 三套）
│   ├── pasteboard.rs          # NSPasteboard 统一（读/写/快照/恢复三套合一）
│   ├── permission.rs          # 权限检测实现
│   └── path_guard.rs          # 统一路径校验（合并 finder-ext BLOCKED_PREFIXES + agent canonicalize）
│
└── http.rs                    # 全局 HTTP 客户端（原 infra/http.rs，独立小文件）
```

### 3.2 前端目录结构

```
src/
├── main.ts                    # 入口
├── App.vue                    # 精简（仅挂载 + useAppLifecycle）
├── commands.ts                # 自动生成（命令名常量，类型手写于 types/）
│
├── runtime/                   # 前端运行时
│   ├── extension-registry.ts  # 扩展注册中心 + contributes 聚合
│   ├── search-engine.ts       # 搜索引擎（staticItems 预算 + dynamic 并行 + filter/rerank 管道）
│   ├── config-registry.ts     # 配置 schema 注册中心 + 字段类型注册表
│   ├── service-registry.ts    # 服务声明 + 查找
│   ├── storage.ts             # useExtensionConfig + useAppConfig + defineExtensionConfig
│   ├── constants.ts           # 语义常量单一源（与 runtime/constants.rs 对齐）
│   └── types.ts               # Extension/SearchProvider/ConfigSchema/Contribution 类型
│
├── stores/                    # 仅 2 个
│   ├── app.ts                 # UI 状态（activeModule/searchQuery/dialog/subview/shortcut 录制/statusMessage）
│   └── update.ts              # 应用更新
│
├── composables/
│   ├── useAppLifecycle.ts     # 抽自 App.vue（窗口生命周期 + 快捷键 + 失焦防抖）
│   ├── useSearchInput.ts      # 拆自 useSearchCommand（输入处理 + 防抖 + web 搜索解析）
│   ├── useResultNavigation.ts # 拆自 useSearchCommand（键盘导航 + 多选）
│   ├── useFloating.ts         # floating-ui 封装（不变）
│   ├── useScrollPosition.ts   # 按 key 保存/恢复滚动（不变）
│   └── useTauriListener.ts    # onMounted/onUnmounted 自动清理（不变）
│
├── components/
│   ├── ui/                    # 原子组件（BaseList 删 appStore 依赖，改 keyboardActive prop）
│   │   ├── BaseList.vue
│   │   ├── BaseDialog.vue
│   │   ├── BaseSelect.vue
│   │   ├── BaseInput.vue      # 合并 useInputControl
│   │   ├── BaseTextarea.vue
│   │   ├── BaseSlider.vue
│   │   ├── BaseButton.vue
│   │   ├── BaseListItem.vue
│   │   ├── BaseEmptyState.vue
│   │   ├── ShortcutInput.vue
│   │   └── UpdateDialog.vue
│   ├── layout/
│   │   ├── MainView.vue       # 删 GROUP_TITLES（合并到 constants）
│   │   ├── ContentView.vue    # 精简（图标分发抽 ResultIcon）
│   │   ├── StatusBar.vue
│   │   └── ResultIcon.vue     # 新增（抽自 ContentView 图标分发）
│   ├── Markdown.vue           # 统一 markdown（用 marked，删 3 处正则）
│   └── ConfigField.vue        # 声明式配置字段渲染器（内置 + 自定义类型分发）
│
├── types/
│   ├── search.ts              # 统一 SearchResult（删除 bindings 重名）
│   └── agent.ts               # 手写 LLM/Agent 类型（Channel 不进 specta）
│
└── utils/
    ├── fuzzy.ts               # 不变（设计优秀，权重读 constants）
    ├── icons.ts               # 文件扩展名 → 图标 + 颜色映射
    ├── clipboard.ts           # 删 useAppStore 依赖（返回 label，调用方 showStatus）
    ├── format.ts              # 合并 provider.ts + error.ts
    ├── dom.ts
    ├── tauri.ts               # 删 toSearchResults（统一类型）
    └── id.ts                  # generateRequestId（迁自 useStreamOutput）
```

### 3.3 扩展统一形态

所有 16 个扩展同构，**只有「扩展」一种概念**，是否含 `native/` 子目录是实现细节，不构成分类。

```
extensions/<id>/
├── index.ts                   # defineExtension({ meta, search?, mainView?, contributes? })
├── config.ts                  # defineExtensionConfig({ schema })
├── View.vue                   # 主视图（若 mainView 能力）
├── Settings.vue               # 设置片段（若 settingsPanel 能力，可选）
├── *.test.ts                  # 测试（必须）
└── native/                    # Rust 后端（仅需要系统级能力时存在）
    ├── mod.rs                 # pub fn init() -> TauriPlugin（局部注册命令）+ Extension trait
    └── ...
```

---

## 4. 核心接口设计

### 4.1 Rust Extension trait

```rust
// src-tauri/src/runtime/registry.rs

#[async_trait::async_trait]
pub trait Extension: Send + Sync + 'static {
    /// 扩展唯一 id（与目录名一致）
    fn id(&self) -> &'static str;

    /// 依赖的其他扩展 id（用于并行 bootstrap 的拓扑排序）
    fn deps(&self) -> &'static str { &[] }

    /// 启动钩子（并行执行，无依赖关系的扩展同时跑）
    async fn setup(&self, _ctx: &SetupContext) -> Result<()> { Ok(()) }

    /// 清理钩子（退出时反向顺序执行）
    async fn teardown(&self, _ctx: &SetupContext) {}
}

pub struct SetupContext<'a> {
    app: &'a AppHandle,
}

impl<'a> SetupContext<'a> {
    pub fn app(&self) -> &AppHandle;
    pub fn storage(&self) -> StorageHandle;       // 配置存取
    pub fn shortcut(&self) -> ShortcutHandle;     // 快捷键钩子注册
    pub fn temp(&self) -> TempHandle;             // 临时文件注册（自动清理）
}
```

**并行 bootstrap**：

```rust
pub async fn parallel_bootstrap(app: &AppHandle) -> Result<()> {
    let mut extensions: Vec<Box<dyn Extension>> = collect_extensions();
    let layers = topological_sort(&extensions);   // 依据 deps() 分层
    for layer in layers {
        let ctx = SetupContext::new(app);
        let results = futures::future::join_all(
            layer.iter().map(|ext| ext.setup(&ctx))
        ).await;
        for r in results { r?; }                  // 任一失败则中断
    }
    Ok(())
}
```

### 4.2 TS Extension 接口（能力槽供给式）

```ts
// src/runtime/types.ts

interface Extension {
  meta: ExtensionMeta
  setup?(ctx: ExtContext): void | Promise<void>
  teardown?(ctx: ExtContext): void

  // 固定能力槽（框架原生支持，类型安全）
  search?: SearchProvider
  mainView?: () => Component
  subviews?: Record<string, () => Component>
  windowViews?: Record<string, () => Component>
  settingsPanel?: () => Component
  statusBarAccessory?: () => Component
  globalShortcuts?: ShortcutBinding[]
  hints?: ModuleHints

  // 开放扩展点（框架遍历合并所有扩展的 contributes）
  contributes?: ContributionPoints
}

interface ExtensionMeta {
  id: string
  name: string
  icon: string
  order: number
  keywords?: string[]
  hidden?: boolean
}

interface ModuleHints {
  enter?: string         // ↵ 动作描述（如「粘贴」「复制」）
  multiSelect?: string   // 多选提示（如「⇧/⌘ 多选」）
  delete?: string        // 删除提示（如「删除」）
}

interface ExtContext {
  app: AppHandle
  config: ConfigAccess
  service: (id: string) => (...args: any[]) => any   // 服务查找
}
```

### 4.3 SearchProvider（双通道）

```ts
interface SearchProvider {
  /** 静态候选：启动时一次计算，框架自动跑 scoreFields 过滤。适合半静态内容（base64 选项、uuid 类型） */
  staticItems?(): SearchResult[]

  /** 动态召回：每次查询并行调用。适合实时召回（mdfind、SQLite、即时计算） */
  dynamic?(query: string, ctx: SearchContext): SearchResult[] | Promise<SearchResult[]>
}

interface SearchContext {
  signal: AbortSignal    // 查询取消（新查询覆盖旧查询时 abort）
}

interface SearchResult {
  id: string
  title: string
  module: string
  description?: string
  icon?: string
  score?: number
  shortcut?: string
  data?: SearchData      // 类型安全的携带数据
}

interface SearchData {
  path?: string
  kind?: 'application' | 'folder' | 'file' | 'module' | 'clipboard' | string
  [key: string]: unknown
}
```

### 4.4 ContributionPoints（开放扩展点）

```ts
interface ContributionPoints {
  /** 搜索结果项的附加动作（如「在 Finder 显示」「复制路径」） */
  resultActions?: ResultAction[]

  /** 右键菜单项 */
  contextMenuItems?: ContextMenuItem[]

  /** 搜索后处理 hook（去重、过滤） */
  searchFilters?: SearchFilter[]

  /** 搜索相关性重排 hook */
  searchRerankers?: SearchReranker[]

  /** 自定义配置字段类型 + 渲染器 */
  configFieldTypes?: Record<string, Component>

  /** 虚拟命令注册（纯 TS，不走 Rust） */
  commands?: CommandBinding[]

  /** 对外服务声明（其他扩展可消费） */
  services?: ServiceDeclaration[]

  /** 状态栏左侧自定义项 */
  statusBarLeftItems?: Array<() => Component>

  /** 自定义快捷键槽位（用户可在设置中绑定） */
  shortcutSlots?: ShortcutSlot[]
}

interface ResultAction {
  id: string
  label: string
  icon: string
  shortcut?: string
  appliesTo: (result: SearchResult) => boolean
  action: (result: SearchResult) => void | Promise<void>
}

interface SearchFilter {
  id: string
  filter: (query: string, results: SearchResult[]) => SearchResult[]
}

interface SearchReranker {
  id: string
  rerank: (query: string, results: SearchResult[]) => SearchResult[]
}

interface ServiceDeclaration {
  id: string                  // 如 'clipboard.write'、'screenshot.cleanup'
  version: string             // semver，如 '1.0'
  handler: (...args: any[]) => any | Promise<any>
}

interface ShortcutSlot {
  id: string                  // 槽位标识，如 'window-manager.toggle-snap'
  label: string               // 设置面板显示
  default?: string            // 默认快捷键
}
```

### 4.5 服务机制（根治横向依赖）

```ts
// clipboard 扩展声明服务
export default defineExtension({
  meta: { id: 'clipboard', ... },
  contributes: {
    services: [
      { id: 'clipboard.write', version: '1.0', handler: async (text: string) => { ... } },
      { id: 'clipboard.read-latest', version: '1.0', handler: async () => { ... } },
    ],
  },
})

// agent 扩展消费
const write = ctx.service('clipboard.write')
await write('text')
```

**规则**：
- 框架在 bootstrap 后（所有 setup 完成）才开放 `ctx.service()` 调用。
- setup 阶段禁用 service 调用（防循环依赖）。
- 未声明的 service id 抛错。
- 服务版本不匹配抛错（强制声明 version）。

### 4.6 配置声明式 schema

```ts
// 内置字段类型
type BuiltinFieldType = 'number' | 'toggle' | 'string' | 'select' | 'slider' | 'keybind' | 'text'

interface ConfigFieldBase {
  type: BuiltinFieldType | string    // string 为自定义类型（需 contributes.configFieldTypes 注册）
  default: unknown
  label: string
  description?: string
  order?: number
}

interface NumberField extends ConfigFieldBase {
  type: 'number'
  default: number
  min?: number
  max?: number
  step?: number
}

interface SelectField extends ConfigFieldBase {
  type: 'select'
  default: string
  options: Array<{ value: string; label: string }>
}

// ... 其他内置类型

type ConfigField = NumberField | SelectField | ToggleField | StringField | SliderField | KeybindField | TextField | CustomField

type ConfigSchema = Record<string, ConfigField>
```

**使用**：

```ts
// extensions/clipboard/config.ts
import { defineExtensionConfig } from '@/runtime/storage'

export default defineExtensionConfig({
  maxDays: { type: 'number', default: 30, label: '保留天数', min: 1, max: 365, order: 1 },
  enabled: { type: 'toggle', default: true, label: '启用剪贴板监听', order: 0 },
})

// 使用配置值（类型安全）
import { useConfig } from '@/runtime/storage'
const { maxDays, enabled } = useConfig('clipboard')
// → 自动持久化至 ~/Library/Application Support/com.litiantao.voidnix/extensions/clipboard/config.json
// → settings 扩展自动扫描渲染 UI（ConfigField.vue）
```

### 4.7 搜索引擎管道

```ts
// src/runtime/search-engine.ts

class SearchEngine {
  private staticCache = new Map<string, SearchResult[]>()  // extId → staticItems
  private filters: SearchFilter[] = []
  private rerankers: SearchReranker[] = []
  private currentController?: AbortController

  init(extensions: Extension[]) {
    // 收集 contributes
    for (const ext of extensions) {
      if (ext.search?.staticItems) {
        this.staticCache.set(ext.meta.id, ext.search.staticItems())
      }
      this.filters.push(...(ext.contributes?.searchFilters ?? []))
      this.rerankers.push(...(ext.contributes?.searchRerankers ?? []))
    }
  }

  async search(query: string): Promise<SearchResult[]> {
    // 取消上一次查询
    this.currentController?.abort()
    const controller = new AbortController()
    this.currentController = controller

    // 1. 静态候选内存过滤（零成本扩展）
    let results = this.searchStatic(query)

    // 2. 并行动态召回（带超时 + abort）
    const dynamic = await this.searchDynamic(query, controller.signal)
    results = [...results, ...dynamic]

    if (controller.signal.aborted) return []

    // 3. 扩展注册的 filter（去重等）
    for (const filter of this.filters) {
      results = filter.filter(query, results)
    }

    // 4. 框架分组排序（语义常量）
    results = this.groupAndSort(results)

    // 5. 扩展注册的 reranker（相关性调整）
    for (const reranker of this.rerankers) {
      results = reranker.rerank(query, results)
    }

    return results
  }

  private searchStatic(query: string): SearchResult[] {
    // 对 staticCache 跑 scoreFields，仅保留命中
  }

  private async searchDynamic(query: string, signal: AbortSignal): Promise<SearchResult[]> {
    // 并行调用所有扩展的 dynamic，带超时（读 constants.SEARCH_TIMEOUT_MS）
  }

  private groupAndSort(results: SearchResult[]): SearchResult[] {
    // 读 constants.GROUP_ORDER + WEIGHTS，分组限流 + 排序
  }
}
```

### 4.8 统一 platform 工具

```rust
// platform/input.rs（统一三套 CGEvent 注入）
pub enum Modifier { Cmd, Shift, Opt, Ctrl, Fn }

pub fn post_key(key_code: u16, modifiers: &[Modifier], pid: Option<i32>)
pub fn post_combo(combo: &str, pid: Option<i32>)         // "cmd+c"、"cmd+v"、"cmd+shift+."
pub fn post_keystroke(string: &str, pid: Option<i32>)    // 输入字符串

// platform/pasteboard.rs（统一三套 NSPasteboard）
pub fn read_text() -> Option<String>
pub fn write_text(s: &str)
pub fn read_image() -> Option<NSImage>
pub fn write_image(img: &NSImage)
pub fn snapshot() -> PasteboardSnapshot                  // 保存当前状态
pub fn restore(snap: PasteboardSnapshot)                 // 恢复
pub fn change_count() -> u64                             // 轮询检测变化用

// platform/focus.rs（统一 PREV_FRONT_PID 唯一源）
pub fn current_frontmost_pid() -> Option<i32>            // 排除自身
pub fn capture_frontmost() -> i32                         // 记录到唯一源
pub fn restore_frontmost(pid: i32)                        // 还给原 app
pub fn activate_app(pid: i32)
pub fn deactivate_self()

// platform/path_guard.rs（统一路径校验，合并 finder-ext + agent）
const BLOCKED_PREFIXES: &[&str] = &["/System", "/usr/bin", "/bin", "/sbin", "/Library", "/opt/homebrew"];
pub fn validate_access(path: &Path) -> Result<PathBuf>   // canonicalize + 符号链接检测 + 黑名单
```

### 4.9 统一 storage + TempHandle

```rust
// runtime/storage.rs
pub struct StorageHandle { /* app handle 引用 */ }
impl StorageHandle {
    pub fn read_app_config() -> serde_json::Value        // 读 config/framework.json
    pub fn write_app_config(v: &serde_json::Value)
    pub fn read_ext_config(id: &str) -> serde_json::Value // 读 extensions/<id>/config.json
    pub fn write_ext_config(id: &str, v: &serde_json::Value)
}

pub struct TempHandle { /* 注册表引用 */ }
impl TempHandle {
    pub fn register(&self, path: PathBuf)                 // screenshot 注册截图、agent 注册命令输出
    pub fn unregister(&self, path: &Path)
    pub fn cleanup_all(&self)                             // 应用退出 + 定期清理
}
```

### 4.10 命令注册机制（消灭 343 行正则脚本）

每个 native 扩展在 `init()` 内局部注册命令：

```rust
// extensions/clipboard/native/mod.rs
pub fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::new("clipboard")
        .invoke_handler(tauri::generate_handler![
            get_items, paste_item, toggle_favorite, delete_item, clear_items
        ])
        .setup(|app| {
            // 启动 DB + monitor（等价于原 Tier1Extension::on_setup）
            Ok(())
        })
        .build()
}
```

`sync-extensions.ts` 简化为 < 50 行：仅扫描 `extensions/*/native/mod.rs` 的 `pub fn init()` 签名，生成：
- `extensions.rs`：`.plugin(clipboard::init())` 链 + `#[path]` mod 声明
- `commands.ts`：命令名常量（从扩展手动声明的 `pub const COMMANDS: &[&str]` 提取，或扩展文档注释）

bindings 类型完全手写于 `types/`，tsc 严格校验。specta 彻底删除。

---

## 5. 三层配置金字塔

### 5.1 语义常量（runtime/constants.rs / constants.ts）

不可配置，集中定义单一真相源：

```ts
// src/runtime/constants.ts
export const SEARCH = {
  WEIGHTS: { prefix: 1000, contains: 600, decay: 0.85, logBase: 2, logMul: 50, cap: 320 },
  GROUP_BOOST: { module: 500, application: 300, folder: 80, file: 0 },
  GROUP_ORDER: ['module', 'application', 'folder', 'file', 'clipboard'] as const,
  GROUP_TITLES: { module: '操作', application: '应用', folder: '文件夹', file: '文件', clipboard: '剪贴板' },
} as const

export const LIMITS = {
  maxAppResults: 30,
  maxFileResults: 50,
  searchTimeoutMs: 3000,
} as const
```

### 5.2 框架可调参数（config/framework.json + useAppConfig）

```ts
// config/framework.json
{
  "window": { "width": 720, "height": 480, "alwaysOnTop": true },
  "shortcuts": { "main": "cmd+space", "clipboard": "cmd+shift+c", "translate": "cmd+shift+t", "agent": "cmd+shift+a" },
  "aiProviders": { /* 通用 AI 基础设施 */ },
  "llm": { "maxContentLen": 32768, "maxMessages": 100 }
}
```

### 5.3 扩展可调参数（extensions/<id>/config.json + defineExtensionConfig）

每扩展完全自管，settings 面板自动渲染。见 §4.6。

### 5.4 agent 9 层防御全部 config 化

```ts
// extensions/agent/config.ts
export default defineExtensionConfig({
  trustedCommands: { type: 'agent.command-list', default: [
    'ls', 'cat', 'git', 'grep', 'find', 'rg', 'fd', 'echo', 'pwd',
    'wc', 'head', 'tail', 'sort', 'uniq', 'diff', 'tree', 'file',
    'which', 'whereis', 'stat', 'du', 'df', 'ps', 'top', 'kill',
    'mkdir', 'touch', 'cp', 'mv', 'ln', 'chmod', 'chown',
    'git status', 'git log', 'git diff', 'git branch', 'git show',
    'cargo build', 'cargo test', 'cargo check', 'cargo run',
    'npm list', 'bun run', 'pnpm list',
    'hostname', 'whoami', 'date', 'uptime', 'env',
  ], label: '可信命令（无需审批）', order: 0 },
  forbiddenCommands: { type: 'agent.command-list', default: [
    'osascript', 'sudo', 'sh', 'bash', 'zsh', 'curl', 'wget', 'nc', 'ssh', 'scp',
  ], label: '禁止命令（即便 approved 也拦）', order: 1 },
  blockedArgs: { type: 'agent.command-list', default: [
    '--exec', '--upload-pack', '-o', '-C', '--init',
  ], label: '禁止参数', order: 2 },
  maxCpuSeconds: { type: 'number', default: 30, label: 'CPU 上限（秒）', min: 1, max: 300, order: 3 },
  maxMemoryMb: { type: 'number', default: 512, label: '内存上限（MB）', min: 64, max: 4096, order: 4 },
  maxOpenFiles: { type: 'number', default: 64, label: '文件描述符上限', min: 8, max: 1024, order: 5 },
  executionTimeout: { type: 'number', default: 30, label: '执行超时（秒）', min: 1, max: 300, order: 6 },
  maxOutputBytes: { type: 'number', default: 1048576, label: '输出截断（字节）', min: 1024, max: 10485760, order: 7 },
  maxTurns: { type: 'number', default: 10, label: '最大对话轮次', min: 1, max: 50, order: 8 },
  systemPrompt: { type: 'text', default: '', label: '系统提示词（空则用默认）', order: 9 },
})
```

---

## 6. 扩展清单与迁移映射（16 个）

所有扩展同构化，按是否含 native/ 区分实现方式（非分类）。

### 含 native/（10 个，需系统级能力）

| 扩展 | 迁移要点 |
|---|---|
| clipboard | config 自管（maxDays/enabled）；monitor 用 platform/pasteboard；CGEvent 模拟 ⌘V 用 platform/input；命令局部注册 init() |
| screenshot | config 自管（savePath）；temp 文件走 TempHandle；CGEvent 用 platform/input；scroll_capture/session/pin 保留 |
| awake | config 自管；**binary 路径 /tmp → app_data_dir**（安全修复）；TrayIconBuilder 保留 |
| zsh-autosuggestions | **目录名 zsh-as → zsh-autosuggestions**（统一约定）；signals.log 改 ring buffer；include_bytes! binary 保留 |
| window-manager | config 自管（customWidth/customHeight/dragSnapEnabled）；PREV_FRONT_PID 走 platform/focus；AX FFI 自管 |
| finder-ext | **删横向调 screenshot**（走 service 或 TempHandle）；**IPC 改 Darwin notification + 共享内存**（零文件）；路径校验走 platform/path_guard |
| translate | config 自管（configs/targetLang）；自管 SELECTED_TEXT State（不泄漏框架）；text_selection 工具下沉扩展内 |
| agent | **接收 core/agent/ 下沉为 native/engine/**（loop_runner/approval/cancellation/secret_scrub/tool_registry）；9 层防御读 config；prompt/turns 由 config 注入 |
| search | **删 icon cache.rs，改 NSWorkspace.icon 实时提取**（零磁盘文件）；icon_cache_dir/cleanup 下沉后删除；init() 局部注册；search 改 staticItems + dynamic |
| ip | **转纯 TS**（46 行 Rust → TS fetch，删 native/） |

### 纯 TS（6 个，无 native/）

| 扩展 | 迁移要点 |
|---|---|
| calculator | config 自管 history（走 storage）；search 走 dynamic（即时计算）；补 parser 单元测试 |
| settings | **改为纯渲染器**：扫描 config-registry，对每扩展 schema 渲染 ConfigField；零硬编码聚合 |
| base64 | **从 .vnext 转 TS**；search 走 staticItems（编码/解码选项）+ dynamic（结果） |
| time | **从 .vnext 转 TS**；search 走 staticItems（格式选项）+ dynamic（结果） |
| currency | **从 .vnext 转 TS**；HTTP 走统一 http 命令；search 走 dynamic |
| uuid | **从 .vnext 转 TS**；search 走 staticItems（UUID/NanoID 选项）+ dynamic（结果） |

---

## 7. 删除清单（绝对清单）

### 7.1 Rust 删除

- `src-tauri/src/core/` 全目录（8+ 文件，拆分下沉）
- `src-tauri/src/infra/` 全目录（4 文件，http 独立 + sse 拆 llm + path 删 icon_cache + tool_calls_parser 迁 llm）
- `src-tauri/src/macos/text_selection.rs`（拆分下沉 translate + platform/input + platform/pasteboard）
- `core/agent/` 全目录（5 文件，下沉 `extensions/agent/native/engine/`）
- `core/ext_loader.rs` + `ext_manifest.rs`（不再有运行时扩展加载）
- `SELECTED_TEXT` static
- 重复的 `PREV_FRONT_PID` ×3（仅留 platform/focus.rs 唯一源）
- 3 套 CGEvent 注入（统一 platform/input.rs）
- 3 套 NSPasteboard 读取（统一 platform/pasteboard.rs）
- specta feature + specta/specta-typescript/tauri-specta 依赖
- tauri-plugin-store 依赖
- tauri-plugin-clipboard-manager 依赖
- `src-tauri/tests/` 空目录

### 7.2 前端删除

- `src/stores/settings.ts`（586 行）
- `src/core/worker-sandbox.ts`（252 行）
- `src/core/tier2-registry.ts`（184 行）
- `src/core/async-view.ts`（20 行）
- `src/components/declarative/` 全目录（474 行，5 原语）
- `src/composables/useStreamOutput.ts`（3 行单函数）
- `src/composables/useShortcutConfig.ts`（9 行）
- `src/composables/useInputControl.ts`（29 行只服务 BaseInput）
- `src/composables/useSettingsInput.ts`（被 ConfigField 替代）
- `src/utils/events.ts`（useScroll/onKeyStroke 迁 composables）
- `src/utils/provider.ts` + `src/utils/error.ts`（合并 format.ts）
- `viewStates` Map 死代码
- `toSearchResults` 桥接函数
- `MainView.vue:GROUP_TITLES`（合并 constants）
- `module-registry.ts:GROUP_ORDER`（合并 constants）
- ContentView 的 6 个图标分发函数（抽 ResultIcon）
- BaseList 的 useAppStore 依赖（改 prop）
- 6 个死依赖（@wdio/\* ×4 + webdriverio + ts-node）

### 7.3 扩展删除

- 4 个 `.vnext/` 目录（base64/time/currency/uuid，转 TS 后删）
- finder-ext 横向调 screenshot 的 cleanup_temp_files
- awake binary /tmp 路径（改 app_data_dir）
- zsh-as 目录名（改 zsh-autosuggestions）

---

## 8. 文件生产最小化

| 原生产 | 新方案 | 收益 |
|---|---|---|
| `search/icons/` 400 PNG | **完全不缓存**，NSWorkspace.icon 实时提取（系统自带缓存） | 删 ~400 文件 + cleanup 逻辑 + icon_cache_dir 代码 |
| `finder-ext/commands/cmd_*.json` | **Darwin notification + 共享内存** | 零文件累积 |
| `zsh-as/signals.log` 无限增长 | **ring buffer + 启动 truncate** | 有上限 |
| screenshot/agent 临时文件 | **TempHandle 统一注册 + 退出/定期清理** | 无残留 |
| `.gitignore` | 补全 `test-results/`、`proptest-regressions/`、`.DS_Store` | 仓库零垃圾入库 |

保留：`.claude/`、`CLAUDE.md`、`.mcp.json`（开发工具）；`.prettierignore` + `.prettierrc`（标准配置）；clipboard SQLite WAL（性能保证）。

---

## 9. 实施阶段（6 阶段，一次性执行）

> 在 `refactor/v2` 分支进行，分阶段提交保证 git 历史清晰。每阶段结束必须可编译。

### 阶段 1：Rust 内核重建

**目标**：建立新目录结构，移动并重命名，统一 platform 工具，删除旧目录。

**步骤**：

1. 创建 `runtime/` + `platform/` + `http.rs` 目录骨架与 mod.rs
2. 移动 `infra/sse.rs` → `runtime/llm/`（拆 4 子模块：client/types/security/parser）
3. 合并 `core/agent/secret_scrub.rs` + `infra/sse.rs` 安全部分 → `runtime/llm/security.rs`
4. 创建 `runtime/constants.rs`（语义常量集中）
5. 创建 `runtime/storage.rs`（StorageHandle + TempHandle 注册表）
6. 创建 `runtime/registry.rs`（Extension trait + SetupContext + 并行 bootstrap）
7. 创建 `runtime/window.rs`（主窗口 show/hide/move，删 size/pick_dir/get_home）
8. 创建 `runtime/shortcut.rs`（快捷键 + 录制，删 SELECTED_TEXT/PREV_FRONT_PID 泄漏）
9. 创建 `runtime/permission.rs`（薄壳，合并原 core/permission + 部分 macos/permission）
10. 创建 `platform/focus.rs`（统一 PREV_FRONT_PID 唯一源，合并 mac_utils）
11. 创建 `platform/input.rs`（统一 post_key/inject_copy/simulate_cmd_v 三套）
12. 创建 `platform/pasteboard.rs`（统一 read_text/write_text/snapshot/restore/change_count）
13. 创建 `platform/path_guard.rs`（统一路径校验，合并 finder-ext + agent）
14. 移动 `macos/{panel,skylight,click_monitor,permission}.rs` → `platform/`
15. 删除 `core/` + `infra/` + `macos/` 旧目录
16. 重写 `lib.rs`（精简到 ~30 行，仅框架自管 + ExtensionRegistry::bootstrap）
17. 重写 `build.rs`（扫描 `extensions/*/native/*.mm` 自动编译，函数化）
18. 重写 `scripts/sync-extensions.ts`（< 50 行，仅扫 init() + 生成 .plugin() 链）
19. 更新 `Cargo.toml`（删 specta/plugin-store/plugin-clipboard-manager 依赖；deps 改为新增）
20. `cargo check` 通过

**验证**：`cd src-tauri && cargo check` 零错误。

### 阶段 2：Rust 扩展迁移

**目标**：所有 native 扩展改用新平台工具，下沉框架层，局部注册命令。

**步骤**：

1. 创建 `extensions/agent/native/engine/`，移动 `core/agent/{loop_runner,tool_registry,approval,cancellation}.rs`
2. agent `mod.rs` 改 `init()` 局部注册命令；Extension trait 实现；9 层防御读 config（通过 SetupContext.storage）
3. agent `DEFAULT_SYSTEM_PROMPT` 改为 config.systemPrompt 默认值（扩展内定义）
4. agent `MAX_TURNS` 改为 config.maxTurns
5. 所有 native 扩展 mod.rs 改 `pub fn init() -> TauriPlugin` 局部注册命令
6. clipboard：monitor 用 platform/pasteboard；simulate_cmd_v 用 platform/input；config 读 maxDays/enabled
7. screenshot：temp 文件走 TempHandle；CGEvent 用 platform/input
8. translate：text_selection 工具下沉扩展内（AX 选择 + inject_copy + poll_clipboard）；config 自管
9. translate：SELECTED_TEXT 改扩展内 State（不再泄漏框架）
10. window-manager：PREV_FRONT_PID 走 platform/focus；AX FFI 自管
11. finder-ext：删横向调 screenshot（走 TempHandle 或 service）；IPC 改 Darwin notification + 共享内存
12. finder-ext：路径校验走 platform/path_guard
13. awake：binary 路径改 app_data_dir（安全）
14. zsh-as：目录名 zsh-as → zsh-autosuggestions（mod 路径、配置路径统一）
15. zsh-as：signals.log 改 ring buffer + 启动 truncate
16. search：删 cache.rs 图标缓存逻辑；icon.rs 改 NSWorkspace.icon 实时提取
17. search：删 icon_cache_dir/cleanup 相关代码
18. ip：删 native/（转纯 TS 在阶段 4）
19. 所有扩展实现 Extension trait（id/deps/setup/teardown）
20. `cargo check` + `cargo test --lib` 通过

**验证**：`cd src-tauri && cargo test --lib` 全绿。

### 阶段 3：前端运行时重建

**目标**：建立新前端运行时，删除旧结构。

**步骤**：

1. 创建 `src/runtime/types.ts`（Extension/SearchProvider/Contribution/ConfigSchema 类型）
2. 创建 `src/runtime/constants.ts`（语义常量，与 runtime/constants.rs 对齐）
3. 创建 `src/runtime/storage.ts`（useExtensionConfig + useAppConfig + defineExtensionConfig）
4. 创建 `src/runtime/config-registry.ts`（schema 注册 + 字段类型注册表）
5. 创建 `src/runtime/extension-registry.ts`（扩展注册 + contributes 聚合）
6. 创建 `src/runtime/search-engine.ts`（staticItems + dynamic + filter + rerank 管道）
7. 创建 `src/runtime/service-registry.ts`（服务声明 + 查找）
8. 创建 `src/components/ConfigField.vue`（声明式渲染器，内置 + 自定义类型分发）
9. 创建 `src/components/Markdown.vue`（用 marked，统一渲染）
10. 创建 `src/components/layout/ResultIcon.vue`（抽自 ContentView 图标分发）
11. 创建 `src/composables/useAppLifecycle.ts`（抽自 App.vue）
12. 拆 `useSearchCommand.ts` → `useSearchInput.ts` + `useResultNavigation.ts`
13. 移动 `utils/events.ts` 内容 → `composables/`（useScroll/onKeyStroke）
14. 合并 `utils/provider.ts` + `utils/error.ts` → `utils/format.ts`
15. 创建 `utils/id.ts`（generateRequestId 迁自 useStreamOutput）
16. 删除 `stores/settings.ts`（586 行）
17. 删除 `core/worker-sandbox.ts` + `tier2-registry.ts` + `async-view.ts`
18. 删除 `components/declarative/` 全目录
19. 删除 `composables/{useStreamOutput,useShortcutConfig,useInputControl,useSettingsInput}.ts`
20. 删除 `utils/events.ts` + `provider.ts` + `error.ts`
21. 改 `App.vue`（精简，用 useAppLifecycle）
22. 改 `BaseList.vue`（删 useAppStore 依赖，改 keyboardActive prop）
23. 改 `ContentView.vue`（删图标分发，用 ResultIcon）
24. 改 `MainView.vue`（删 GROUP_TITLES，读 constants）
25. 改 `utils/clipboard.ts`（删 useAppStore 依赖，返回 label）
26. 改 `utils/fuzzy.ts`（权重读 constants）
27. 改 `utils/tauri.ts`（删 toSearchResults）
28. `bun run typecheck` 通过

**验证**：`bun run typecheck` + `bun run test` 全绿。

### 阶段 4：扩展迁移（16 个同构化）

**目标**：所有扩展改用新接口。

**步骤（按依赖顺序）**：

1. clipboard：改 `defineExtension({ meta, search: { dynamic }, mainView, contributes: { services } })`；config.ts 声明 schema
2. screenshot：改 defineExtension；config.ts；temp 文件注册
3. awake：改 defineExtension；config.ts；binary 路径修复
4. zsh-autosuggestions：改目录名；改 defineExtension；config.ts；signals.log ring buffer
5. window-manager：改 defineExtension；config.ts；PREV_FRONT_PID 走 platform
6. finder-ext：改 defineExtension；config.ts；IPC 改 Darwin notification；删横向依赖
7. translate：改 defineExtension；config.ts；SELECTED_TEXT 自管；text_selection 下沉
8. agent：改 defineExtension；config.ts（9 层防御全 config 化）；接收 engine/ 下沉
9. search：改 defineExtension；search 改 staticItems + dynamic；删 icon cache
10. ip：转纯 TS（删 native/）；search 走 dynamic；config.ts
11. calculator：改 defineExtension；config.ts（history 走 storage）；search 走 dynamic；补 parser 测试
12. settings：改为纯渲染器（扫描 config-registry）；自定义快捷键槽位读 contributes.shortcutSlots
13. base64：从 .vnext 转 TS；search 走 staticItems + dynamic；config.ts
14. time：从 .vnext 转 TS；search 走 staticItems + dynamic；config.ts
15. currency：从 .vnext 转 TS；HTTP 走统一命令；search 走 dynamic；config.ts
16. uuid：从 .vnext 转 TS；search 走 staticItems + dynamic；config.ts
17. 删除 4 个 `.vnext/` 目录
18. `bun run typecheck` + `bun run test` 通过

**验证**：`bun run typecheck` + `bun run test` + `cargo test --lib` 全绿。

### 阶段 5：测试 + 工具链 + 文档

**目标**：补齐测试，清理工具链，更新文档。

**步骤**：

1. 补扩展单元测试：
   - calculator：tokenizer + parser（递归下降）
   - clipboard：去重逻辑 + 淘汰策略 + emoji 跳过
   - agent：命令白名单匹配 + 参数黑名单 + 元字符检测 + 断路器
   - search：mdfind 输出解析 + Spotlight 元数据
   - translate：语言检测 + smart_target_lang
   - base64/time/uuid/currency：编码/转换逻辑
2. 补前端运行时测试：
   - search-engine：staticItems 过滤 + dynamic 并行 + filter/rerank 管道
   - config-registry：schema 注册 + 字段类型分发
   - service-registry：声明 + 查找 + 未声明抛错
   - storage：useExtensionConfig 持久化
3. 删除 6 个死依赖（@wdio/\* ×4 + webdriverio + ts-node）
4. 删除 specta feature + 相关依赖
5. 删除 tauri-plugin-store + tauri-plugin-clipboard-manager 依赖
6. 补 `package.json` 的 `check:extensions` script
7. `.gitignore` 补全 `test-results/`、`proptest-regressions/`、`.DS_Store`
8. 删除 `src-tauri/tests/` 空目录
9. 重写 `AGENTS.md`（新架构 + 三层配置 + contributes + 服务机制 + 扩展开发指南）
10. 删除 `docs/tier1-extensions.md` + `tier2-extensions.md`，统一 `docs/extensions.md`
11. 更新 `docs/extensions/*.md`（每个扩展的深度文档）
12. `bun run lint` 通过

**验证**：全部测试 + lint 通过。

### 阶段 6：验证

**目标**：全功能人工验证 + 性能验证。

**步骤**：

1. `bun run test`（前端单元）
2. `cd src-tauri && cargo test --lib`（Rust 单元）
3. `bun run test:e2e`（E2E）
4. `bun run lint`（Prettier + ESLint）
5. `bun run tauri:dev` 启动，逐扩展手测：
   - search：应用/文件搜索、中文/拼音/英文、回车启动
   - clipboard：复制监听、历史列表、粘贴、收藏、删除
   - screenshot：区域截图、标注、OCR、长截图、钉图
   - translate：划词翻译、AI 流式、有道
   - agent：对话、tool calling、命令执行审批、网络搜索
   - awake：合盖不休眠、托盘切换
   - zsh-autosuggestions：补全触发、Ctrl+C 拦截
   - window-manager：分屏、拖拽 snap
   - finder-ext：右键菜单、文件 IPC
   - calculator：表达式计算、历史
   - base64/time/uuid/currency：编码/转换/生成
   - ip：公网 IP 查询
   - settings：快捷键配置、扩展开关、权限检测
6. 性能验证：
   - 冷启动 <100ms（Activity Monitor 测）
   - 常驻内存 <50MB
   - icon 零磁盘文件（`ls ~/Library/.../extensions/search/` 无 icons 目录）
   - finder-ext commands 目录无累积（`ls ~/Library/.../extensions/finder-ext/commands/` 为空或不存在）
7. 终极校验清单（见 §10）

**验证**：全部通过后合并到 main。

---

## 10. 验收标准

重构完成的终极校验：

- [ ] `src-tauri/src/{core,infra,macos}/` 目录不存在
- [ ] `src/{stores/settings.ts,core/worker-sandbox.ts,core/tier2-registry.ts,components/declarative/}` 不存在
- [ ] 全仓零 `SELECTED_TEXT`、零重复 `PREV_FRONT_PID`、零重复 CGEvent/NSPasteboard 实现
- [ ] lib.rs < 50 行、setup 闭包零扩展专属配置
- [ ] sync-extensions.ts < 50 行、零正则解析命令签名
- [ ] 每扩展 `*.test.ts` 存在、`cargo test` 通过
- [ ] bindings/commands 手写覆盖全部命令、tsc 零错误
- [ ] 冷启动 <100ms、常驻内存 <50MB
- [ ] icon 零磁盘文件、finder-ext commands 零累积
- [ ] agent 9 层防御全 config 化、用户可在设置面板调整
- [ ] AGENTS.md + docs/ 全面重写、零 tier1/tier2 字样
- [ ] 零硬编码可调参数（语义常量除外，集中 constants）
- [ ] 每扩展有 config.ts（声明式 schema）
- [ ] 每扩展有 index.ts（defineExtension 能力槽）
- [ ] contributes 扩展点 + 服务机制 + 搜索管道全部实现

---

## 11. 风险与缓解

- **一次性重构期间不可用**：在 `refactor/v2` 分支进行，main 保持可用；分阶段提交，每阶段可编译验证。
- **扩展接口全变，迁移机械工作量大**：16 扩展模板化迁移，每扩展 ~30 分钟。
- **agent 框架下沉后第二消费者出现需再提取**：YAGNI，出现再提取；engine/ 模块边界保持清晰。
- **命令类型全手写失去自动同步**：commands.ts 常量兜底 + tsc 严格 + check:extensions CI。
- **并行 setup 引入竞态**：deps 声明 + 拓扑排序；无依赖才并行；setup 阶段禁用 service 调用。
- **自研 storage 并发**：JSON + RwLock，扩展级独立文件无跨扩展竞争。
- **Darwin notification IPC 复杂度**：finder-ext 核心功能，值得投入；fallback 是共享内存 + 文件锁（仍有文件，加上限清理）。
- **icon 实时提取性能**：NSWorkspace.icon 系统级缓存，实测 <1ms；可选启动后并行预热常用应用。
- **contributes 聚合时机**：setup 阶段收集所有扩展的 contributes，构建不可变注册表，运行时零开销。
- **配置全 config 化的安全风险**：agent 危险参数（如 maxTurns=1000）设合理 max 上限 + UI 警告。

---

## 12. 关键代码模板

### 12.1 native 扩展 mod.rs 模板

```rust
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin::TauriPlugin;

mod commands;
mod db;

pub fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::new("clipboard")
        .invoke_handler(tauri::generate_handler![
            commands::get_items,
            commands::paste_item,
            commands::toggle_favorite,
            commands::delete_item,
            commands::clear_items,
        ])
        .setup(|app| {
            // 启动 DB + monitor
            let db = db::open(app.handle())?;
            app.manage(db);
            monitor::start(app.handle());
            Ok(())
        })
        .build()
}

pub struct ClipboardExtension;
#[async_trait::async_trait]
impl crate::runtime::registry::Extension for ClipboardExtension {
    fn id(&self) -> &'static str { "clipboard" }
    async fn setup(&self, ctx: &crate::runtime::registry::SetupContext<'_>) -> tauri::Result<()> {
        // 额外初始化（如注册快捷键钩子）
        Ok(())
    }
}
```

### 12.2 纯 TS 扩展 index.ts 模板

```ts
import { defineExtension } from '@/runtime/extension-registry'
import type { SearchResult } from '@/runtime/types'
import { useConfig } from '@/runtime/storage'
import { base64Encode, base64Decode } from './utils'
import View from './View.vue'

export default defineExtension({
  meta: {
    id: 'base64',
    name: 'Base64',
    icon: 'i-ri-code-s-slash-line',
    order: 100,
    keywords: ['编码', '解码', 'encode', 'decode'],
  },

  search: {
    staticItems: () => [
      { id: 'base64-encode', title: 'Base64 编码', module: 'base64', data: { action: 'encode' } },
      { id: 'base64-decode', title: 'Base64 解码', module: 'base64', data: { action: 'decode' } },
    ],
    dynamic: (query: string) => {
      if (!query) return []
      const encoded = base64Encode(query)
      return [{
        id: 'base64-result',
        title: encoded,
        module: 'base64',
        description: 'Base64 编码结果',
        data: { action: 'encode', result: encoded },
      }]
    },
  },

  mainView: () => View,

  contributes: {
    resultActions: [
      {
        id: 'copy',
        label: '复制',
        icon: 'i-ri-clipboard-line',
        shortcut: '↵',
        appliesTo: (r) => r.module === 'base64' && r.data?.result,
        action: (r) => navigator.clipboard.writeText(r.data!.result),
      },
    ],
  },
})
```

### 12.3 config.ts 模板

```ts
import { defineExtensionConfig } from '@/runtime/storage'

export default defineExtensionConfig({
  enabled: { type: 'toggle', default: true, label: '启用', order: 0 },
  maxHistory: { type: 'number', default: 100, label: '最大历史条数', min: 10, max: 1000, order: 1 },
  defaultAction: {
    type: 'select',
    default: 'encode',
    label: '默认动作',
    options: [
      { value: 'encode', label: '编码' },
      { value: 'decode', label: '解码' },
    ],
    order: 2,
  },
})
```

### 12.4 sync-extensions.ts 简化后骨架

```ts
// scripts/sync-extensions.ts（< 50 行）
import { readdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs'

const EXT_DIR = new URL('../extensions/', import.meta.url)

function scanInitFunctions() {
  const extensions = []
  for (const dir of readdirSync(EXT_DIR)) {
    const modPath = new URL(`${dir}/native/mod.rs`, EXT_DIR)
    if (!existsSync(modPath)) continue  // 纯 TS 扩展无 native
    const src = readFileSync(modPath, 'utf8')
    if (/pub fn init\(\)/.test(src)) {
      extensions.push(dir)
    }
  }
  return extensions
}

function buildExtensionsRs(exts: string[]) {
  const pluginChain = exts.map((e) => `.plugin(crate::extensions::${e.replace(/-/g, '_')}::init())`).join('\n')
  const modDecls = exts.map((e) => `#[path = "../../extensions/${e}/native/mod.rs"]\npub mod ${e.replace(/-/g, '_')};`).join('\n')
  return `// AUTO-GENERATED by sync-extensions.ts. DO NOT EDIT.
macro_rules! configure_app {
    ($builder:expr) => {
        $builder
            ${pluginChain}
    };
}
pub(crate) use configure_app;

${modDecls}
`
}

const exts = scanInitFunctions()
writeFileSync(new URL('../src-tauri/src/extensions.rs', import.meta.url), buildExtensionsRs(exts))
console.log(`Synced ${exts.length} extensions`)
```

---

## 13. 常用命令速查

```bash
# 开发
bun install                  # 安装依赖
bun run tauri:dev            # 开发模式（sync → lint → 启动）
./deploy.sh                  # 打包部署

# 测试
bun run test                 # 前端单元（Vitest + happy-dom）
bun run test:watch           # 前端监听
bun run test:e2e             # E2E（Playwright）
cd src-tauri && cargo test --lib   # Rust 单元

# 同步与生成
bun run sync:extensions      # 同步扩展注册（生成 extensions.rs）
bun run check:extensions     # CI 校验（新增，验证 extensions.rs 已同步）

# 质量门禁
bun run lint                 # Prettier + ESLint（含 UnoCSS class 排序）
bun run typecheck            # vue-tsc 严格类型检查

# 类型生成（删除 specta 后不再需要）
# 原：cd src-tauri && cargo test --features specta export_bindings -- --nocapture
```

---

## 14. 完成后维护

- 新增 native 扩展：`extensions/<id>/native/mod.rs` 加 `pub fn init()` + Extension trait 实现；运行 `bun run sync:extensions` 自动注册
- 新增纯 TS 扩展：`extensions/<id>/index.ts` 加 `defineExtension({...})`；自动被 `import.meta.glob` 扫描
- 新增配置项：扩展 `config.ts` 加字段；settings 面板自动渲染
- 新增能力供给：扩展 `contributes` 声明；框架自动聚合
- 新增对外服务：扩展 `contributes.services` 声明；其他扩展 `ctx.service(id)` 消费

---

**本蓝本完整、自包含、可执行。新会话凭此文档 + AGENTS.md 即可推进全部 6 阶段。**
