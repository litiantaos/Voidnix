# 扩展开发

所有扩展同构，目录结构统一。是否含 `native/` 子目录区分实现方式（Rust 端 vs 纯 TS），不构成分类。全局架构见 [AGENTS.md](../AGENTS.md)。

## 目录结构

```
extensions/<id>/
├── index.ts               # 前端注册（export default defineExtension({...})）
├── config.ts              # defineConfig 自管配置（可选）
├── View.vue               # 主视图（若声明 mainView）
├── Settings.vue           # 配置子视图（若声明 subviews.config）
├── Actions.vue            # 搜索栏配件（若声明 searchBarAccessory，命名约定 Actions 后缀）
├── logic.ts               # 纯逻辑提取（可选，便于测试）
├── *.test.ts              # 测试（co-location）
└── native/                # Rust 端（仅需要系统级能力时存在）
    ├── mod.rs             # Extension trait 实现（setup 生命周期 + 命令）
    └── ...                # 子模块（commands.rs / engine/ 等）
```

23 个扩展：含 native/ 的 16 个（clipboard、screenshot、video、awake、clean-mode、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search、proxy、system-status、ai-providers、image、homebrew），纯 TS 的 7 个（calculator、settings、ip、base64、time、uuid、currency）。

## 前端注册

`index.ts` 顶层 `export default defineExtension({...})`，由 `main.ts` 的 `import.meta.glob(['@ext/*/index.ts'], { eager: true })` 自动扫描注册。扩展 `setup()` 钩子在 Vue 挂载后由 `main.ts` 并行触发。

```typescript
import { defineExtension } from '@/runtime/extension-registry'

export default defineExtension({
  meta: { id: 'base64', name: 'Base64', icon: 'i-ri-code-s-slash-line', order: 100, keywords: ['编码'] },
  placeholder: '输入文本编解码 Base64',
  search: { dynamic: (query, ctx) => [...] },
  onExecute: (result) => { ... },
  mainView: () => View,
})
```

### 能力槽（按需声明，均有真实消费者）

- `search`：SearchProvider.dynamic 单通道召回（消费者见下「搜索集成」）
- `onExecute`：搜索结果回车动作，扩展私有（无消费者）
- `mainView`：主视图组件（16 扩展）
- `searchBarAccessory`：搜索栏右侧配件（6：clipboard/agent/translate/proxy/ai-providers/image）
- `subviews`：扩展私有命名子视图（6：screenshot{ocr}、clipboard{config}、agent{config}、translate{config}、proxy{connections/rules/logs}、homebrew{detail}）
- `subviewTitle`：子视图显示名（id→中文名），激活子视图时搜索栏 placeholder 用「搜索{name}」（2：proxy、homebrew）
- `windowViews`：独立窗口视图，key 须存在于 `tauri.conf.json` `windows[].label`，`-`/`*` 结尾为动态前缀（2：screenshot/window-manager）
- `globalShortcuts`：全局快捷键绑定（5：clipboard/screenshot/agent/translate/finder-ext）
- `placeholder`：搜索框占位提示，激活扩展时显示（6：clipboard/currency/ip/time/base64/calculator）
- `windowHeight`：扩展激活时主窗口高度，三种声明语义：
  - **`number`**：固定高度，clamp `[MIN,MAX]`
  - **`'auto'`**：随内容自适应
  - **未声明**：默认高度
  - 共 7 消费者：agent/proxy=840、translate/system-status/video/finder-ext/image='auto'
- `subviewHeights`：subview 级高度覆盖，key→语义同 windowHeight（1：screenshot{ocr:'auto'}）

**高度机制**：统一由 `useExtensionHeight`（MainView 全局唯一调用）处理，扩展只需声明，View 不用管。

- **动画**：高度变化一次 IPC 触发 Rust → `platform/window.rs::animate_frame` 用 macOS `NSAnimationContext` + `animator setFrame:display:animate:` 系统级动画（CoreAnimation 接管，非 JS 逐帧）
- **auto 模式**：ResizeObserver 监听内容根，窗口高 = chrome + 内容高，clamp `[DEFAULT_HEIGHT, 屏幕高 90%]`，底部将出屏则上移，离开 auto 还原原位

生命周期：`setup?()`（启动钩子，无参）。3 行为槽：`disableSearchInput`（扩展自管输入，禁用主搜索框）、`listOptions.multiSelect`（标准列表多选）、`onOpenSubview`（子视图打开回调，如 OCR payload 转交）。三者与能力槽同等地位（见 `runtime/types.ts`）。

### 跨扩展通信

禁止扩展之间直 import 内部状态（如 `import { x } from '@ext/other'`）。需要跨扩展投递数据时走 Tauri 事件总线：发送方 `emit('ext-<event>', payload)`，接收方在 `setup()` 内 `listen('ext-<event>', ...)`。约定事件名前缀以目标扩展 id 开头（如 `translate-pending-text`），避免冲突。screenshot OCR → translate 待翻译文本即此模式。

### 菜单栏贡献（Rust 侧）

框架唯一菜单栏托盘图标（`runtime/menubar.rs`，`public/bar_icon.png` 模板图），左键弹聚合菜单。含 native/ 的扩展在 Rust `setup` 内 `menubar::register(MenuBarContribution)` 声明贡献段：

- `title: &'static str`：分组标题（disabled 项渲染，如「保持唤醒」/「代理」）。菜单按 `title` 分组——每段贡献前插标题项，段间分隔线。
- `build: Arc<dyn Fn(&AppHandle) -> Vec<MenuEntry>>`：返回当前菜单快照。空 `Vec` = 该扩展当前不贡献（不参与菜单、不影响图标可见性）。
- `on_event: Arc<dyn Fn(&AppHandle, &str)>`：收到所有点击的 item id，扩展自行过滤归属项（约定 id 以扩展 id 为前缀避免碰撞，如 `proxy_toggle`）。

`MenuEntry` 四态：`Item{id,label,enabled}` / `CheckItem{id,label,checked}` / `Submenu{label,items}` / `Separator`。状态变更后调 `menubar::refresh(&app)` 触发重建。**图标可见性 = Σ 各段 `build()` 项数 > 0**（扩展全关则图标自动隐藏）。与快捷键 hook 同范式（`LazyLock<Mutex<Vec>>` + free function）。现 2 消费者：awake（保持系统唤醒：打开扩展 + 启用开关 + 显示模式二级菜单）、proxy（代理：打开扩展 + 已连接状态 CheckItem 可点断开「已连接：节点」；断开后图标隐藏，重连走扩展面板，其余控制全部在面板）。

### UI 规约补充

- **`order` 唯一性**：扩展 `meta.order` 在非 hidden 扩展间应唯一，避免扩展列表稳定排序抖动。当前分配：clipboard=10 / translate=20 / agent=30 / ai-providers=35 / proxy=40 / time=50 / ip=60 / uuid=70 / base64=80 / calculator=90 / currency=100 / screenshot=110 / video=115 / image=116 / window-manager=120 / finder-ext=130 / system-status=135 / zsh-autosuggestions=140 / clean-mode=150 / awake=160 / homebrew=170；hidden 扩展 settings=998 / search=999。
- **`disableSearchInput` 决策**：与 `mainView` 独立——mainView 扩展若仍用主搜索框过滤列表（如 clipboard）则不声明；自管输入或无需搜索框（agent/translate/settings 等）声明 `true`。uuid 有 search 但 disableSearchInput（进入后只展示即时结果）。
- **clipboard 敏感内容过滤**：monitor 对源 app 为已知密码管理器（1Password/Bitwarden/KeePassXC 等）或内容匹配 secret 启发规则（`password=`/长 base64/PEM 等）的文本不入库，避免明文密码落 SQLite。ConcealedType marker 是第一道防线，此为兜底。
- **View 根禁止与 ContentView 竞争的纵向双滚**：经 ContentView 渲染的 View（mainView/subviews）根及主内容流不得设 `overflow-y-auto`/`overflow-auto`。ContentView 的 `scrollContainer` 是页面级唯一滚动容器——View 根再设 overflow 会形成双层滚动，`BaseList` 键盘导航的 `el.closest('.overflow-y-auto')` 命中失效内层 → 选中框出视口。固定高度媒体预览等局部区域（如 OCR 图预览）可自滚。`windowViews`（独立窗口）不经 ContentView，不受此约束。

## 搜索集成

### SearchProvider（单通道）

```typescript
interface SearchProvider {
  dynamic(query: string, ctx: SearchContext): ProviderResult[] | Promise<ProviderResult[]>
}

interface SearchContext {
  signal: AbortSignal // 新查询覆盖旧查询时 abort
  extensionMode?: boolean // true=扩展独占（进入扩展），false=全局聚合（默认列表）
  emit?: (results: ProviderResult[]) => void // 流式部分结果：扩展可多次调用先产出快结果，最后 return 补充
}
```

- **全局模式**（`searchEngine.search`）：

  流程：**流式增量召回**——并发启动所有扩展 dynamic，每个扩展的 `emit`/`resolve` 都触发一次增量重排（keyword 合流 → dedupe → groupAndSort）并回调 `onUpdate`。快结果（应用缓存/同步扩展）秒出，慢结果（mdfind 文件/网络）增量补充，不再 `Promise.all` barrier 等全部。finalScore 仍只预算一次（emit 时打分，groupAndSort 复用）。

  - **流式**：扩展可选调用 `ctx.emit(partial)` 多次产出部分结果（如 search 扩展应用 emit 秒出、文件 return 后补），不调用的扩展走一次性 return 行为不变。框架按 `extId:id` 去重，emit 与 return 重叠不会产生重复项；但扩展应遵循「emit 产出首批、return 产出补充」的语义分工——已 emit 的内容不放入 return，避免多余打分计算
  - **keyword 合流**：`scoreExtensionEntry`（name/id/description 正向 + keywords 双向，与 `/` 工具列表共用）；每次 flush 重算（纯同步、扩展数少），keyword 入口 finalScore 复用内部 score（含 keywordMatch 反向贡献）
  - **入口抑制**：dynamic 产出相关 tool 型结果（kind=extension，finalScore > 0）的扩展抑制其入口（即时答案优先）；clipboard 等数据型 kind≠extension 不抑制
  - **过滤规则**：空 query 按 `finalScore>0`；非空 query 查找型需 `fuzzy>0`，extension 类即时答案靠 `finalScore>0` 穿透

- **扩展模式**（同一 `searchEngine.search`，`setActiveExtension` 后）：

  - **召回**：只调激活扩展 dynamic，bypass groupAndSort 保留扩展返回序
  - **超时/abort**：同样受 `searchTimeoutMs` 超时与 abort 保护；每扩展独立 child `AbortSignal`（超时只 abort 该扩展，父 abort 同步取消）
  - **模式快照**：`search()` 入口快照 `activeExtension`，await 期间切换不影响本次后处理
  - **UX**：外壳（`useSearchInput`）延迟 50ms 显示 loading，同步 dynamic 不闪、网络型才占位

- `extensionMode` 区分调用场景：**全局即时答案仅 calculator / currency**；ip / time / uuid / base64 等须 `if (!ctx?.extensionMode) return []`，仅扩展内响应。网络型（currency）全局空 query 仍应跳过请求返回 `[]`，避免拖慢默认列表。
- 半静态内容（如 base64）用扩展内缓存自管，走 dynamic 返回。

### SearchResult

```typescript
{
  id: string                  // 扩展内 localId
  title: string               // 进拼音索引，框架统一打分
  extId: string               // 框架自动注入（扩展禁填）= 产出扩展 meta.id
  description?, icon?, shortcut?, boost?,
  data: { kind, extId?, path?, ... }
  score?: number              // 仅框架填，扩展禁止填
  source?: string             // 框架注入（扩展禁填）：全局模式 kind=extension 结果的来源扩展显示名
}
```

- `kind` 严格枚举：`application | folder | file | extension | clipboard | web`（folder/file 同组）。扩展须正确设置，否则分组错乱。
- `boost?`：扩展可选组内优先级提示（默认 0），`finalScore = fuzzy(title,query) + boost`。调整相关性**只能**通过 boost（score 框架独占）。
- 扩展返回 `ProviderResult`（Omit extId/source），框架注入 extId 与 source（全局模式 + kind=extension 时自动注入来源扩展显示名，UI 右侧标注）。

### 执行分派（框架内置契约）

搜索结果回车由 `data.kind` 分派：

- `data.kind === 'extension' && data.extId` → **框架内置激活**（setActiveExtension），不走 onExecute（扩展入口结果，由 keywordSearchAll 产出）
- 其余 → 扩展 `onExecute` 槽，执行后框架回全局模式 + 隐藏窗口

### 管道层次（不可破坏）

去重 → 分组 → 组内排序 → 组间定序 → 组内限流。组间序由 `constants.GROUP_ORDER` 锁死（`application → extension → file → clipboard → web`），不开放给扩展调整。扩展调整相关性的唯一通道：`data.kind` 归组（组间位）+ `boost`（组内位）。

## 扩展配置（defineConfig）

```typescript
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('extensions/clipboard/config', { maxDays: 30 })

// 响应式读写，变更自动持久化至 extensions/clipboard/config.json（300ms 防抖）
config.maxDays // → 30
config.maxDays = 60 // 自动写盘
```

- 第一参数为完整 plugin-store path（不含 `.json` 后缀），扩展用 `extensions/<id>/config`，框架级用 `config/settings`。
- backfill 类型守卫：磁盘值类型与 default 不符则丢弃；`isStillDefault` 走递归 deepEqual（顺序无关）。
- 启动期 `isLoading` 抑制 watch 冗余写；退出 `onCloseRequested` flush 防抖窗口内变更。
- 不订阅 plugin-store `onChange`：plugin-store 的 set 会向本进程回放 `store://change`（无来源标识），回灌会以旧快照覆盖 emit 到达前已 mutate 的新值（实测复现）；所有 config 仅在 main 窗口持有（子窗口纯内存 reactive），无跨窗口同步需求。
- schema 变更：自开发自用不维护迁移，改 schema 时手动删磁盘 config.json 即可。
- store 实例缓存（文件级 `Map<storePath, Store>`），watch 回调复用，禁止每次保存重新 `load()`。
- 加载异步竞态：`load()` 异步，扩展 setup 早期可能读 defaults。安全参数由 Rust clamp 兜底。
- 资源上限（agent 专属）：plain `BOUNDS` const 表达 floor/cap，**权威在 Rust `native/policy.rs`**，TS 仅 UI 镜像，详见 [agent.md](./extensions/agent.md)。
- 含 Rust 命令同步的配置按**数据位置**分两类同步规约：
  - **Config 字段型**（数值/字符串/枚举/boolean，持久化在 `config.json`）：在 `config.ts` 用 `watch(..., { immediate: true })` 同步，View.vue 仅改 config 不显式 invoke，失败仅 `console.error`。`immediate: true` 确保启动期磁盘回填后自动同步持久化值（避免「上次开启 → 重启丢失」回归）。样板：`window-manager/config.ts`（`enabled` / `customWidth` / `customHeight`）、`awake/config.ts`（`displayMode`）、`clipboard/config.ts`（`maxDays`）。
  - **Rust 状态型**（无 config 字段，状态权威在 Rust 端）：在 `View.vue` 显式 `invoke` + 错误反馈（`showStatus error`），成功才更新 UI 局部状态。样板：`awake/View.vue::toggleAwake`（子进程开关，状态查 `is_awake_enabled`）。

框架级配置（全局快捷键）在 `stores/settings.ts`，同样走 `defineConfig`（`config/settings` storePath）。**AI 提供商**（`src/runtime/ai-providers.ts`）：只存 URL/Key/模型（无「使用中」）；列表按提供商分组、**每把 Key 一行**；选用由 agent/translate 等消费者自管；自动写 `ai.env`（`VOIDNIX_ZHIPU_API_KEY` / `VOIDNIX_DEEPSEEK_API_KEY` 等私有名，外部工具须显式引用）并幂等装 shell 钩子。详见 [ai-providers.md](./extensions/ai-providers.md)。

### 配置字段命名规范

同类配置必须统一命名与参数，禁止各扩展自创风格：

- **扩展整体启用**：`enabled: boolean`（默认 `false`，需用户主动启用）
- **特定功能启用**：`<feature>Enabled: boolean`
- **枚举型**：字符串字面量联合（如 `displayMode: 'mirror' | 'extend'`），不用 boolean 伪装模式枚举
- **单对象 vs 数组**：唯一实体用单对象（`searchProvider: {...}`），多实体并发执行用数组（`configs: [...]`）；数组禁止 `isDefault` 标记或独立的 `activeXxxId` 字段——若需单选激活才加 `activeXxxId`
- **Rust 同步命令**：`set_<ext>_<field>` 模板（boolean 启用型统一 `set_X_enabled`）
- **Rust 查询命令**：`is_<ext>_enabled`（仅 Rust 状态型需要；config 字段型前端自有真理，勿加查询命令）
- **Boolean 参数**：统一 `enabled`（过去分词，形容词性），禁止 `enable`（动词原形）或领域词

数值型配置的 floor/cap 表达为 TS `BOUNDS` const + Rust const 双源，CI 强制约束（`check:agent-bounds` / `check:wm-bounds`）。

## Rust 扩展（含 native/）

### 注册机制

扩展命令与生命周期分离注册：

1. **命令**：`#[tauri::command]` 函数由 `sync-extensions` 扫描，生成 `extensions.rs` 的 `configure_app!` 宏（单一全局 `generate_handler!`），前端裸名 `invoke('cmd')` 路由。Tauri 2 插件命令需 `plugin:name|cmd` 格式，裸名只路由全局 `invoke_handler`，故扩展命令必须全局注册。
2. **生命周期**：`Extension` trait（`runtime/registry.rs`），在 `lib.rs` 的 `ExtensionRegistry` 注册，提供 `setup` 钩子（并行 bootstrap via `join_all`）。命令执行依赖的 State（DB 等）在 `setup` 内 `app.manage`。

> 扩展无需声明 `init()` / plugin 空壳——纯 `Builder::new().build()` 对运行时零贡献（不注册命令/state/setup），已消除。

### Extension trait

```rust
#[async_trait::async_trait]
impl Extension for ClipboardExtension {
    fn id(&self) -> &'static str { "clipboard" }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // 跨扩展可见副作用：快捷键钩子、窗口配置、命令执行依赖的 State（app.manage）
        Ok(())
    }
}
``

**双 setup 职责**（按副作用可见性）：

- plugin `.setup`：`invoke_handler` 注册已移除（全局注册），这里只管命令执行依赖的 State（`app.manage(DB)` 等）。
- Extension trait `setup`：跨扩展可见副作用（快捷键钩子、窗口配置、扩展级共享 State）。

**并行 bootstrap 约束**：setup 在 `join_all` 并行执行，A.setup 不应依赖 B.setup 产物（禁跨扩展调用 + 禁框架级共享资源初始化）。框架级共享资源（如 AX timeout）在 lib.rs pre-bootstrap 串行执行。

### 新增命令

1. 在 `native/` 下声明 `#[tauri::command]`
2. 运行 `bun run sync:extensions` 自动注册到 `configure_app!`
3. 前端在 `src/commands.ts` 加 `CMD.xxx` 常量（`check:commands` CI 双向差集校验，禁止裸 `invoke('xxx')`）

### 独立 binary

含独立 `[[bin]]` target 的扩展（当前仅 zsh-autosuggestions）需在 `tauri:dev` 前置编译（`package.json` 已配）+ `deploy.sh` release 编译并嵌入 app bundle。Tauri 不自动打包额外 `[[bin]]`。详见 [zsh-autosuggestions.md](./extensions/zsh-autosuggestions.md)。

## 框架能力（platform / runtime / http）

扩展可消费的框架原语：

- `runtime::window`：主窗口 show/hide/move + panel 转换 + `pick_directory` / `pick_files` / `get_home_dir`
- `runtime::shortcut`：快捷键注册 + 录制 + `register_shortcut_hook`（扩展钩子）
- `runtime::storage`：`TempHandle` RAII（new / Drop 自动清理）+ `cleanup_all_voidnix_temps`（lib.rs setup 启动期统一扫 `voidnix_*` / `voidnix-icon-*` / `voidnix/picker.jpg`）+ `ext_data_dir(app, id)`（统一扩展数据目录，替代各 native/ 重复的 `app_data_dir().unwrap_or_else().join(...)` 模式）+ `save_png_safely`（create_dir_all + path_guard + write 共用）
- `runtime::permission`：系统权限薄壳
- `runtime::llm`：LLM 基础设施（`stream_openai_request` / `validate_ai_request` / `LlmMessage`），agent + translate 共享（`trim_conversation` 在 agent engine 内）
- `runtime::pasteboard`：框架命令薄壳（`pasteboard_write_text`；原语在 `platform::pasteboard`）
- `platform::focus`：焦点管理（`capture_frontmost` / `restore_captured` / `captured_pid`，PREV_FRONT_PID 唯一源）
- `platform::input`：键盘注入（`post_key(key_code, &[Modifier], Option<pid>)` 原语 / `post_combo` 字符串糖）
- `platform::pasteboard`：NSPasteboard 原语（read_text / read_file_urls / read_png(max) / read_tiff_as_png(max) / encode_image_to_png / set_png_bytes / write_text / set_string / set_file_urls(marker?) / set_custom / has_type / change_count / snapshot / restore）
- `platform::selection`：AX 选中文本提取（`try_ax` / `poll_clipboard` / `init_ax_timeout`）
- `platform::path_guard`：路径安全校验（`validate(path)`，canonicalize + 拦系统致命前缀）
- `http::client()`：全局 reqwest 客户端
- `http_get` 命令：通用 HTTP GET（绕过 webview UA/Referer 反爬与 CORS，纯 TS 扩展消费）

## 纯 TS 扩展（无 native/）

前端注册即可。HTTP 走 `http_get` 命令（绕反爬/CORS），不用 webview `fetch`（对反爬站点如 ipwhois.app 会 403）。

## 测试

纯逻辑提取至 `logic.ts`，co-location 写 `logic.test.ts`（vitest 自动扫描）；Rust 用 `#[cfg(test)]` 内联。运行命令见 [AGENTS.md](../AGENTS.md)。abort cleanup 按资源型分流：持有非自动释放资源（事件订阅/子进程/连接池）的 provider 须补 abort 测试，纯 fetch+signal 透传型随 abort 自动释放免测试。
```
