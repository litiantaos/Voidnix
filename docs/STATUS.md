# Voidnix v2 重构进度

> 实现状态追踪，高频变动。目标设计见 [REFACTOR-V2.md](./REFACTOR-V2.md)。章节引用以 `RV §x` 标注。

分支：`refactor/v2`

## 状态标记

- ✅ 已落地且符合目标设计
- 🟡 已落地但为过渡方案（目标形态见 RV 对应章节）
- ⬜ 未落地（目标形态见 RV 对应章节）

## 阶段进度

### 阶段 0：创建分支 + 提交蓝本 ✅

### 阶段 1：Rust 后端重建 ✅

- ✅ core/infra/macos → runtime/platform/http 纯净三层（旧目录已全删）
- ✅ sse.rs 539 行 → runtime/llm/{types,client,parser}（security 溶解入 client，见阶段 2 步骤 8）
- ✅ 3 套 CGEvent → 1 套 platform/input（统一 post_key/post_combo）
- ✅ NSPasteboard 全部走 platform/pasteboard（含 write_text/clear/set_string/set_file_url/set_png/set_custom 原语全集，见阶段 2 步骤 16）
- ✅ PREV_FRONT_PID 唯一源 platform/focus（window_snap/session 重复已删，见阶段 2 步骤 11）
- ✅ icon 缓存 400 PNG → 0（实时提取，磁盘零文件）
- ✅ agent engine 下沉 `extensions/agent/native/engine/`（6 文件齐全）
- ✅ path_guard policy 化（Interactive/Automated，见阶段 2 步骤 10）

### 阶段 2：Rust 扩展迁移 ✅

- ✅ Tier2 沙箱完全删除（Rust ext\_\* + 前端 worker-sandbox/declarative）
- ✅ finder-ext 横向依赖 screenshot 消除（全仓零跨扩展 import）
- ✅ awake binary 安全修复（/tmp → app_data_dir）
- ✅ clipboard 第三套 CGEvent 消灭
- ✅ zsh-as 目录名统一（注：内部仍用 `_zsh_as_*` / `ZSH_AS_BIN` 命名 100+ 处，待同步）
- ✅ SELECTED_TEXT 下沉 translate（框架层零引用）
- ✅ search icon 磁盘 cache 完全删除（仅内存 CachedApp.icon_cache，实时 NSWorkspace.icon 提取）
- ✅ core/agent/ 全目录下沉 `extensions/agent/native/engine/`（6 文件齐全）
- ✅ agent DEFAULT_SYSTEM_PROMPT/MAX_TURNS 已 config 化（policy.rs 权威源，§3.4）
- ✅ specta 依赖已删；bindings.ts 待阶段 4 删除（随 commands.ts 替换）
- ✅ agent 9 层防御双层安全（policy.rs floor/cap + BOUNDS UI 镜像 + agent_run 入口 clamp）
- ✅ screenshot temp 走 TempHandle（ocr×2/scroll 单函数 + pin 窗口注册表；picker.jpg 为复用 scratch 保留）
- ✅ TempHandle RAII struct（Drop 自动清理 + 全局注册表 + cleanup_all_temps）
- ✅ finder-ext IPC「处理完即删 + 启动清空」
- ✅ lib.rs 71 行（setup 闭包零专属配置；13+1 框架命令 + 9 扩展 registry）
- ✅ runtime/registry.rs 并行 bootstrap（async setup + join_all + block_on 探针）
- ✅ search 统一 Extension trait（纳入 registry bootstrap）
- ✅ 命令注册边界清晰：扩展命令在各 init() 局部注册、框架 14 命令在 lib.rs 手写、extensions.rs 零 generate_handler!
- ✅ 扩展结构体统一命名 XxxExtension
- ✅ tauri-plugin-clipboard-manager 依赖删除（前端 utils 迁 platform/pasteboard::write_text）；tauri-plugin-store 保留

### 阶段 3：前端运行时重建 ✅

`src/core/` 已删除，框架消费者全量重连 `src/runtime/`（stores/app、App.vue、MainView、ContentView、StatusBar、useSearchCommand、main.ts）。

- ✅ `src/runtime/` 5 文件齐备
  - ✅ `types.ts`：Extension/SearchProvider/SearchResult（9 能力槽 + 3 承载字段 disableSearchInput/listOptions/onOpenSubview）
  - ✅ `constants.ts`：SEARCH.WEIGHTS/GROUP_ORDER/GROUP_TITLES/KEYWORD_MODULE_BOOST + LIMITS（单一源）
  - ✅ `storage.ts`：defineConfig + store 实例缓存（B1）；删死代码 defineExtensionConfig/ConfigField/ConfigSchema
  - ✅ `extension-registry.ts`：defineExtension + getAllExtensions + getExtension
  - ✅ `search-engine.ts`：dynamic 单通道 + keyword 合流 + dedupe + groupAndSort（finalScore = fuzzy + boost；模块模式 bypass 保留扩展序，全局模式零分过滤）
- ✅ `utils/fuzzy.ts` 权重读 constants（WEIGHTS.prefix/contains/pinyinBase/decay/logBase/logMul/cap）
- ✅ `MainView.vue` GROUP_TITLES 读 constants（单一源）
- ✅ settings.ts 586 → 181 行（扩展配置下沉 defineConfig）
- ✅ `src/core/` 已删除（module-registry/module-helpers/async-view + module-registry.test）
- ✅ `src/types/module.ts` 已删除（SearchResult 统一到 runtime/types）
- ⬜ composables 拆分（useAppLifecycle/useSearchInput/useResultNavigation）— 纯重构延后（零行为变更）
- 🟡 `stores/settings.ts` 保留（181 行，框架级快捷键+AI Provider）

### 阶段 4：扩展同构化 ✅

- ✅ 4 个 .vnext 重建为纯 TS（base64/time/uuid/currency）
- ✅ ip 从 native 转纯 TS（删 46 行 Rust）
- ✅ 16/16 扩展 `registerModule` → `defineExtension`（9 能力槽，一次性迁移无 adapter）
- ✅ kind 合并 `web-search/open-url → web`；组间序 `application → file → module → clipboard → web`
- ✅ search 两 hidden 模块合并为单 dynamic（frequency/recency 加权填 `boost`）
- ✅ clipboard `onActivate` 移入 View 的 `onActivated`（KeepAlive）
- ✅ 扩展配置保持 defineConfig 形态；agent `BOUNDS` const（UI 镜像）
- ✅ `bindings.ts` → `commands.ts` 替换（见阶段 5）；`toSearchResults` 删除

#### 本次迁移决策（透明记录）

1. **ContentView 双模式保留**：未做成蓝本理想的"纯渲染器"，但已切到 `extension.search.dynamic` 接口 + mainView 跳过框架搜索——降低风险、行为一致。纯渲染器理想态留待 composables 拆分时一并完成。
2. **search-engine 补强**：模块模式 bypass groupAndSort（保留扩展返回序，如 clipboard 时间序），全局模式过滤零分（避免 calculator history 等无关项污染）——与旧 module-registry 行为对齐。
3. **settings 跨扩展 settingsView 扫描延后**：保留 subviews 式设置入口（clipboard/translate/agent 的 `subviews{settings}`），当前 UX 不变；`settingsView` 槽已定义在 types.ts 但未消费。SettingsView 自过滤（去 filteredItems 注入链）。
4. **translate 删除 vestigial onModuleSearch/onExecute**：View 自管 `translateText` 流式结果，标准列表从不展示 translate 结果，故旧 onModuleSearch/onExecute 为死代码。
5. **composables 拆分延后**：`useAppLifecycle`/`useSearchInput`/`useResultNavigation` 为纯重构零行为变更，待系统稳定后做。

### 阶段 5：测试 + 工具链 + 文档 🟡

- ✅ check:extensions + typecheck script 补齐
- ✅ vitest 纳入 extensions
- ✅ 6 死依赖删除（@wdio/\* + webdriverio + ts-node）
- ✅ `.gitignore` 补全 test-results/ / proptest-regressions/ / .DS_Store
- ✅ `src-tauri/tests/` 空目录删除
- ✅ **`check:commands` CI 已实现**（`scripts/check-commands.ts`：Rust `#[tauri::command]` ↔ `src/commands.ts` 双向差集；已抓到 `translate_ai_stream` 漂移 + `#[allow]` 属性隔断边界）
- ✅ `bindings.ts` → `commands.ts` 替换完成（69 命令常量，43 处裸 invoke 全迁移，零裸 `invoke('xxx')`）
- ✅ 语义常量集中（GROUP_ORDER/GROUP_TITLES/LIMITS 全在 `runtime/constants.ts` 单一源）
- 🟡 **测试覆盖**：前端 165 用例（14 文件）；Rust 77 个；但 **12/16 扩展无 `*.test.ts`**（仅 base64/calculator/time/uuid 有），前端 runtime/ 新机制零测试
- ⬜ AGENTS.md 与代码对齐（本次已增量更新见下方）

### 阶段 6：验证 ⬜

## 当前验证状态

```
cargo check --lib        → 零错误零警告
cargo test --lib         → 93 passed
bun run typecheck        → 零错误
bun run test             → 165 passed (14 files)
bun run check:commands   → 69 commands in sync
bun run check:extensions → 9 extensions, check passed
bun run lint             → 零错误
```

## 未达标项速查

**阶段 1/2/3/4 已全部 ✅**。剩余集中在测试/工具链收尾（阶段 5）与验证（阶段 6）。

**阶段 5 测试/工具链**

- 12/16 扩展无 `*.test.ts`（仅 base64/calculator/time/uuid 有）
- 前端 runtime/ 新机制（search-engine/extension-registry）零测试
- `check:extensions` 未增 windowViews 漂移校验（v1.5 A4）
- composables 拆分（useAppLifecycle/useSearchInput/useResultNavigation）— 纯重构延后

**已知偏差（记录待议）**

- `lib.rs` 71 行（目标 <50）：setup 闭包已零专属配置，剩余为 14 框架命令 generate_handler + 9 扩展 registry 注册（均框架自管、不可压缩）。实质目标达成。
- `scripts/sync-extensions.ts` 66 行（目标 <50）：含 `--check` 模式（CI 漂移校验，必要）。
- screenshot `ffi.rs:129` picker.jpg：固定路径复用 scratch（每次覆写、不累积），不适配 TempHandle Drop 语义，保留；启动 sweep 不覆盖（前缀不符），单文件残留可接受。
- platform/pasteboard 未实现 read_image/write_image（NSImage 级）：零消费者（clipboard 用 PNG 字节级 read_png/set_png），按 YAGNI 跳过。
- `utils/clipboard.ts` 仍含 useAppStore 依赖：copyAndShow/copyAndHide 调用 showStatus 反馈，解耦 ripple 大，保留。
- ContentView 未做蓝本理想的"纯渲染器"：保留双模式但已切 dynamic 接口（见阶段 4 决策 #1）。
- settings 跨扩展 settingsView 扫描延后：保留 subviews 式设置入口（见阶段 4 决策 #3）。

## AGENTS.md 同步状态

阶段 1/2/3/4 的 AGENTS.md 描述已全部增量对齐：命令注册边界、registry 并行、pasteboard 统一、policy.rs、TempHandle RAII、LLM 溶解、前端 `src/runtime/` 5 文件、`defineExtension` 能力槽、SearchEngine、`commands.ts` + `check:commands` CI。剩余待同步项（composables 拆分、settingsView 扫描）随对应阶段落地再更。

## 阶段实施步骤

### 阶段 1：Rust 后端重建

1. 创建 `runtime/` + `platform/` + `http.rs` 目录骨架与 mod.rs
2. 移动 `infra/sse.rs` → `runtime/llm/`（拆 4 子模块：client/types/security/parser）
3. 合并 `core/agent/secret_scrub.rs` + `infra/sse.rs` 安全部分 → `runtime/llm/security.rs`
4. ~~创建 `runtime/constants.rs`~~（**删除此项**：Rust 端零消费者，搜索常量仅前端；LLM 常量随 security 溶解并入 client.rs，见 RV §3.1）
5. 创建 `runtime/storage.rs`（仅 TempHandle RAII 注册表，含 Drop；**不设 StorageHandle**——settings.json/config.json 全前端管，Rust 零消费者，见 RV §2.7）
6. 创建 `runtime/registry.rs`（Extension trait + 并行 bootstrap，不引入 Phase/SetupContext）
7. 创建 `runtime/window.rs`（主窗口 show/hide/move + panel 转换/圆角配置；**保留 `set_main_window_size` / `pick_directory` / `get_home_dir` 作为 window::\* 3 框架命令**——多扩展消费，不删除。RV §1.1/§2.8 已澄清；v1.1 勘误作废「删 size/pick_dir/get_home」）
8. 创建 `runtime/shortcut.rs`（快捷键 + 录制，删 SELECTED_TEXT/PREV_FRONT_PID 泄漏）
9. 创建 `runtime/permission.rs`（薄壳，合并原 core/permission + 部分 macos/permission）
10. 创建 `platform/focus.rs`（统一 PREV_FRONT_PID 唯一源，合并 mac_utils）
11. 创建 `platform/input.rs`（统一 post_key/inject_copy/simulate_cmd_v 三套）
12. 创建 `platform/pasteboard.rs`（无状态原语全集 + snapshot/restore 不可变快照：read_text/write_text/read_image/write_image/string_for_type/data_for_type/has_type/change_count/snapshot/restore；snapshot/restore **留 platform**，不上移 runtime，见 RV §2.6）
13. 创建 `platform/path_guard.rs`（统一路径校验，合并 finder-ext + agent）
14. 移动 `macos/{panel,skylight,click_monitor,permission}.rs` → `platform/`
15. 删除 `core/` + `infra/` + `macos/` 旧目录
16. 重写 `lib.rs`（精简到 ~30 行，仅框架自管 + ExtensionRegistry::bootstrap）
17. 保持 `build.rs` 显式编译（每个 .mm 参数不同，不扫描化：YAGNI）
18. 重写 `scripts/sync-extensions.ts`（< 50 行，仅扫 init() + 生成 .plugin() 链，依赖阶段 2 步骤 1-5 命令注册下沉）
19. 更新 `Cargo.toml`（删 specta；**仅删 `tauri-plugin-clipboard-manager`**——前置：阶段 2 步骤 16 前端 utils 迁移完成。`tauri-plugin-store` 保留：持久化后端，非冗余，见 RV §2.7 v1.2）
20. `cargo check` 通过

### 阶段 2：Rust 扩展迁移

_命令注册下沉（sync-extensions 简化的前置，见 RV §2.8）_

> ⚠️ **原子化约束（v1.6 N9）**：步骤 1-5 须**单次提交完成**（9 扩展 init() + 清空 extensions.rs 顶层 generate_handler! + 13 框架命令迁 lib.rs + 重写 extensions.rs/sync-extensions.ts），**不留中间可编译态**——分扩展逐个迁移会触发 Tauri 重复注册 panic（同一命令既在顶层又在 init() 内）。

1. 所有 9 个 native 扩展 `mod.rs` 改 `pub fn init() -> TauriPlugin<Wry>` 骨架（Builder + `.setup`），与现有 `Extension` trait 实现并存（双 setup 职责判据见 RV §2.8）
2. 各扩展 `#[tauri::command]` 函数收进 `init()` 的 `invoke_handler(generate_handler![...])` 局部注册（当前 0/9 有局部 invoke_handler）
3. 13 个框架命令（permission/shortcut/window）从 `extensions.rs` 迁到 `lib.rs` 手写 `generate_handler!`；`extensions.rs` 顶层 `generate_handler!` 彻底清空
4. 重写 `scripts/sync-extensions.ts` < 50 行：仅扫 `pub fn init()` 签名（无 COMMAND_REGEX）
5. 重写 `src-tauri/src/extensions.rs`：仅 `.plugin()` 链 + mod 声明

_registry 升级（RV §2.1）_

6. `runtime/registry.rs`：删除未用的 `deps()` 字段；不引入 Phase/SetupContext；`on_setup`/`on_teardown` 改名 `setup`/`teardown`；teardown 改并行（v1.6 N5）
7. bootstrap 改 `futures::future::join_all` 并行（达成冷启动 <100ms）。**两项硬前置（v1.6 N7/N8，缺一不可）**：
   - **步骤 7.0a 串行 baseline 埋点**：lib.rs 启动埋点测串行 bootstrap 耗时，量化并行收益 + 判断 <100ms 可达性（§0.2）
   - **步骤 7.0b `block_on` 探针**：setup 同步闭包内先 `tauri::async_runtime::block_on` 一个空 async，确认不 panic（若 setup 闭包在 tokio worker 内则 block_on 嵌套 panic，需改方案）

_框架层业务残留清理_

8. `runtime/llm/security.rs` 全量溶解：`validate_endpoint`/`validate_ai_request`/SSRF 黑名单 + `MAX_SSE_BUFFER`/`MAX_MESSAGE_CONTENT_LEN` + `truncate_message` 并入 `client.rs`（请求管道校验+常量，agent+translate 2 消费者）；`trim_conversation` + `MAX_CONVERSATION_MESSAGES` 下沉 `extensions/agent/native/engine/`（agent 唯一消费者）；`parser.rs` **留 runtime/llm**（client.rs 消费）；runtime/llm = client.rs + parser.rs + types.rs
9. ~~`platform/pasteboard.rs` 的 snapshot/restore 上移 `runtime/pasteboard_tx.rs`~~（**删除此项**：snapshot/restore 留 platform，不可变快照符合原语原则，见 RV §2.6）
10. `platform/path_guard.rs` policy 化：`Policy::{Interactive, Automated}` + `validate(path, policy)`
11. 删除重复 `PREV_FRONT_PID`：`window_snap.rs:67` + `session.rs:330` 两处
12. `runtime/storage.rs`：TempHandle 改 RAII struct（含 Drop 自动清理，见 RV §2.7）
13. screenshot temp 文件改走 TempHandle 注册（ocr.rs:28,214 / scroll_capture.rs:1035 / ffi.rs:129 / pin.rs:40 / mod.rs:85 共 6 处）
14. `lib.rs` 瘦身到 <50 行，5 处专属配置下沉（仅保留框架级 `generate_handler!`）：
    - **L37-38 agent SessionRegistry+ApprovalManager** → agent setup 内 `app.manage()`
    - **L43-44 translate init_ax_timeout** → translate setup 内
    - **L46-62 main 窗口 panel 转换 + 圆角** → `runtime/window.rs::configure_main_window()`
    - **L64-89 snap-panel 窗口配置** → window-manager setup 内
    - **L91-108 screenshot+snap-panel 禁阴影** → screenshot setup.rs `configure_overlay_window` 内合并

_扩展配置化（RV §3.4）_

15. agent 9 层防御全 config 化（RV §3.4 v1.1）：新增 `native/policy.rs` 集中 floor/cap Rust const（权威源，**初始值 ⊇ 现网 `run_command.rs` FORBIDDEN_PROGRAMS 31 项 / DENIED_ARG_PREFIXES 15 项 / MAX_WALL_SECS=30 / MAX_OUTPUT_BYTES=1MiB / rlimits，禁止缩窄**）；`run_command.rs` 改读 config（用户值）+ policy.rs const（底线）取 clamp/并集；`is_circuit_breaker_hit`（rm -rf 断路器）保持硬编码；trustedCommands 匹配语义=程序名（首 token）；TS 端 `BOUNDS` const 仅 UI 镜像；默认 trusted 移除 kill/ps/top 及复合条目
16. clipboard：config 读 maxDays（500ms 轮询、5000 行限制改 config 可调）；**先在 platform/pasteboard 补 write_text/read_image/write_image 3 原语**，再迁移 monitor/commands 完整走 platform；**同步迁移前端 utils**（5 文件 6 调用点：`src/utils/clipboard.ts`×2 + `extensions/translate/{index.ts,View.vue}` + `extensions/screenshot/composables/useOverlayEvents.ts` + `extensions/calculator/index.ts`，全部从 `@tauri-apps/plugin-clipboard-manager` 的 `writeText` 改为 `invoke` 调 `platform/pasteboard::write_text`）——此为步骤 19 删除 `tauri-plugin-clipboard-manager` 的前置
17. 所有扩展实现 Extension trait（id/setup/teardown，无 phase）
18. **search 扩展统一化**：改为 Extension trait 实现，纳入 registry bootstrap（确保 init_app_watcher + prewarm_cache 在 setup 内正确启动）

_收尾_

19. 删 `Cargo.toml` 的 `tauri-plugin-clipboard-manager` 依赖（**仅此一个**；`tauri-plugin-store` 保留为持久化后端，见 RV §2.7 v1.2。前置：步骤 16 前端 utils 迁移完成）
20. `cargo check` + `cargo test --lib` 通过

### 阶段 3：前端运行时重建

_runtime 5 文件（RV §1.2）_

1. `src/runtime/types.ts`（Extension/SearchProvider 类型，9 能力槽）
2. `src/runtime/constants.ts`（语义常量**单一源，仅前端**；Rust 端无常量文件，见 RV §3.1）
3. `src/runtime/storage.ts` 保持当前 defineConfig 形态；**删除零消费死代码**（`defineExtensionConfig` / `ConfigField` / `ConfigSchema`，storage.ts:5-22，全仓零引用，RV §2.4 已标废弃）；**store 实例缓存（v1.5 B1）**：`defineConfig` 须缓存 store 实例（模块级 `Map<extId, Store>`），禁止每次保存重新 `load()`（现网 storage.ts:54-60 需改造）；agent 安全项的 `BOUNDS` const 定义在 `extensions/agent/config.ts`（不进 storage.ts）
4. `src/runtime/extension-registry.ts`（defineExtension + getAllExtensions，无 contributes 聚合）
5. `src/runtime/search-engine.ts`（dynamic 单通道并行 + filter/group 管道，RV §2.5）。**含 kind 分类变更**（RV §2.3 v1.5）：`web-search/open-url → web` 合并；**folder/file 维持同组**（v1.5 supersede v1.1 拆分决策，folder 组内优先用 `SearchResult.boost` 表达，不再拆组）；组间顺序改为 `application → file → module → clipboard → web`（文件跃升 module 前）。`module-registry.ts::getGroupKey` **保留**（仍合并 file/folder → 'file'）、重写 `groupAndSort`（finalScore = fuzzy + boost）、E2E 覆盖相关性回归

_composables_

6. `src/composables/useAppLifecycle.ts`（抽自 App.vue）
7. 拆 `useSearchCommand.ts` → `useSearchInput.ts` + `useResultNavigation.ts`
8. ~~`utils/id.ts`（generateRequestId）~~ ✅ 已存在（`src/utils/id.ts`）

_改造旧文件（让旧代码读 constants 而非硬编码）_

9. `utils/fuzzy.ts`（权重读 constants）
10. `module-registry.ts`（GROUP*ORDER/MAX*\*/TIMEOUT 读 constants）
11. `MainView.vue`（GROUP_TITLES 读 constants）

### 阶段 4：扩展迁移（一次性全量，不留 adapter）

**策略**：阶段 3 落地 5 文件后，16 扩展一次性 `registerModule` → `defineExtension`，迁移完成后立即删 `core/` + `registerModule` + `toSearchResults` + `bindings.ts`。迁移期间系统不可编译（~8 小时窗口），在 refactor/v2 分支单次提交完成——不留 adapter 过渡层（符合 RV §0.2 一步到位，不考虑历史包袱）。

步骤（按依赖顺序）：

_纯 TS 扩展先行（7 个）_

1. base64：defineExtension（search dynamic + 模块级缓存）；config.ts 保持 defineConfig
2. time：同上
3. uuid：同上
4. currency：同上（HTTP 走前端 `fetch`，同 ip；无统一 http 命令）
5. calculator：defineExtension；config.ts 保持 defineConfig；**history 走 plugin-store 直用**（`calc_history.json` 是数据非配置，无 schema 默认值语义，不强制迁 defineConfig，保留 `load()` 直访）；补 parser 测试
6. ip：defineExtension（search dynamic）
7. settings：改为扫描各扩展 `config.ts`，渲染各扩展 Settings.vue 子视图；searchItems 改走 dynamic 返回静态项（dynamic 内调 getAllExtensions 派生）

_含 native 扩展（9 个，前端迁移；Rust 端改造已在阶段 2 完成）_

8. clipboard：defineExtension（search dynamic + mainView + searchBarAccessory + globalShortcuts + hints）
9. screenshot：defineExtension（含 windowViews）
10. awake：defineExtension
11. zsh-autosuggestions：defineExtension
12. window-manager：defineExtension（含 windowViews + globalShortcuts）
13. finder-ext：defineExtension
14. translate：defineExtension（含 searchBarAccessory + globalShortcuts）
15. agent：defineExtension；config.ts 加 `BOUNDS` const（安全底线 UI 镜像）
16. search：defineExtension（search 改 dynamic）

_收尾（依赖全部扩展迁移完成）_

17. 删除 `src/core/` 全目录（module-registry/module-helpers/async-view）
18. composables 重整：拆分 `useSearchCommand.ts` → `useSearchInput.ts` + `useResultNavigation.ts`（`.test.ts`/`.test-utils.ts` 随之迁移）；保留 `useFloating.ts`/`useScrollPosition.ts`/`useTauriListener.ts`（RV §1.2）。**注**：`events.ts`/`useInputControl.ts`/`useShortcutConfig.ts`/`useSettingsInput.ts` 当前全部活跃（见未达标项「前端运行时」），迁移时逐项评估去留，不直接删除
19. 删除 `utils/tauri.ts::toSearchResults`（search 扩展迁移后无消费者，含 index.ts 2 处调用点）
20. 删除 `src/bindings.ts`，替换为 `src/commands.ts`（约 40 个裸 invoke + bindings 7 处活引用一并迁移）
21. `bun run typecheck` + `bun run test` 通过；`grep registerModule extensions/` 归零

### 阶段 5：测试 + 工具链 + 文档

1. 补扩展单元测试（当前 4/16 有）：clipboard/agent/search/translate/screenshot/window-manager/finder-ext/awake/zsh-autosuggestions 核心逻辑；base64/time/uuid/currency 补边界
2. 补前端运行时测试（当前 0 个）：search-engine（dynamic 并行 + abort cleanup + filter/group 管道）、extension-registry（defineExtension）、storage（defineConfig 持久化 + debounce）；**abort cleanup 测试按资源型分流（v1.5 B4）**：仅持有非自动释放资源（事件订阅/子进程/手动连接池）的 provider 须补 cleanup 用例；纯 fetch+signal 透传型（currency/ip）随 abort 自动释放，**免额外 cleanup 测试**（invoke 型本就无需）
3. 删除 `tauri-plugin-clipboard-manager` 依赖（依赖阶段 2 步骤 19；`tauri-plugin-store` 保留为持久化后端）
4. **新增 `check:commands` CI（阻塞项）**：扫描 Rust `#[tauri::command]` 名集合与前端 `src/commands.ts` 常量集合作差集比对；**`check:extensions` 增 windowViews 漂移校验（v1.5 A4）**：声明 windowViews 槽的扩展，其每个 key 必须在 `tauri.conf.json` `windows[].label` 中存在
5. AGENTS.md 增量更新（每阶段完成同步）
6. 删除 `docs/tier1-extensions.md` + `tier2-extensions.md`，统一 `docs/extensions.md`
7. 更新 `docs/extensions/*.md`
8. `bun run lint` 通过

### 阶段 6：验证

1. `bun run test`（前端单元）
2. `cd src-tauri && cargo test --lib`（Rust 单元）
3. `bun run test:e2e`（E2E）——含相关性回归专项断言（v1.5 B3）：搜索结果组间序（application→file→module→clipboard→web）+ kind 归属（web 合并、folder/file 同组）
4. `bun run lint`（Prettier + ESLint）
5. `bun run tauri:dev` 启动，逐扩展手测
6. 性能验证：冷启动 <100ms（lib.rs 启动埋点日志）、常驻内存 <50MB、icon 零磁盘文件、finder-ext commands 目录无累积

## 验收清单

对照 RV 目标设计。

### Rust 后端

- ✅ `src-tauri/src/{core,infra,macos}/` 目录不存在
- ✅ 全仓零重复 `NSPasteboard` 直访 — clipboard monitor/commands 完整走 platform/pasteboard
- ✅ 全仓零重复 `PREV_FRONT_PID` — platform/focus 唯一源（pin 的 PIN_PREV_PID 为独立生命周期保留）
- ✅ 全仓零 `SELECTED_TEXT`（框架层）、零重复 CGEvent 实现
- ✅ icon 零磁盘文件（实时提取）
- ✅ `lib.rs` 71 行、setup 闭包零专属配置（仅框架级 generate_handler + pre-bootstrap + configure_main_window）
- ✅ `runtime/llm/` = client.rs + parser.rs + types.rs（security 溶解入 client；trim_conversation 下沉 agent engine）
- ✅ `runtime/registry.rs` Extension trait 并行 bootstrap（`join_all`）+ async setup/teardown
- ✅ `tauri-plugin-clipboard-manager` 依赖删除
- ✅ `tauri-plugin-store` 保留（defineConfig + settings.json 持久化后端，非冗余，见 RV §2.7 v1.2）
- ✅ `path_guard` policy 化（Interactive/Automated）
- ✅ `platform/pasteboard` 补齐 write_text/clear/set_string/set_file_url/set_png/set_custom（snapshot/restore 留 platform；read_image/write_image 零消费者跳过）
- ✅ `TempHandle` RAII struct（Drop 自动清理）
- ✅ clipboard monitor/commands 完整迁移 platform/pasteboard
- ✅ search 扩展统一 Extension trait
- ✅ 命令注册边界清晰：扩展命令在各 `init()` 局部注册、框架 14 命令在 `lib.rs` 手写、`extensions.rs` 零 `generate_handler!`
- ✅ `build.rs` 保持显式编译（不扫描化）

### 前端

- ✅ `src/core/` 不存在（module-registry/module-helpers/async-view 全删）
- ✅ `src/runtime/` 5 文件齐备（types/constants/storage/extension-registry/search-engine）
- ✅ 16 扩展用 `defineExtension`（9 能力槽，一次性迁移无 adapter）
- ⬜ composables 拆分（useAppLifecycle/useSearchInput/useResultNavigation）— 纯重构延后
- ✅ `sync-extensions.ts` 66 行、无 COMMAND_REGEX（含 --check 模式，必要）
- ✅ 零硬编码可调参数（GROUP_ORDER/GROUP_TITLES/LIMITS 集中 `runtime/constants.ts`）
- ✅ `bindings.ts` 替换为 `commands.ts`，零裸 `invoke('xxx')`（69 命令常量，check:commands CI 守护）

### 扩展

- ✅ 每扩展 `index.ts` 用 `defineExtension`（9 能力槽，无 contributes）
- ✅ 每扩展 `config.ts` 保持 `defineConfig` 形态 — 8/9 native 扩展配置自管（search 无 config.ts）
- ⬜ 每扩展 `*.test.ts` 存在 — 仅 base64/calculator/time/uuid 4 个
- ✅ agent 9 层防御双层安全（`BOUNDS` const floor/cap + forbidden 并集，policy.rs 权威源）
- ✅ screenshot temp 走 TempHandle 注册

### 工具链与文档

- ✅ `check:commands` CI（命令名漂移检测，阻塞项）— 69 命令 in sync
- ⬜ `check:extensions` 增 windowViews 漂移校验（v1.5 A4）— 未实现
- 🟴 AGENTS.md 与代码对齐 — 本次已增量更新（defineExtension/commands.ts/runtime 消费者）

### 性能

- ⬜ 冷启动 <100ms、常驻内存 <50MB（bootstrap 并行化是关键；测量法：lib.rs 启动埋点日志）
- ✅ finder-ext commands 零累积（「处理完即删+启动清空」已实现）
