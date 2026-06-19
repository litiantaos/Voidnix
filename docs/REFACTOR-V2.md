# Voidnix v2 架构蓝本（目标设计）

> 本文是 Voidnix 应用底层架构的**目标设计锚点**——描述彻底重设计后的理想终态，稳定不随进度变动。实现进度见 [STATUS.md](./STATUS.md)。

---

## 0. 目标与原则

### 0.1 目标

彻底完全重新设计应用底层架构，追求极致的结构清晰、优雅、轻量、高性能、低占用。

### 0.2 原则

极简主义、强迫症、精神洁癖；第一性推导、一步到位、不考虑兼容性与历史包袱。

**两个正交层，不用「内核」一词混称：**

- `runtime/`（Rust）：调度与生命周期核心，**平台无关**。只负责"扩展如何注册（registry）、临时文件如何清理（storage/TempHandle）、主窗口/快捷键/权限原语如何暴露"，不含任何 macOS 调用，不负责搜索调度与配置存取（二者是前端 runtime/ 职责）。
- `platform/`：macOS 适配层，**无业务语义**。只暴露 NSPanel/CGEvent/NSPasteboard 等原语，不知道"扩展""搜索"为何物。

二者正交：runtime 可在任意 OS 上跑（换 platform 实现即可），platform 可被任意 runtime 消费。「内核」这个词模糊了边界，本文档避免使用。

**机制最少化：** 每新增一个机制（接口字段、扩展点、生命周期钩子）都要先回答"现有机制能否覆盖"。优先扩展已有机制的参数，而非新增并列机制。

**扩展 = 元数据 + 能力供给 + 生命周期**

- 扩展声明自己供给什么能力（search/view/config...），框架按需消费。未供给即不支持，零默认值。
- 每个扩展目录是自治单元，零跨扩展 import 依赖、零框架业务泄漏。

**性能目标（待 baseline 量化后确认）：** 启动、搜索、IO 全并行。**范围明确二分**：

- **Rust bootstrap <100ms**：pre-bootstrap（串行）+ `join_all`（并行）的纯 Rust 启动段，lib.rs 启动埋点测量。这是并行化的直接收益面。
- **webview first paint**：另立目标（Tauri webview 初始化本身常 >100ms，不计入 bootstrap 指标，避免验收扯皮）。
- **常驻内存 <50MB**：稳态（启动完成后空闲态）。

落地前 lib.rs 启动埋点测串行 baseline → bootstrap 并行化后量化收益；若 baseline 已超目标，分析瓶颈（DB 打开 / AX 初始化 / icon 预热）。

### 0.3 三条用户铁律

- 避免硬编码：三层配置金字塔（语义常量集中 / 框架可调 / 扩展自管，§3）。
- 减少垃圾：icon 零缓存、IPC 零文件、TempHandle 统一清理、gitignore 完善。
- 增强扩展性：能力槽（扩展私有），**每项须有明确消费者才纳入**（YAGNI）。零消费者的扩展点一律不预设——出现真实需求再加。

### 0.4 修订记录

本节记录对目标设计本身的勘误与补强（非进度变动）：

- **v1.1**：§3.4 agent 安全 floor 扩至现网 `FORBIDDEN_PROGRAMS`（31 项）/ `DENIED_ARG_PREFIXES`（15 项）全集，禁止缩窄；默认 `trustedCommands` 移除 `kill/ps/top`（与 L5 冲突）及全部复合条目（匹配语义为程序名，复合条目无效）；补 trusted 匹配语义、`rm -rf` 断路器、`agent_run` 参数清单、不变量「floor ⊇ 现网硬编码」。
- **v1.1**：§1.1 / §2.8 window.rs 澄清——`set_main_window_size` / `pick_directory` / `get_home_dir` 保留为 window.rs 框架命令（3 个），不删除；STATUS 阶段 1 步骤 7「删 size/pick_dir/get_home」作废。
- **v1.1**：§0.2 性能目标范围二分（Rust bootstrap vs webview first paint）。
- **v1.1**：§2.1 并行 bootstrap 执行器规范（`tauri::async_runtime::block_on` + setup async 约束）。
- **v1.1**：§2.2 windowViews 生命周期说明（窗口实体仍由 tauri.conf.json 静态声明）。
- **v1.1**：§2.5 / §3.1 kind 分类变更说明（`web-search/open-url → web`；`file/folder` 同组 → 分组）。
- **v1.2**：plugin-store 重新定性——**保留**（defineConfig + settings.json 持久化后端，非冗余）。仅 `tauri-plugin-clipboard-manager` 在前端 utils 迁移到 `platform/pasteboard::write_text` 后删除。§2.7「Rust 端零消费者」精确化为「业务层 Rust 零消费者；持久化由 plugin-store 基础设施承担」；删除「原子落盘 tmp+rename」不实表述（plugin-store 自管写入语义）。
- **v1.3**：§3.4 trusted 默认值对齐现网 `agent/config.ts` 精选集（43 项，含 sed/awk/jq/yq/bat/ag 等），替换原窄示例；§2.7 删「与 TempHandle 同列」不精确类比。
- **v1.4**：§2.2 能力槽消费者计数经全量核查校正——subviews 的 clipboard key 由 `{screenshot}` 更正为 `{settings}`（实际代码 `extensions/clipboard/index.ts` subviews 仅含 settings）；hints enterHint 补全清单（clipboard/ip/calculator）；Extension 接口补 `placeholder?` 字段（7 消费者，迁移到 defineExtension 时必须承载，原遗漏）；§4 search 迁移要点补「当前注册 2 hidden 模块需合并为单 dynamic」澄清。其余 7 项槽位计数（mainView 9 / searchBarAccessory 3 / windowViews 2 / globalShortcuts 4 / multiSelectHint·deleteHint 各 1）核查准确。
- **v1.5（A1）**：§2.3/§2.5 引入 `SearchResult.boost?` 字段——扩展可填组内优先级提示（默认 0），框架 `finalScore = fuzzy(title,query) + boost`；`score` 仍框架独占、扩展**禁止填**。解决原「扩展不应填 score」与 search-apps 频率/最近使用加权（frequencyBoost/recencyScore）无处安放的功能性回归。
- **v1.5（A2/A3，supersede v1.1）**：撤回 v1.1「file/folder 拆为两个独立分组」决策——A1 的 boost 使拆组失去必要。folder/file 恢复同属 'file' 分组（两个 kind 值仅图标/展示区分），folder 组内优先由扩展填 boost 表达。连带消除「组内限流 vs folder+file 共享」矛盾与「folder 恒先 file 精确匹配」回归。`GROUP_ORDER` 移除 'folder' 单列。
- **v1.5（A4）**：§2.2 windowViews 边界补 `check:extensions` 断言——声明 windowViews 槽的扩展，其每个 key 必须在 `tauri.conf.json` `windows[].label` 中存在，防窗口声明漂移。
- **v1.5（B1）**：§2.4/§2.7 defineConfig 持久化补约束——缓存 store 实例（模块级 `Map<extId, Store>`），watch 回调复用，禁止每次保存重新 `load()`。
- **v1.5（B2）**：§3.4 Settings.vue 补 `trusted ∩ forbiddenCommands.floor` 交集实时警告（UI 标红「被底线覆盖」；Rust 端并集兜底）。
- **v1.5（B3）**：§2.3/阶段 6 相关性回归增自动化 E2E 断言（组间序 + kind 归属），非仅手测。
- **v1.5（B4）**：§2.3 SearchProvider abort cleanup 收窄——仅「持有非自动释放资源（事件订阅/子进程/手动连接池）」的 provider 须 cleanup + 测试；纯 fetch+signal 透传型（currency/ip）随 abort 自动释放，免额外 cleanup 测试。
- **v1.5（C2）**：§1.2 composables 目标清单补列 events.ts/useInputControl.ts/useShortcutConfig.ts/useSettingsInput.ts（4 个现存全部活跃被消费），标注迁移时逐项评估去留。
- **v1.6（N1）**：§2.2/§2.3 补「执行分派」框架契约——`data.kind==='module' && moduleId` 走框架内置激活（setActiveModule），不走 onExecute；模块入口结果 data 形状 `{kind:'module', moduleId}`。原蓝本完全未提，迁移核心 spec。
- **v1.6（N2）**：§2.5 SearchEngine 补 keywordSearchAll 合流步骤（全局模式运行、模块模式禁用），原 §2.5 漏画。
- **v1.6（N3）**：§2.2 拆 `settingsView?` 专用槽（3 消费者，跨扩展契约），`subviews` 收窄为扩展私有（仅 screenshot{ocr}）。消除 'settings' magic key 与「槽扩展私有」措辞矛盾。
- **v1.6（N4）**：§2.3/§2.5 `SearchResult.module` 改框架自动注入（dynamic = 产出扩展 id；keyword = 目标模块 id），扩展禁填。消除现网 module-helpers 的 `module:'system'` vs `module:mod.id` 不一致。
- **v1.6（N5）**：§2.1 teardown「反向顺序」改「并行执行」——setup 零跨扩展依赖故反向无意义。
- **v1.6（N6）**：§3.1 keywordSearchAll 的 +500 模块加权进 `constants.SEARCH.KEYWORD_MODULE_BOOST`（原 module-helpers.ts:45 魔数）。
- **v1.6（N7/N8/N9/N10）**：§7/§2.4 + STATUS 阶段 2 补执行前置——block_on 探针（防嵌套 panic）、串行 baseline 埋点、命令注册原子化（防重复注册 panic）、defineConfig 异步加载竞态文档化。

---

## 1. 最终架构设计

### 1.1 Rust 后端目录结构

```
src-tauri/src/
├── main.rs                    # 入口（~6 行，不变）
├── lib.rs                     # 装配清单（<50 行：框架 generate_handler! 13 命令 + pre-bootstrap 共享初始化 + ExtensionRegistry::bootstrap）
├── build.rs                   # 保持显式编译（每个 .mm 编译参数不同，不扫描化：YAGNI）
├── extensions.rs              # 自动生成（仅 .plugin() 链 + mod 声明，<40 行，零 generate_handler!）
│
├── runtime/                   # 运行时核心（平台无关）
│   ├── mod.rs
│   ├── window.rs              # 主窗口 show/hide/move + panel 转换/圆角配置（main 窗口原语集中）+ 框架级路径/对话框命令（set_main_window_size/pick_directory/get_home_dir，§2.8 的 window::* 3 框架命令）
│   ├── shortcut.rs            # 快捷键 + 录制（零业务语义泄漏）
│   ├── storage.rs             # TempHandle RAII 注册表（扩展临时文件统一清理，§2.7）
│   ├── permission.rs          # 系统权限薄壳
│   ├── registry.rs            # Extension trait + Registry（并行 bootstrap：join_all，§2.1）
│   └── llm/                   # LLM 基础设施（仅多消费者共享原语；agent 专属逻辑下沉 agent engine）
│       ├── mod.rs
│       ├── client.rs          # 流式请求 + SSRF 防护（validate_endpoint/validate_ai_request）+ 请求管道常量（buffer/content 上限），agent+translate 共享
│       ├── parser.rs          # SSE tool_call 增量解析（ToolCallAccumulator），client.rs 消费
│       └── types.rs           # LlmMessage/LlmStreamEvent/FinalizedToolCall 通用类型
│
├── platform/                  # macOS 适配层（无状态原语）
│   ├── mod.rs
│   ├── panel.rs               # NSPanel 转换（不变）
│   ├── skylight.rs            # Space 迁移（不变）
│   ├── focus.rs               # 焦点管理（PREV_FRONT_PID 唯一源）
│   ├── click_monitor.rs       # 点击监听
│   ├── input.rs               # 键盘注入（统一 post_key/post_combo/post_keystroke）
│   ├── pasteboard.rs          # NSPasteboard 无状态原语全集
│   ├── permission.rs          # 权限检测实现
│   ├── path_guard.rs          # 路径校验原语 validate(path, policy)，policy 区分 finder-ext/agent 信任级
│   └── selection.rs           # AX 选中文本提取 + ax_timeout 初始化原语（无状态，translate 划词消费）
│
└── http.rs                    # 全局 HTTP 客户端（独立小文件）
```

**关键边界**：

- `runtime/` 与 `platform/` 严格正交。runtime 零 macOS 调用，platform 零业务语义。
- **无 `runtime/constants.rs`**：搜索逻辑全在前端（fuzzy.ts + module-registry.ts），Rust 端零消费者；LLM 请求管道常量随 security.rs 溶解并入 client.rs。Rust 端不设常量集中文件（§0.2 机制最少化）。
- `runtime/llm/` 按**消费者计数**判定归属（§0.2 机制最少化 + 自述「仅多消费者共享原语」）。`security.rs` 全量溶解映射：
  - `validate_endpoint` + `validate_ai_request` + SSRF 黑名单 + `MAX_SSE_BUFFER` + `MAX_MESSAGE_CONTENT_LEN` → 并入 `client.rs`（请求管道校验 + 管道常量，agent+translate 2 消费者）
  - `truncate_message` → 并入 `client.rs`（请求管道消息截断，多消费者）
  - `trim_conversation` + `MAX_CONVERSATION_MESSAGES` → 下沉 `extensions/agent/native/engine/`（agent 唯一消费者，仅 chat_stream 路径调）
  - `secret_scrub` 已在 agent engine
  - 溶解后 runtime/llm/ = client.rs + parser.rs + types.rs（parser 经核实为 client.rs 消费，留框架层）
- `platform/pasteboard.rs` 含无状态原语全集（read_text/write_text/read_image/write_image/string_for_type/data_for_type/has_type/change_count）+ snapshot/restore（snapshot 是 read_text + string_for_type + data_for_type + change_count 的**不可变组合**，无状态无副作用，符合 platform 原语原则；零事务语义需求上移 runtime）。

### 1.2 前端目录结构

```
src/
├── main.ts                    # 入口
├── App.vue                    # 精简（仅挂载 + useAppLifecycle）
├── commands.ts                # 命令名常量（替换 bindings.ts 的 specta 残留，类型手写于 types/）
│
├── runtime/                   # 前端运行时（5 文件）
│   ├── extension-registry.ts  # 扩展注册中心（defineExtension + getAllExtensions）
│   ├── search-engine.ts       # 搜索引擎（dynamic 单通道并行 + filter/group 管道，§2.5）
│   ├── storage.ts             # defineConfig（reactive + watch 自动持久化）
│   ├── constants.ts           # 语义常量单一源（仅前端；Rust 端无常量文件，见 §3.1）
│   └── types.ts               # Extension/SearchProvider 类型
│
├── stores/                    # 3 个（settings.ts 保留）
│   ├── app.ts                 # UI 状态（activeModule/searchQuery/dialog/subview/shortcut 录制/statusMessage）
│   ├── settings.ts            # 框架级配置（快捷键 + AI Provider；181 行）
│   └── update.ts              # 应用更新
│
├── composables/
│   ├── useAppLifecycle.ts     # 抽自 App.vue（窗口生命周期 + 快捷键 + 失焦防抖）
│   ├── useSearchInput.ts      # 拆自 useSearchCommand（输入处理 + 防抖 + web 搜索解析）
│   ├── useResultNavigation.ts # 拆自 useSearchCommand（键盘导航 + 多选）
│   ├── useFloating.ts         # floating-ui 封装（不变）
│   ├── useScrollPosition.ts   # 按 key 保存/恢复滚动（不变）
│   ├── useTauriListener.ts    # onMounted/onUnmounted 自动清理（不变）
│   ├── events.ts              # onKeyStroke/useScroll（v1.5 C2：现存活跃，迁移时评估内联/保留）
│   ├── useInputControl.ts     # BaseInput/BaseTextarea 用（v1.5 C2：现存活跃，评估去留）
│   ├── useShortcutConfig.ts   # shortcut 设置渲染（v1.5 C2：现存活跃，评估去留）
│   └── useSettingsInput.ts    # 设置输入渲染（v1.5 C2：现存活跃，评估去留）
│
├── components/
│   ├── ui/                    # 原子组件（BaseList 删 appStore 依赖，改 keyboardActive prop）
│   └── layout/
│       ├── MainView.vue       # 删 GROUP_TITLES（合并到 constants）
│       ├── ContentView.vue    # 精简（图标分发抽 ResultIcon）
│       ├── StatusBar.vue
│       └── ResultIcon.vue     # 抽自 ContentView 图标分发
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
    └── id.ts                  # generateRequestId
```

**保留说明**：

- `stores/settings.ts`（181 行，已是框架级快捷键+AI Provider，原 framework.json + useAppConfig 目标废弃：当前规模无需引入新机制）
- 各扩展 `Settings.vue` 自管渲染（原 `ConfigField.vue` 声明式渲染器目标废弃）
- `Markdown.vue` 不抽共享组件（agent 单消费者内联 marked + DOMPurify，translate 不用 markdown）

### 1.3 扩展统一形态

所有 16 个扩展同构，**只有「扩展」一种概念**，是否含 `native/` 子目录是实现细节，不构成分类。

```
extensions/<id>/
├── index.ts                   # defineExtension({ meta, search?, onExecute?, mainView?, ..., hints? })
├── config.ts                  # defineConfig('<id>', { ...defaults })（可选）
├── View.vue                   # 主视图（若 mainView 能力）
├── Settings.vue               # 设置片段（可选，自管渲染）
├── *.test.ts                  # 测试（必须）
└── native/                    # Rust 后端（仅需要系统级能力时存在）
    ├── mod.rs                 # pub fn init() -> TauriPlugin（局部注册命令）+ Extension trait
    └── ...
```

---

## 2. 核心接口设计

### 2.1 Rust Extension trait

```rust
// src-tauri/src/runtime/registry.rs

#[async_trait::async_trait]
pub trait Extension: Send + Sync + 'static {
    /// 扩展唯一 id（与目录名一致）
    fn id(&self) -> &'static str;

    /// 启动钩子（并行执行）
    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> { Ok(()) }

    /// 清理钩子（退出时并行执行，与 setup 对称；setup 零跨扩展依赖故无序约束，v1.6 N5）
    async fn teardown(&self, _app: &AppHandle) {}
}
```

**并行 bootstrap**（9 个 native 扩展 setup 无依赖，全仓零跨扩展 import）：

```rust
pub async fn bootstrap(app: &AppHandle, exts: Vec<Box<dyn Extension>>) -> tauri::Result<()> {
    let results = futures::future::join_all(
        exts.iter().map(|e| e.setup(app))
    ).await;
    for r in results { r?; }   // 任一失败则中断
    Ok(())
}
```

**执行器（关键，sync 闭包内跑 async）**：`bootstrap` 从 `tauri::Builder::setup` 的**同步闭包**调用，无法直接 `.await`。用 `tauri::async_runtime::block_on(join_all(...))` 阻塞执行。约束：

- `Extension::setup` 必须是 `async fn`，且内部 IO 非阻塞（DB 打开 / watch 启动等用 `tokio::task::spawn_blocking` 或异步 API）。
- 并发 setup 共享 `&AppHandle` 调 `app.manage(T)` —— Tauri manage 线程安全，但**并发 manage 顺序不确定**；依赖前序 manage 产物的场景必须禁跨扩展依赖（当前零依赖，§setup 并行竞态约束已覆盖）。

**不引入 Phase/SetupContext**：9 个 native 扩展 setup 全部能用裸 `&AppHandle` 满足（screenshot 调 `runtime::storage::cleanup_temps_by_prefix` + screenshot/translate 调 `runtime::shortcut::register_shortcut_hook` 均直接调用）。Phase（Early/Normal）零消费者、SetupContext.has_service 零消费者。给零需求系统加抽象违背 §0.2 机制最少化。

**双阶段时序（并行 bootstrap 的竞态边界，关键）**：

- **pre-bootstrap**（lib.rs，bootstrap 之前，**串行**）：框架级共享资源初始化。当前唯一项 `platform::selection::init_ax_timeout()`（AX API 全局超时，多扩展共享）。此类初始化**不可下沉扩展 setup**——并行 bootstrap 时无法保证时序，且属框架共享资源非扩展职责。
- **bootstrap**（registry join_all 并行）：扩展自身 State + 扩展级共享 State（agent SessionRegistry/ApprovalManager）+ 跨扩展可见副作用（快捷键钩子注册、窗口配置）。

**setup 并行竞态约束**：bootstrap 改 join_all 后，A.setup 不应依赖 B.setup 的产物——setup 内**禁跨扩展调用 + 禁框架级共享资源初始化**（当前零跨扩展依赖）。出现真实 setup 依赖时再加，即使加也优先扩展 setup 签名参数，不引入 Phase enum。

### 2.2 TS Extension 接口

扩展对外只暴露 `defineExtension({...})` 一个入口。能力采用**槽机制**——每个能力扩展私有、单一。框架直接取"这个扩展的 search"。

**不设 contributes 全局聚合扩展点**：当前全仓零跨扩展 import、零跨扩展通用动作需求。按 §0.3「零消费者一律不预设」，不保留 contributes 字段。出现真实跨扩展需求（如"所有 file 结果可'在 Finder 显示'"由单扩展贡献给全局）再加，届时基于真实消费者规格化接口——零消费者时堆砌字段是诱导过早实现。

```ts
// src/runtime/types.ts

interface Extension {
  meta: ExtensionMeta
  setup?(app: AppHandle): void | Promise<void>
  teardown?(): void

  // 能力槽（单一）—— 均有真实消费者
  search?: SearchProvider
  onExecute?: (result: SearchResult) => void | Promise<void> // 搜索结果回车动作（扩展私有；模块入口结果走框架内置激活，见下方「执行分派」）
  mainView?: () => Component // 9 扩展：clipboard/screenshot/agent/translate/window-manager/settings/awake/zsh-as/finder-ext
  searchBarAccessory?: () => Component // 3 扩展：clipboard/agent/translate
  subviews?: Record<string, () => Component> // 1 扩展：screenshot{ocr}（扩展私有命名子视图，自消费）
  settingsView?: () => Component // 3 扩展：clipboard/agent/translate（**跨扩展契约**：settings 扩展 mainView 扫描消费，渲染各扩展配置子视图）
  windowViews?: Record<string, () => Component> // 2 扩展：screenshot{screenshot,pin-*}/window-manager{snap-panel}
  globalShortcuts?: ShortcutBinding[] // 4 扩展：clipboard/screenshot/agent/translate
  hints?: ModuleHints // enterHint 3 扩展（clipboard/ip/calculator）；multiSelectHint/deleteHint 各 1（clipboard）

  // UI 配置（非能力供给，扩展私有）
  placeholder?: string // 搜索框占位提示（激活模块时显示）；7 扩展：clipboard/calculator/currency/ip/base64/time/uuid
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
  enter?: string // ↵ 动作描述（如「粘贴」「复制」）
  multiSelect?: string // 多选提示（如「⇧/⌘ 多选」）
  delete?: string // 删除提示（如「删除」）
}

interface ShortcutBinding {
  id: string // 快捷键业务 id（如 'screenshot'、'translate'）
  default?: string // 默认组合（如 'cmd+shift+s'），可被用户覆盖
  onExecute: (wasVisible: boolean) => void // 触发回调
}
```

**windowViews 生命周期（重要边界）**：`windowViews` 槽只承担**前端视图组件供给**——对应的 OS 窗口实体（`main` / `screenshot` / `snap-panel`）仍由 `tauri.conf.json` **静态声明**（Tauri 不支持运行时动态注册窗口配置）。窗口的原生配置（透明 / 跨 Space / 禁阴影 / level）由对应扩展在 `setup` 内调 `platform` 原语完成（screenshot `setup.rs::configure_overlay_window` / window-manager setup 内配 snap-panel）。框架不为 windowViews 自动创建窗口——扩展假设其窗口已由 tauri.conf.json 存在，仅负责 show/hide + 视图渲染。新增 windowViews 的扩展须同步在 tauri.conf.json 加窗口声明。

**`check:extensions` 漂移校验（v1.5 A4）**：CI 须断言——声明 `windowViews` 槽的扩展，其每个 key 必须在 `tauri.conf.json` 的 `windows[].label` 中存在，否则失败；防窗口声明与扩展视图漂移（扩展自治单元原则下，框架配置文件耦合处须有兜底校验）。

**槽位判据**：归属与基数——服务自身且单一 → 槽。每个槽均有真实消费者（消费者数见上方注释）。`settingsView` 是唯一**跨扩展契约槽**（settings 扩展消费所有扩展的 settingsView），故独立于扩展私有的 `subviews`——避免 magic string key 模糊归属。

**执行分派（v1.6 N1，框架内置契约）**：搜索结果回车分两路，由 `data.kind` 分派：
- `data.kind === 'module'` 且 `data.moduleId` 存在 → **框架内置激活**：`setActiveModule(data.moduleId)`，**不走 onExecute**（模块入口结果，由 keywordSearchAll 产出，见 §2.5）
- 其余 → 扩展 `onExecute` 槽，执行后框架回到全局模式（`setActiveModule(null)`）

**不预设的扩展点**（YAGNI，出现需求再加）：

- ~~`settingsPanel` 槽~~：settings 扩展通过自身 `mainView` 渲染整个面板，扫描各扩展 `settingsView` 槽聚合配置子视图（不再用 subviews.settings magic key，v1.6 N3）
- ~~`contributes.resultActions`~~：零跨扩展通用动作消费者，onExecute 槽已承担结果回车
- ~~`contributes.services`~~：零跨扩展 import，服务机制零消费者
- ~~`contextMenuItems`~~：当前无右键菜单 UI
- ~~`searchRerankers`~~：会破坏分组语义（§2.5），无消费者
- ~~`contributes.shortcutSlots`~~：与 `globalShortcuts` 槽重叠
- ~~`contributes.statusBarItems`~~：StatusBar 是框架全局组件，零扩展贡献
- ~~`contributes.configFieldTypes`~~：随 field builder 删除而废

**关于 ExtContext.config/service**：原设计 setup ctx 提供 config + service 句柄。config 删除——扩展通过 `useConfig(config)` 自管（reactive + watch 自动持久化）；service 零消费者删除。setup 内仅传裸 `AppHandle`。

### 2.3 SearchProvider（单通道）

```ts
interface SearchProvider {
  /** 动态召回：每次查询并行调用，带超时 + abort。
   *  **abort cleanup 按资源型分流**（v1.5 B4 收窄）：
   *    - 持有**非自动释放资源**（事件订阅 / 子进程 / 手动连接池）的 provider 须在 ctx.signal.abort 时 cleanup（关连接 / kill 子进程），并随附 cleanup 测试；
   *    - 纯 invoke/sync 型（Tauri invoke 不可取消，Rust 命令自行完成清理无泄漏）abort = 丢弃过期结果，无需测试；
   *    - 纯 fetch + signal 透传型（如 currency/ip）连接随 abort 自动释放，**无需额外 cleanup 测试**。
   *  半静态内容（如 base64 编码/解码选项）由扩展内部用模块级缓存自管，走 dynamic 返回。
   *  返回项**禁止带 score**（框架统一重算）；扩展可用 `boost` 表达组内优先级（§2.5）。 */
  dynamic(query: string, ctx: SearchContext): SearchResult[] | Promise<SearchResult[]>
}

interface SearchContext {
  signal: AbortSignal // 新查询覆盖旧查询时 abort；持有非自动释放资源的 provider 须 addEventListener('abort', cleanup)（纯 fetch+signal 透传型无需，见 dynamic 注释）
}

interface SearchResult {
  id: string // 扩展内 localId（自管唯一，如 'encode'、'item-123'）；框架去重用 `<module>:<id>` 组合键（§2.5），扩展无需 prefix module
  title: string // 进拼音索引，由框架统一打分
  module: string // **框架自动注入，扩展禁填**（v1.6 N4）：dynamic 结果 = 产出扩展 meta.id；keyword 模块入口结果 = 目标模块 id。dedup 键 `<module>:<id>`（§2.5）
  description?: string
  icon?: string
  shortcut?: string
  data?: SearchData
  boost?: number // 扩展可选组内优先级提示（默认 0）；框架 finalScore = fuzzy(title,query) + boost（§2.5）。如 search-apps 的 frequencyBoost+recencyScore、folder 的 +80 均填此
  score?: number // **仅框架填，扩展禁止填**；扩展调整相关性只能通过 boost
}

interface SearchData {
  kind: SearchResultKind // 必填：分组依据（经 getGroupKey 映射到组，§2.5）；扩展须正确设置，否则分组错乱
  moduleId?: string // 模块入口结果专用（v1.6 N1）：`kind==='module'` 时必填，框架内置激活此模块（见 §2.2 执行分派）
  path?: string
  [key: string]: unknown
}

// 严格枚举（禁止任意 string）。
// kind→group 映射：file/folder 同属 'file' 组（getGroupKey 合并，v1.5）；其余 kind 即组名。
// GROUP_ORDER/GROUP_TITLES 按 group 索引（§3.1）；新增 kind 须同步 getGroupKey + GROUP_* 语义常量
type SearchResultKind = 'application' | 'folder' | 'file' | 'module' | 'clipboard' | 'web'
```

**kind 分类变更（相对现状的行为变更，须 E2E 覆盖）**：

- 合并：现状 `'web-search' / 'open-url'`（`module-registry.ts:62-66`）→ 统一 `'web'`。
- **folder/file 维持同组**（v1.5 A2/A3，supersede v1.1 拆分决策）：`file` 与 `folder` 保留为**两个 kind 值**（图标/展示区分），但**同属 'file' 分组**（保留 `module-registry.ts:69-72` 的 getGroupKey 合并语义）；folder 的组内优先由扩展填 `boost` 表达（如 search 给 folder +80），不再用拆组强制。理由：A1 的 boost 使拆组失去必要，且避免「folder 恒先 file 精确匹配」回归与「跨组共享限流」矛盾。
- 组间顺序变更：现状 `application → module → clipboard → web → file`（module 高于文件）→ 新设计 `application → file → module → clipboard → web`（文件跃升至 module 之前）。理由（§3.1）：查找文件是启动器高频动作，应高于操作类。
- 影响：用户可见的相关性排序变化；`module-registry.ts::getGroupKey` **保留**（仍合并 file/folder → 'file'）；`groupAndSort` 重写读 GROUP_ORDER + finalScore（含 boost）。

**搜索机制精简**：当前 4 套机制（onSearch/onModuleSearch/searchItems/keywordSearchAll）边界模糊。新设计映射：

| 当前机制           | 新接口归属                                                                                                                                                                                                   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `onSearch`         | `SearchProvider.dynamic`（全局聚合）                                                                                                                                                                         |
| `onModuleSearch`   | **删除概念**——SearchEngine 双模式替代：全局模式（默认）聚合所有扩展 dynamic；模块模式（激活时）只调用激活模块 dynamic。search 槽接口不变，模式切换在 SearchEngine 内（§2.5），保持「激活模块只看模块内容」UX |
| `searchItems`      | **删除**——settings 改走 dynamic 返回静态项                                                                                                                                                                   |
| `keywordSearchAll` | **框架内置**（基于 meta.keywords 自动匹配模块入口，留 search-engine 内）                                                                                                                                     |

**不设 staticItems 双通道**：search 扩展用 `appListCache` 内存缓存替代，唯一类似场景 settings 的 searchItems（1 消费者）改走 dynamic。半静态内容走 dynamic + 模块级缓存即可，无需引入 staticItems 机制（YAGNI）。

### 2.4 扩展配置（defineConfig 形态）

保持当前 defineConfig 形态（原 `defineExtensionConfig + field builder + zod` 目标已废弃：YAGNI，defineConfig 已够用且类型推导可接受）。

```ts
// extensions/<id>/config.ts
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('clipboard', {
  maxDays: 30,
  // 其他默认值...
})
```

**自动持久化**：`defineConfig` 内部 `reactive()` + `watch(deep)` + 防抖（300ms）写入 `extensions/<id>/config.json`。扩展通过 `import { config } from './config'` 直接使用，tsc 推导字段类型。

**store 实例缓存（v1.5 B1）**：`defineConfig` 须缓存 `load()` 返回的 store 实例（模块级 `Map<extId, Store>`），watch 回调复用，**禁止每次保存重新 `load()`**（现网 `storage.ts:54-60` 每次防抖回调重新开 store，需改造）。

**加载语义（v1.6 N10）**：`load()` 异步，扩展 setup 阶段或早期命令调用可能读到 **defaults 而非持久化值**（磁盘值尚未回填）。agent 安全参数由 Rust clamp 兜底（defaults 经 clamp 仍安全），但 UI 可能短暂显示 defaults——属可接受竞态，无需阻塞首帧。

**安全项的双层表达（agent 专属，见 §3.4）**：plain `BOUNDS` const 导出底线元信息（floor/cap）。权威源在 Rust const（`native/policy.rs`），agent_run 入口强制 clamp 到 [floor, cap]；TS 端 `BOUNDS` 仅是 UI 镜像（Settings.vue 越界警告），不参与持久化、不被 Rust 信任。

**settings 面板渲染**：各扩展 `Settings.vue` 自管渲染（原 `ConfigField.vue` 声明式渲染器目标废弃）。

### 2.5 搜索引擎管道

```ts
// src/runtime/search-engine.ts

class SearchEngine {
  private currentController?: AbortController
  private activeModule?: string // 激活模块 id（undefined = 全局模式）

  /** 模块激活/退出时切换模式。激活模块时只调该模块 dynamic；退出时恢复全局聚合 */
  setActiveModule(id: string | undefined) {
    this.activeModule = id
  }

  async search(query: string): Promise<SearchResult[]> {
    this.currentController?.abort() // 取消上一次，触发其 dynamic cleanup
    const controller = new AbortController()
    this.currentController = controller

    // 1. dynamic 并行召回（带超时 + abort；扩展负责 abort 时 cleanup）
    //    模块模式下只调用激活模块的 dynamic（高效），全局模式聚合所有扩展
    //    召回时框架按产出扩展 meta.id 注入 module（扩展禁填，v1.6 N4）
    let results = await this.searchDynamic(query, controller.signal)

    // 1.5 keyword 合流（框架内置，v1.6 N2）：全局模式下匹配 meta.keywords 产出模块入口结果，
    //     合流到 results（结果已带目标 module id）；**模块模式禁用**（已在某模块内，不展示其他模块入口）
    if (!this.activeModule && query.trim()) {
      results = [...results, ...this.keywordSearchAll(query)]
    }

    if (controller.signal.aborted) return []

    // 2. 框架去重（按组合键 `<module>:<id>`，见 §2.3）
    results = dedupe(results)

    // 3. 框架分组排序：读 constants.GROUP_ORDER，分组限流 + 组内按 finalScore（= fuzzy + boost）排
    results = this.groupAndSort(results)

    return results
  }

  private keywordSearchAll(query: string): SearchResult[] {
    // 框架内置：扫描 getAllExtensions() 的 meta.keywords，scoreFields 匹配，
    // 命中产出模块入口结果 { module: 目标模块id, data:{kind:'module', moduleId}, boost: KEYWORD_MODULE_BOOST }（§3.1）
    // 这些结果回车走框架内置激活（§2.2 执行分派），不走 onExecute
  }

  private async searchDynamic(query: string, signal: AbortSignal): Promise<SearchResult[]> {
    // 全局模式：并行调用所有扩展的 dynamic；模块模式：只调用 activeModule 对应扩展的 dynamic（零额外开销）
    // 超时读 LIMITS.searchTimeoutMs；signal 透传给扩展
    // 半静态内容（如 base64 编码选项）由扩展内部模块级缓存自管，走 dynamic 返回
    // **框架注入 module = 产出扩展 meta.id**（扩展返回时禁填 module，v1.6 N4）
  }

  private groupAndSort(results: SearchResult[]): SearchResult[] {
    // 分组（file/folder 同属 'file'，经 getGroupKey）→ 组内按 finalScore 降序 → 组间按 GROUP_ORDER 定序 → 组内限流
    // finalScore = fuzzy(title, query) + (item.boost ?? 0)；boost 为扩展可选组内优先级提示（§2.3）
  }
}
```

**管道层次（不可破坏）**：去重 → 分组 → 组内排序 → 组间定序 → 组内限流。**组间定序由 GROUP_ORDER 锁死，不开放给扩展调整**——否则破坏分组语义、引入不可调试的相关性回归。扩展调整相关性的唯一通道：`data.kind` 归组（决定组间位）+ `boost`（决定组内位）；改组间序只能提案改 GROUP_ORDER 语义常量。

### 2.6 统一 platform 工具

platform 层只暴露**无状态原语**。snapshot/restore 作为不可变快照（无状态无副作用）保留 platform 层；真正的有状态/事务性操作（当前零需求）才考虑上移 runtime。

```rust
// platform/input.rs（统一三套 CGEvent 注入）
pub enum Modifier { Cmd, Shift, Opt, Ctrl }   // 不含 Fn：macOS 上 Fn 是硬件键非修饰键

pub fn post_key(key_code: u16, modifiers: &[Modifier], pid: Option<i32>)
pub fn post_combo(combo: &str, pid: Option<i32>)         // "cmd+c"、"cmd+v"、"cmd+shift+."
pub fn post_keystroke(string: &str, pid: Option<i32>)    // 输入字符串

// platform/pasteboard.rs（无状态原语全集 + snapshot/restore 不可变快照）
//   注：read_text/string_for_type/data_for_type/has_type/change_count/snapshot/restore 已实现；
//       write_text/read_image/write_image 为目标 API（clipboard 迁移前需先补齐，见 STATUS 阶段 2 步骤 16）。
//       snapshot 是 read_text + string_for_type + data_for_type + change_count 的不可变组合，
//       无状态无副作用——符合 platform 原语原则（零事务语义需求上移 runtime）。
pub fn read_text() -> Option<String>
pub fn write_text(s: &str)
pub fn read_image() -> Option<NSImage>
pub fn write_image(img: &NSImage)
pub fn change_count() -> i32                              // 轮询检测变化用
pub fn string_for_type(ty: &str) -> Option<String>        // 富文本等类型读取
pub fn data_for_type(ty: &str) -> Option<Vec<u8>>
pub fn has_type(ty: &str) -> bool
pub fn snapshot() -> PasteboardSnapshot                   // 不可变快照（screenshot/translate 无痕粘贴用）
pub fn restore(snap: &PasteboardSnapshot)

// platform/focus.rs（PREV_FRONT_PID 唯一源）
pub fn current_frontmost_pid() -> Option<i32>             // 排除自身
pub fn capture_frontmost() -> i32                         // 记录到唯一源
pub fn restore_frontmost(pid: i32)                        // 还给原 app
pub fn activate_app(pid: i32)
pub fn deactivate_self()

// platform/path_guard.rs（policy 化：不同调用方信任级不同）
pub enum Policy {
    Interactive,   // finder-ext：用户主动右键操作，尊重用户选择，仅拦系统致命路径
    Automated,     // agent：AI 自动执行，严格黑名单 + 符号链接解析 + canonicalize
}
pub fn validate(path: &Path, policy: Policy) -> Result<PathBuf>
//   Interactive：canonicalize + 拦 ["/System", "/usr/bin", "/bin", "/sbin"]
//   Automated：在 Interactive 基础上 + 拦 ["/Library", "/opt/homebrew"] + 拒绝符号链接逃逸
```

**为何 policy 化**：统一 BLOCKED_PREFIXES 对 finder-ext（用户主动选 /Library 路径应尊重）过度限制，对 agent（AI 想碰 /usr/bin 应拦）又欠保护。信任级是调用方的固有属性，校验必须感知它。

### 2.7 TempHandle RAII 注册表

**settings.json / extensions/<id>/config.json 全部由前端持久化**（stores/settings.ts + defineConfig），**业务层 Rust 代码零消费者读这些 JSON**——配置值通过命令参数注入（agent_run 接收 endpoint/model/api_key/trusted_commands 等）。持久化由 `tauri-plugin-store` 基础设施承担（前端 `load()` 读写，非业务 Rust 直访）。因此业务层 Rust **不设 StorageHandle 抽象**（§0.2 机制最少化：零消费者一律不预设）。

**TempHandle 归属判据**：当前唯一消费者是 screenshot（6 处 temp_dir），但 TempHandle 是**通用基础设施**（非业务逻辑），任何需要临时文件的扩展都可消费——与 agent engine（agent 专属业务逻辑，单消费者下沉）不同。判据：通用基础设施按"潜在多消费者"留框架层，业务逻辑按"实际消费者数"判归属。

```rust
// runtime/storage.rs（仅 TempHandle RAII guard + 退出兜底）
pub struct TempHandle { path: PathBuf }

impl TempHandle {
    /// 创建 guard：注册路径到全局表，Drop 时自动注销 + 删除文件。
    /// screenshot 持有 guard 于窗口 State，pin 窗口关闭时 Drop 自动清理。
    pub fn new(path: PathBuf) -> Self { /* register 到全局表 */ }
}

impl Drop for TempHandle {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);   // 异步 detach 见下方「Drop 行为」
        // 从全局表注销
    }
}

/// 应用退出时阻塞清理所有残留（兜底异常退出，Drop 已清理的正常情况空转）
pub fn cleanup_all_temps()
```

**Drop 行为**：

- `TempHandle` 实现 `Drop`：`new()` 时注册路径到全局表，Drop 时自动从全局表移除 + 删除文件。
- **同步约束**：Drop 在 tokio runtime 内同步执行，**不能 await**。大文件 IO（截图、命令输出）用 `tokio::task::spawn_blocking` 包装后 **detach**（fire-and-forget，丢弃 JoinHandle）——Drop 内不阻塞等待 spawn_blocking 完成，避免 runtime 死锁。
- **应用退出**：`cleanup_all_temps()` 阻塞完成（确保所有临时文件清理后再退出），最坏情况 100ms 超时强制返回。与 Drop 的 detach 路径独立——退出时走阻塞清理，运行时 Drop 走异步 detach。
- **定期清理**：screenshot on_setup 内启动定时任务（每小时扫一次 `voidnix*` 前缀），兜底异常退出残留。

**配置持久化（前端管，非 Rust 端职责）**：

- per-ext 独立 config.json，无跨扩展竞争。
- 前端 `defineConfig` 内部 `reactive()` + `watch(deep)` + `debounce(300ms)` 写入——避免 slider 拖动等高频变更把整个 JSON 序列化+落盘 N 次。
- **持久化语义**：由 `tauri-plugin-store` 承担（`store.set` + `store.save`），写入原子性由插件保证；前端无需手写 `tmp+rename`。
- **store 实例缓存（v1.5 B1）**：`defineConfig` 须缓存 store 实例复用，禁止每次保存重新 `load()`（见 §2.4）。

### 2.8 命令注册机制

每个 native 扩展在 `init()` 内局部注册命令。**双 setup 职责判据**（按副作用可见性划分）：

- **plugin `.setup`**：负责 `invoke_handler` 注册 + 命令执行所需 State（`app.manage(DB/monitor)` 等扩展内部依赖）
- **Extension trait `setup`**：负责**跨扩展可见的副作用**（快捷键钩子注册、窗口配置、`app.manage` 扩展级共享 State 如 agent SessionRegistry）

理由：命令注册必须在 plugin Builder 内（Tauri 约束），而 Extension trait 是 runtime registry 的生命周期钩子（参与并行 bootstrap）。两者职责正交，不合并。

**生命周期时序（关键，证明 State 可达性）**：

```
app 构建（各扩展 plugin .setup 执行）
  ├─ invoke_handler 注册各扩展命令
  └─ 各扩展 .setup 内 app.manage(DB/monitor) ← 命令执行依赖的 State
↓
.run()（ExtensionRegistry::bootstrap 执行）
  ├─ join_all 并行调用各 Extension::setup
  │   └─ agent setup 内 app.manage(SessionRegistry/ApprovalManager) ← 扩展级共享 State
  ↓
事件循环（命令可调用）
  └─ agent_run 通过 State<SessionRegistry> 取值 ← 保证已 manage
```

命令通过 `State<T>` 取值的前提是 `app.manage(T)` 在命令可调用前完成。plugin .setup 在 app 构建时执行，bootstrap 在 `.run()` 内执行——两者均早于任何命令调用，时序安全。

```rust
// extensions/clipboard/native/mod.rs
pub fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::new("clipboard")
        .invoke_handler(tauri::generate_handler![
            get_items, paste_item, toggle_favorite, delete_item, clear_items
        ])
        .setup(|app| {
            // 启动 DB + monitor（命令执行依赖的 State）
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
    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // 跨扩展可见的副作用（快捷键钩子注册、窗口配置、扩展级共享 State 注入等）
        // 直接调 runtime::storage / runtime::shortcut 等，无 SetupContext 抽象
        // 注：命令执行所需 State（DB/monitor）在上方 plugin .setup 内 app.manage，此处不重复
        Ok(())
    }
}
```

`sync-extensions.ts` 简化为 < 50 行：扫描 `extensions/*/native/mod.rs` 的 `pub fn init()` 签名，生成 `extensions.rs`（`.plugin()` 链 + `#[path]` mod 声明）。

**框架命令 vs 扩展命令的注册边界**：命令分两类，注册位置不同——

- **扩展命令**（~55 个）：归各自扩展，在 `init()` 的 `invoke_handler(generate_handler![...])` 内局部注册。`extensions.rs`（自动生成）**零 `generate_handler!`**，只有 `.plugin()` 链（9 扩展 init()）+ `#[path]` mod 声明。
- **框架命令**（13 个：`runtime::permission::*` 5 + `runtime::shortcut::*` 5 + `runtime::window::*` 3）：不归任何扩展，是框架自身能力（权限检测/全局快捷键/主窗口）。在 `lib.rs` 保留一个**固定的、手写的** `generate_handler!`——框架自管，与扩展增减无关，不参与 sync-extensions 扫描，零漂移。

**shortcut/window 不需 plugin 形态**：当前 `runtime::shortcut::init()` / `runtime::window::init()` 是纯空 Builder（无 invoke_handler、无 setup，仅占 plugin name）。命令下沉后（框架 13 命令迁 lib.rs `generate_handler!`）**直接删除这两个空 plugin**——框架命令不需 TauriPlugin 包装，`generate_handler!` 即完成注册。删除后 `extensions.rs` 的 `.plugin()` 链**只含 9 个扩展**（search 统一化后也走 init()，见 §4）。

边界判据与存储结构同构：框架命令之于 `lib.rs`，等同框架配置之于 `config/settings.json`、框架 UI 之于 `components/layout/`——框架自管项驻框架层。`check:commands` 扫描**全仓** `#[tauri::command]`（框架 + 扩展）与前端 `commands.ts` 常量集合作差集，两者都覆盖。

```rust
// lib.rs（框架自管：固定 13 命令，手写，不参与 sync-extensions 扫描）
.invoke_handler(tauri::generate_handler![
    crate::runtime::permission::check_accessibility_permission,
    crate::runtime::permission::request_accessibility_permission,
    // ... runtime::permission::* (5) + runtime::shortcut::* (5) + runtime::window::* (3)
])
```

> **「零正则」精确含义**：无 `COMMAND_REGEX`（不再扫 `#[tauri::command]` 汇总扩展命令到顶层 `generate_handler!`）；保留 `init()` 签名检测的正则（`/pub fn init\(\)/`，必要）。

**specta 决策：删除，但诚实面对信息损失**

specta 删除的理由（割裂、不在主构建、仅覆盖部分命令）成立，但删除带来真实信息损失：**Rust 端改命令参数/serde 表示，前端 `types/` 不同步，tsc 发现不了，运行时才崩**。文档不回避这个限制。

**阻塞项（必做）**：

1. **`check:commands` CI**：扫描 Rust `#[tauri::command]` 名集合，与前端 `src/commands.ts` 常量集合作差集比对——**能抓命令名漂移**（Rust 删/改名，前端没跟）。当前约 40 个裸 invoke 是真实痛点，必须先补 CI 兜底。
2. 前端所有 `invoke()` 走 `src/commands.ts` 常量通道（**禁止裸字符串**），tsc 保证命令名拼写 + 存在性。
3. **无法自动抓的**：命令参数签名漂移（Rust 改字段类型/重命名 serde 字段）。靠**人工 checklist**——Rust 命令签名变更必须同步前端 `types/`，PR review 强制核对。

`commands.ts`：命令名常量，**替换当前 `bindings.ts` 的 specta 残留**。bindings 类型完全手写于 `types/`，tsc 严格校验调用点。

---

## 3. 三层配置金字塔

### 3.1 语义常量（仅前端 src/runtime/constants.ts）

不可配置，集中定义单一真相源。**Rust 端无常量集中文件**（搜索逻辑全在前端 fuzzy.ts + search-engine.ts，Rust 端零消费者；LLM 请求管道常量随 security.rs 溶解并入 client.rs，见 §1.1）：

```ts
// src/runtime/constants.ts
export const SEARCH = {
  WEIGHTS: { prefix: 1000, contains: 600, decay: 0.85, logBase: 2, logMul: 50, cap: 320 },
  // 组间定序严格锁死；不设组级 GROUP_BOOST（GROUP_ORDER 已定组间序，组级 boost 是死机制）。
  // 扩展可用 per-item `boost`（SearchResult.boost）调整组内优先级（§2.3/§2.5）。
  // 顺序按使用频率第一性推导（§0.2）：启动应用 / 查找文件是启动器最高频动作，操作类工具次之，剪贴板辅助，web 垫底
  GROUP_ORDER: ['application', 'file', 'module', 'clipboard', 'web'] as const,
  GROUP_TITLES: {
    application: '应用',
    file: '文件', // file 与 folder 共用（同组，仅 kind 值区分，v1.5）
    module: '操作',
    clipboard: '剪贴板',
    web: '快捷操作',
  },
  KEYWORD_MODULE_BOOST: 500, // keywordSearchAll 产出的模块入口结果组内加权（v1.6 N6，原 module-helpers.ts:45 魔数）
} as const

export const LIMITS = {
  maxAppResults: 30,
  maxFileResults: 50, // file 组限流（含 folder；v1.5 恢复合并，单组计数，无跨组共享）
  searchTimeoutMs: 3000,
} as const
```

### 3.2 框架可调参数（stores/settings.ts）

保持当前 stores/settings.ts 形态（原 framework.json + useAppConfig 目标已废弃：YAGNI，当前 181 行已简洁管理 2 组配置）。

**Rust 端不读 settings.json**：配置值通过命令参数注入（如 agent_run 接收 endpoint/model/api_key/trusted_commands）。settings.json 是前端单一消费者，Rust 端零读取——故不设 Rust 端 StorageHandle 抽象（§2.7）。

```ts
// src/stores/settings.ts
// config/settings.json
{
  "shortcuts": { "global": "cmd+shift+space", "overrides": {} },
  "aiProviders": {
    "configs": [{ "id": "", "endpoint": "", "apiKey": "", "models": [] }],
    "activeProviderModelKey": ""
  }
}
```

- **快捷键**：`global`（主快捷键）+ `overrides`（扩展快捷键覆盖，按 id 索引）
- **AI Provider**：`configs`（多 provider 列表）+ `activeProviderModelKey`（`<providerId>::<model>` 格式）

### 3.3 扩展可调参数（extensions/<id>/config.json + defineConfig）

每扩展完全自管，`defineConfig('<id>', { ...defaults })` 形态（见 §2.4）。各扩展 Settings.vue 自管渲染。

### 3.4 agent 9 层防御全部 config 化（双层安全）

安全敏感项采用**双层模型**：`floor ≤ 用户值 ≤ cap`。用户在 UI 配的值由 Rust 端读取后**强制 clamp** 到 [floor, cap]——直改 config.json 写入越界值无效。黑名单类（forbiddenCommands/blockedArgs）用户只能**加严**（用户列表 ∪ 硬编码底线），不能放宽。

**表达方式**：`defineConfig` 声明用户可调值（plain 值）；安全底线以**独立 plain const `BOUNDS`** 表达（替代原 field builder + secured 包装，见 §2.4）。

#### 权威源（不可含糊，安全相关）

floor/cap 是**安全底线，权威定义在 Rust 端**，TS 端只是 UI 镜像。理由：

- config.json 只存用户配置值（`defineConfig` 的 plain 值）。`BOUNDS` const 是代码内字面量，**不进入 config.json**——若让 floor/cap 也写进 JSON，用户直改 config.json 即可绕过，核心安全承诺失效。
- 因此 **Rust 端是唯一权威源**：floor/cap 定义在 `extensions/agent/native/policy.rs`，`agent_run` 入口按 Rust const 强制 clamp，**不信任任何前端传值**。
- **TS 端 `BOUNDS` 降级为 UI 镜像**：仅用于 Settings.vue 越界输入警告（不阻止输入，Rust 兜底）；警告阈值读 TS `BOUNDS` 镜像，可能与 Rust 权威有偏差（靠注释同步），但不影响安全。

> 单一真相源原则的诚实落地：Rust const 是真相，TS 镜像是衍生。不追求 TS/Rust 自动同步（agent 是此机制唯一消费者，构建期生成镜像违背 YAGNI），靠注释 + PR review 保持一致。

> 为何 plain const 而非 secured() 包装：secured() 需 `Object.defineProperty` 挂非枚举属性，经 `reactive()` Proxy 包装后行为不确定；且其唯一消费者是 Settings.vue，plain const 完全覆盖、零魔法（§0.2 机制最少化）。

agent config 目标态示例：

```ts
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('agent', {
  // —— 可调项（用户自由）——
  maxTurns: 10, // 默认 10（= Rust 端 DEFAULT_MAX_TURNS，靠注释同步）；用户可调，Rust 端 clamp 到 BOUNDS [1, 50]
  systemPrompt: '', // 空串用 DEFAULT_SYSTEM_PROMPT fallback（Rust 端 mod.rs:80）；非空用用户值

  // —— 安全底线项（Rust 强制 clamp/并集，config.json 越界无效）——
  //   BOUNDS（见下方）仅是 UI 镜像；权威在 extensions/agent/native/policy.rs，Rust agent_run 入口按 const 强制 clamp。
  //   ⚠️ BOUNDS 须与 policy.rs 手动保持同步。
  //
  //   trustedCommands 匹配语义：**程序名匹配（命令首 token）**，非复合前缀。
  //   即 trusted 含 'git' 则 `git status`/`git push` 全放行（危险参数由 blockedArgs 拦）。
  //   复合条目（如 'cargo build'）不会匹配——故默认值只列程序名。
  //   cargo/npm/bun/pnpm 等可执行任意构建脚本/	postinstall 的程序**默认不进白名单**（需审批），
  //   用户接受构建脚本风险后可自行追加 'cargo' 等。
  //   kill/ps/top 等侦察/进程控制程序在 forbiddenCommands.floor 中，永不进 trusted。
  //   下方默认值与现网 `extensions/agent/config.ts` 对齐（用户精心精选的开发工具集），
  //   已通过安全不变量校验（无 kill/ps/top、无复合条目、与 forbidden floor 零交集）。
  //   用户可自由增删；唯一硬约束：trusted 不得与 forbiddenCommands.floor 有交集（Rust 端取并集时交集项仍被禁）。
  trustedCommands: [
    // 读 / 检索
    'ls', 'cat', 'pwd', 'echo', 'head', 'tail', 'wc', 'file', 'stat', 'date',
    'which', 'whoami', 'uname', 'find', 'grep', 'rg', 'fd', 'ag', 'tree', 'diff',
    'comm', 'cmp', 'md5sum', 'shasum',
    // 文本处理
    'sort', 'uniq', 'cut', 'tr', 'paste', 'expand', 'sed', 'awk', 'jq', 'yq', 'bat',
    // git（程序名匹配，所有子命令放行；危险参数由 blockedArgs 拦，如 git -C / git --upload-pack）
    'git',
    // 文件操作（写）
    'mkdir', 'touch', 'cp', 'mv', 'ln', 'tee', 'truncate',
  ]

  // forbiddenCommands 用户值与底线取并集——用户只能加严（底线见 BOUNDS，= 现网 FORBIDDEN_PROGRAMS 全集）
  forbiddenCommands: [], // 用户自定义补充
  // blockedArgs 同 forbiddenCommands 机制——用户值与底线取并集，用户只能加严（底线 = 现网 DENIED_ARG_PREFIXES 全集）
  blockedArgs: [], // 用户自定义补充（底线见 BOUNDS.blockedArgs.floor）

  maxCpuSeconds: 30,
  maxMemoryMb: 512,
  maxOpenFiles: 64,
  executionTimeout: 30,
  maxOutputBytes: 1048576,

  // 非安全项保持现状
  searchProviders: [] as SearchProviderConfig[],
  activeSearchProviderId: '',
})

// 安全底线 UI 镜像（权威在 native/policy.rs，⚠️ 须手动同步）
//   不变量：floor 必须 ⊇ 现网 tools/run_command.rs 的硬编码集合（FORBIDDEN_PROGRAMS / DENIED_ARG_PREFIXES），
//   迁移即取并集，禁止缩窄。新增危险项时 policy.rs 与此处同步追加。
export const BOUNDS = {
  maxTurns:         { floor: 1,    cap: 50 },       // 限制 agent 自主迭代轮数（防 runaway）
  maxCpuSeconds:    { floor: 1,    cap: 300 },
  maxMemoryMb:      { floor: 64,   cap: 4096 },
  maxOpenFiles:     { floor: 8,    cap: 1024 },
  executionTimeout: { floor: 1,    cap: 300 },
  maxOutputBytes:   { floor: 1024, cap: 10485760 },
  // forbiddenCommands 的 floor = 现网 FORBIDDEN_PROGRAMS 全集（31 项），与用户值取并集——用户只能加严
  forbiddenCommands: {
    floor: [
      // shell（任何 shell → 放弃 L1「不经 shell」防御）
      'sh', 'bash', 'zsh', 'dash', 'ksh', 'fish', 'csh', 'tcsh',
      // macOS 特权 / 系统控制
      'osascript', 'sudo', 'open', 'launchctl', 'defaults', 'networksetup', 'scutil',
      // 进程管理（kill/ps/top 等侦察/控制）
      'killall', 'kill', 'pkill',
      // 触网（走专用 web_search 工具）
      'curl', 'wget', 'nc', 'socat', 'telnet', 'ssh',
      // 提权 / 逃逸
      'su', 'doas', 'expect',
      // 数据持久化（走应用 API，防止直改 sqlite）
      'sqlite3',
      // 侦察
      'ps', 'top', 'htop',
    ],
  },
  // blockedArgs 的 floor = 现网 DENIED_ARG_PREFIXES 全集（15 项），与用户值取并集——用户只能加严
  blockedArgs: {
    floor: [
      '--exec', '--exec-file', '--exec-rm',
      '--upload-pack', '--use-compress-program',
      '--config', '-C',                       // git -C 改 cwd / curl --config 读配置
      '--output', '-o', '-O', '--write-out',  // curl/wget 写文件
      '--eval', '-e',                         // node/bash eval
      '--init-file', '--rcfile',
    ],
  },
} as const
```

**Settings.vue 读取 BOUNDS 示例**（直接 import，无 descriptor 魔法）：

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { config, BOUNDS } from './config'

// slider min/max + 越界警告
const cpuFloor = BOUNDS.maxCpuSeconds.floor // 1
const cpuCap = BOUNDS.maxCpuSeconds.cap // 300

// forbiddenCommands 底线展示（UI 标注「不可移除的底线」）
const forbiddenFloor = BOUNDS.forbiddenCommands.floor // 31 项底线（shell/macOS 特权/触网/侦察/进程控制），见上方 BOUNDS 定义

// trusted ∩ forbidden floor 交集实时警告（v1.5 B2）
//   用户把 kill 等加进 trusted 无效（Rust 端取并集时被 forbidden floor 覆盖），UI 须标红提示
const conflictTrusted = computed(() =>
  config.trustedCommands.filter((c) => BOUNDS.forbiddenCommands.floor.includes(c)),
)
</script>
```

**命令执行链（层叠顺序，缺一不可）**：

1. **trustedCommands 白名单**：只有列表中的程序可执行（用户可调，defineConfig 声明；**程序名匹配**，首 token 比对）
2. **forbiddenCommands 黑名单**：即使 trusted 也禁（shell/macOS 特权/触网/侦察/进程控制 31 项；floor 与用户值**并集**，用户只能加严）
3. **blockedArgs**：参数前缀黑名单（15 项，含 `--exec/--upload-pack/--config/-C/--output/-o/-O/--eval/-e/--init-file/--rcfile` 等；floor 与用户值**并集**，用户只能加严）
4. **rm 断路器（硬编码、不可 config 化、不可放宽）**：`rm -rf / ~ /*` 形式无条件拒（`run_command.rs::is_circuit_breaker_hit`）。这是不可让用户关闭的最后一道防线，不进 BOUNDS、不进 config。
5. **rlimits**：CPU/内存/文件数限制（apply_rlimits，读 config 用户值 + policy.rs 底线 clamp）
6. **wall timeout + output bytes**：执行墙钟 + 输出字节上限（同上 clamp）

> L1（不经 shell，免疫元字符注入）/ L3（env_clear）/ L7（kill_on_drop + 显式 reap）等 inherent 防御**非 config 化**（机制本身固有，无用户可调面），不在上述链中列出但始终生效。**「9 层 config 化」精确含义**：9 层中可参数化的部分（白名单/黑名单/数值上限）config 化，固有机制保持代码内锁死。

**设计约束**：trustedCommands 默认值与 forbiddenCommands floor **无交集**（避免默认配置自相矛盾——trusted 白名单不会包含 forbidden 黑名单项；`kill/ps/top` 在 forbidden floor 故不进 trusted 默认）。

**关键约束**：

- `floor` / `cap` 是**不可绕过的底线**，**权威在 Rust const**，Rust 端命令执行入口强制 clamp。用户直改 config.json 写 `maxTurns: 9999`，实际跑 `min(9999, cap)`（config.json 里根本没有 floor/cap 可改）。
- `forbiddenCommands` 的 `floor`（Rust 端硬编码底线）与用户配置值取**并集**——用户只能添加禁止项，不能删除硬编码底线。
- `blockedArgs` 的 `floor` 同 `forbiddenCommands` 机制——与用户配置值取**并集**，用户只能加严。
- **trusted ∩ forbidden floor 交集（v1.5 B2）**：用户把 forbidden floor 项（如 `kill`）加进 trusted 无效——Rust 端取并集时交集项仍被 forbidden 覆盖。Settings.vue 须对 `config.trustedCommands ∩ BOUNDS.forbiddenCommands.floor` 实时标红警告（「被底线覆盖，加进 trusted 无效」）；UI 仅警示，Rust 端并集兜底。
- `DEFAULT_SYSTEM_PROMPT` 作为 `systemPrompt` 字段为空时的 fallback，定义在扩展内（`extensions/agent/native/mod.rs:80`），不下沉框架。用户配了 systemPrompt 用用户值，否则用 Rust const 默认。
- Settings.vue 渲染安全项时，越界输入给警告（不阻止输入——Rust 端兜底，UI 仅提示）；警告阈值读 TS `BOUNDS` 镜像，可能与 Rust 权威有偏差（靠注释同步），但不影响安全。

**Rust 端实现要点**：

- 新增 `extensions/agent/native/policy.rs`：集中定义所有 floor/cap 为 Rust const（`MIN_CPU_SECS`/`MAX_CPU_SECS`/`FORBIDDEN_FLOOR`/`DENIED_ARG_FLOOR` 等），是双层安全的**唯一权威源**。**初始值必须 ⊇ 现网 `tools/run_command.rs` 的 `FORBIDDEN_PROGRAMS`（31 项）/ `DENIED_ARG_PREFIXES`（15 项）/ `MAX_WALL_SECS=30` / `MAX_OUTPUT_BYTES=1MiB` / apply_rlimits（CPU 30、DATA 512MB、NOFILE 64）**——迁移即取现网全集，禁止缩窄。
- `extensions/agent/native/tools/run_command.rs` 的 `FORBIDDEN_PROGRAMS`/`DENIED_ARG_PREFIXES`/`MAX_WALL_SECS`/`MAX_OUTPUT_BYTES` + `apply_rlimits` 全部改读 config（用户值）+ policy.rs const（底线），用户值与底线取 clamp/并集后生效。`is_circuit_breaker_hit`（rm -rf 断路器）保持硬编码不变。
- **`agent_run` 入口集中 clamp，不信任前端传值**。参数清单（前端将整份 agent config 序列化注入，Rust 端逐项 clamp/并集）：
  - LLM 侧：`endpoint` / `model` / `api_key` / `system_prompt`（空串 fallback DEFAULT_SYSTEM_PROMPT）/ `max_turns`（clamp [1,50]）
  - 执行沙箱：`trusted_commands`（Vec<String>，程序名白名单）/ `forbidden_commands`（用户值 ∪ FORBIDDEN_FLOOR）/ `blocked_args`（用户值 ∪ DENIED_ARG_FLOOR）/ `max_cpu_seconds`（clamp）/ `max_memory_mb`（clamp）/ `max_open_files`（clamp）/ `execution_timeout`（clamp）/ `max_output_bytes`（clamp）
  - 其它：`search_providers` / `active_search_provider_id`（非安全项，原样存取）

> **路径说明**：agent 的 Rust 后端目录结构是 `extensions/agent/native/`，其中 `engine/`（loop_runner/approval/cancellation/secret_scrub/tool_registry）与 `tools/`（run_command/web_search）是 **平级兄弟目录**，非嵌套。`policy.rs` 放 `native/policy.rs`（与 engine/、tools/ 平级）。

---

## 4. 扩展清单与迁移映射（16 个）

所有扩展同构化，按是否含 native/ 区分实现方式（非分类）。

### 含 native/（9 个，需系统级能力）

| 扩展                | 迁移要点                                                                                                                                                                                                                                                                             |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| clipboard           | config 自管（maxDays）；**monitor/commands 完整迁移 platform/pasteboard**（当前 commands.rs 写入完全直访 NSPasteboard、monitor.rs file/PNG 直访；**迁移前需先在 platform/pasteboard 补 write_text/read_image/write_image**）；CGEvent 模拟 ⌘V 用 platform/input；命令局部注册 init() |
| screenshot          | config 自管（savePath）；temp 文件走 TempHandle；CGEvent 用 platform/input；scroll_capture/session/pin 保留                                                                                                                                                                          |
| awake               | config 自管；binary 路径 app_data_dir；TrayIconBuilder 保留                                                                                                                                                                                                                          |
| zsh-autosuggestions | signals.log 保持 1MB 截断；include*bytes! binary 保留；内部命名 `\_zsh_as*_`同步为`*zsh_autosuggestions*_`                                                                                                                                                                           |
| window-manager      | config 自管（customWidth/customHeight/dragSnapEnabled）；PREV_FRONT_PID 走 platform/focus；AX FFI 自管                                                                                                                                                                               |
| finder-ext          | 零横向依赖；IPC「处理完即删 + 启动清空」；路径校验走 platform/path_guard（Interactive policy）                                                                                                                                                                                       |
| translate           | config 自管（configs/targetLang）；自管 SELECTED_TEXT State（不泄漏框架）；text_selection 工具下沉扩展内                                                                                                                                                                             |
| agent               | 接收 core/agent/ 下沉为 native/engine/（loop_runner/approval/cancellation/secret_scrub/tool_registry）；9 层防御读 config + BOUNDS；prompt/turns 由 config 注入；secret_scrub 下沉 native/engine/（parser 留 runtime/llm，client.rs 消费）；trim_conversation 下沉 engine            |
| search              | 删 icon cache，改 NSWorkspace.icon 实时提取（零磁盘文件）；init() 局部注册；当前注册 2 hidden 模块（search-apps/search-files），迁移时合并为单 dynamic（按 kind 区分 application/file/folder）；**频率/最近使用加权（frequencyBoost/recencyScore）填入 `SearchResult.boost`，`APP_BOOST`/`(50-i)` pre-sort 随 application 组首位 + boost 自然消解删除（§2.3/§2.5，v1.5）；folder 组内优先用 boost 表达（不再拆组）**；统一 Extension trait |

### 纯 TS（7 个，无 native/）

| 扩展       | 迁移要点                                                                             |
| ---------- | ------------------------------------------------------------------------------------ |
| calculator | config 自管 history（走 storage）；search 走 dynamic（即时计算）；补 parser 单元测试 |
| settings   | 改为扫描各扩展 config.ts，渲染各扩展 Settings.vue 子视图；零硬编码聚合               |
| base64     | 从 .vnext 转 TS；search 走 dynamic（编码/解码选项用模块级缓存）                      |
| time       | 从 .vnext 转 TS；search 走 dynamic（格式选项用模块级缓存）                           |
| currency   | 从 .vnext 转 TS；HTTP 走前端 `fetch`（同 ip）；search 走 dynamic                     |
| uuid       | 从 .vnext 转 TS；search 走 dynamic（UUID/NanoID 选项用模块级缓存）                   |
| ip         | 转纯 TS（fetch，删 native/）                                                         |

---

## 5. 文件生产最小化

| 原生产                           | 新方案                                                                              | 收益                                              |
| -------------------------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------- |
| `search/icons/` 400 PNG          | **完全不缓存**，NSWorkspace.icon 实时提取（系统自带缓存）                           | 删 ~400 文件 + cleanup 逻辑 + icon_cache_dir 代码 |
| `finder-ext/commands/cmd_*.json` | **处理完即删 + 启动清空**（`mod.rs:87-106` + `:165`）                               | 零累积（运行时短暂存在，处理完即删）              |
| `zsh-as/signals.log`             | **保持 1MB 截断**（已工作，原 ring buffer 方案废弃）                                | 有上限                                            |
| screenshot/agent 临时文件        | **TempHandle 统一注册 + 退出/定期清理**                                             | 无残留                                            |
| clipboard/clipboard.db           | **启动时按 maxDays GC**（monitor 启动时 `DELETE WHERE created_at < now - maxDays`） | 有上限（maxDays 配置可调）                        |
| `.gitignore`                     | 补全 `test-results/`、`proptest-regressions/`、`.DS_Store`                          | 仓库零垃圾入库                                    |

保留：`.claude/`、`CLAUDE.md`、`.mcp.json`（开发工具）；`.prettierignore` + `.prettierrc`（标准配置）；clipboard SQLite WAL（性能保证）。

---

## 6. 关键代码模板

### 6.1 native 扩展 mod.rs 模板

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
            // 启动 DB + monitor（仅 plugin 自身的初始化，命令执行依赖的 State）
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
    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // 跨扩展可见的副作用（快捷键钩子注册、窗口配置、扩展级共享 State 注入等）
        // 直接调 runtime::storage / runtime::shortcut 等，无 SetupContext 抽象
        // 注：命令执行所需 State（DB/monitor）在上方 plugin .setup 内 app.manage，此处不重复
        Ok(())
    }
}
```

### 6.2 纯 TS 扩展 index.ts 模板

```ts
import { defineExtension } from '@/runtime/extension-registry'
import type { SearchResult } from '@/runtime/types'
import { copyAndShow } from '@/utils/clipboard'
import { base64Encode, base64Decode } from './utils'
import View from './View.vue'

// 模块级缓存：半静态内容自管，走 dynamic 返回
let cachedOptions: SearchResult[] | null = null
function getOptions(): SearchResult[] {
  if (!cachedOptions) {
    cachedOptions = [
      {
        id: 'encode',
        title: 'Base64 编码',
        module: 'base64',
        data: { kind: 'module', action: 'encode' },
      },
      {
        id: 'decode',
        title: 'Base64 解码',
        module: 'base64',
        data: { kind: 'module', action: 'decode' },
      },
    ]
  }
  return cachedOptions
}

export default defineExtension({
  meta: {
    id: 'base64',
    name: 'Base64',
    icon: 'i-ri-code-s-slash-line',
    order: 100,
    keywords: ['编码', '解码', 'encode', 'decode'],
  },

  search: {
    // 单通道 dynamic：半静态选项（模块级缓存）+ 实时结果合并
    dynamic: (query: string) => {
      const options = query
        ? getOptions().filter((o) => o.title.toLowerCase().includes(query.toLowerCase()))
        : getOptions()
      if (!query) return options
      const encoded = base64Encode(query)
      return [
        ...options,
        {
          id: 'result',
          title: encoded,
          module: 'base64',
          description: 'Base64 编码结果',
          data: { kind: 'module', action: 'encode', result: encoded },
        },
      ]
    },
  },

  mainView: () => View,

  // 结果回车动作走 onExecute 槽（扩展私有）
  // 注：禁止 navigator.clipboard（panel 策略下不抢 NSApp active，行为不稳定），
  //     统一走 @/utils/clipboard 的 copyAndShow/copyAndHide（内部调 Rust platform/pasteboard::write_text）
  onExecute: (r) => {
    if (r.data?.result) copyAndShow(r.data.result)
  },
})
```

### 6.3 config.ts 模板（defineConfig 形态）

```ts
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('base64', {
  enabled: true,
  maxHistory: 100,
  defaultAction: 'encode' as 'encode' | 'decode',
})
```

### 6.4 sync-extensions.ts 简化后骨架

```ts
// scripts/sync-extensions.ts（< 50 行）
import { readdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs'

const EXT_DIR = new URL('../extensions/', import.meta.url)

function scanInitFunctions() {
  const extensions = []
  for (const dir of readdirSync(EXT_DIR)) {
    const modPath = new URL(`${dir}/native/mod.rs`, EXT_DIR)
    if (!existsSync(modPath)) continue // 纯 TS 扩展无 native
    const src = readFileSync(modPath, 'utf8')
    if (/pub fn init\(\)/.test(src)) {
      extensions.push(dir)
    }
  }
  return extensions
}

function buildExtensionsRs(exts: string[]) {
  const pluginChain = exts
    .map((e) => `.plugin(crate::extensions::${e.replace(/-/g, '_')}::init())`)
    .join('\n')
  const modDecls = exts
    .map((e) => `#[path = "../../extensions/${e}/native/mod.rs"]\npub mod ${e.replace(/-/g, '_')};`)
    .join('\n')
  return `// AUTO-GENERATED by sync-extensions.ts. DO NOT EDIT.
// 仅含扩展 .plugin() 链 + mod 声明；零 generate_handler!。
// sync-extensions 完全不扫 #[tauri::command]：扩展命令在各 init() 局部注册；
// 框架命令（permission/shortcut/window）在 lib.rs 手写 generate_handler!（见 §2.8）。
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

## 7. 结构性风险

- **agent engine 第二消费者出现需再提取**：YAGNI，出现再提取；engine/ 模块边界保持清晰。`runtime/llm` 按**消费者计数**判定归属（§1.1）：parser（client 消费）、client、types 留框架层；security 溶解入 client；trim_conversation/secret_scrub 下沉 agent engine。
- **命令类型手写失去自动同步**：`check:commands` 抓命令名漂移 + tsc 严格 + 人工 checklist；**参数签名漂移无法自动抓**，靠 PR review 强制核对（§2.8）。
- **pre-bootstrap 与扩展 setup 边界**：框架级共享资源初始化（`init_ax_timeout` 等）必须在 lib.rs pre-bootstrap 串行执行，禁止下沉扩展 setup（并行 bootstrap 无法保证时序）。当前唯一项是 AX timeout；新增共享资源初始化时先判明归属层（§2.1）。
- **setup 并行竞态**：bootstrap 改 join_all 后，A.setup 不应依赖 B.setup 的产物——setup 内禁跨扩展调用 + 禁框架级共享资源初始化（当前零跨扩展依赖）。
- **config 越界绕过**：agent 安全项 floor/cap **权威定义在 Rust const**（`native/policy.rs`），agent_run 入口强制 clamp；TS `BOUNDS` 仅 UI 镜像、不持久化、不被 Rust 信任；forbidden 取并集不可放宽（§3.4「权威源」）。
- **Markdown XSS**：agent 输出是 LLM 生成内容，marked 默认不转义 HTML；`extensions/agent/View.vue` 配 DOMPurify（保持内联，单消费者不抽共享组件）。**白名单**：允许 p/h1-6/ul/ol/li/code/pre/em/strong/a/img/blockquote/table/br/hr；禁用 script/iframe/form/object/embed。**强制**：a 标签 `target=_blank rel=noopener noreferrer`，img 由 DOMPurify ≥3.0 默认移除 onerror。prompt injection 注入 `<img onerror>` 即 RCE，不可含糊。
- **icon 实时提取性能**：NSWorkspace.icon 系统级缓存，实测 <1ms；可选启动后并行预热常用应用。
- **screenshot 窗口配置下沉时序**：lib.rs:91-108 禁阴影发生在所有扩展 setup 之后；screenshot setup.rs:10-41 也操作同窗口——下沉时需合并到一处（setup 内），避免时序倒置导致配置丢失。
- **search 扩展统一化**：当前走 plugin init().setup 绕过 registry bootstrap，改 Extension trait 后需确保 init_app_watcher + prewarm_cache 在 setup 内正确启动（时序与原 plugin.setup 一致）。迁移时 frequency/recency 加权填 `boost`（见 §4），`APP_BOOST`/`(50-i)` 删除。
- **PREV_FRONT_PID 静默重复**：`window_snap.rs:67` + `session.rs:330` 重复定义 static，与 platform/focus 唯一源并存，潜在不一致——需统一走 platform/focus。
- **clipboard monitor/commands 完整迁移**：commands.rs 写入路径完全直访 NSPasteboard、monitor.rs file/PNG 读取仍直访——迁移前需确保 platform/pasteboard 的 `write_text`/`read_image`/`write_image` 已补齐（当前不存在）。
- **invoke 型 abort 不取消 Rust 端工作**：search 扩展的 mDFind 文件搜索等长耗时命令，前端 abort 只丢弃结果，Rust 端子进程仍跑完。快速连续输入会临时并行多个子进程。当前可接受（mDFind 自身有系统级调度）；若未来出现真正不可接受的长耗时命令，需 Rust 端自行设计取消机制（命令参数传 session id + cancel registry）。
- **boost 滥用（v1.5 A1）**：扩展可填 boost 刷组内序。可接受——GROUP_ORDER 锁组间序，boost 仅影响组内；且 boost 是显式字段，易审计。无消费者时框架不预设上限（YAGNI），出现刷分乱象再加 clamp。
- **并行 bootstrap 的 `block_on` 嵌套（v1.6 N7）**：§2.1 在 setup 同步闭包内 `tauri::async_runtime::block_on(join_all)`。Tauri 文档称 block_on 可在 sync 上下文调用，但 setup 闭包是否在 tokio worker 内未证实——若是则 `block_on` 嵌套 panic。阶段 2 步骤 7 落地前须先写探针（setup 内 block_on 一个空 async 验证不 panic），作为硬前置。
- **defineConfig 异步加载竞态（v1.6 N10）**：`load()` 异步，扩展 setup/早期命令可能读 defaults。安全参数 Rust clamp 兜底，UI 可能短暂显示 defaults——可接受，已文档化（§2.4）。

---

## 8. 完成后维护

- 新增 native 扩展：`extensions/<id>/native/mod.rs` 加 `pub fn init()` + Extension trait 实现；运行 `bun run sync:extensions` 自动注册
- 新增纯 TS 扩展：`extensions/<id>/index.ts` 加 `defineExtension({...})`；自动被 `import.meta.glob` 扫描
- 新增配置项：扩展 `config.ts` 加字段（defineConfig 形态）；各扩展 Settings.vue 自管渲染
- 新增能力槽：扩展 `defineExtension({...})` 内直接声明（9 个已有槽之一）；**出现真实跨扩展需求时再加 contributes 全局聚合扩展点**（当前零消费者，不预设）

---

**本蓝本描述目标终态。实现进度见 [STATUS.md](./STATUS.md)。**
