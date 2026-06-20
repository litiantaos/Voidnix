# 扩展开发

所有扩展同构，目录结构统一。是否含 `native/` 子目录区分实现方式（Rust 后端 vs 纯 TS），不构成分类。全局架构见 [AGENTS.md](../AGENTS.md)。

## 目录结构

```
extensions/<id>/
├── index.ts               # 前端注册（export default defineExtension({...})）
├── config.ts              # defineConfig 自管配置（可选）
├── View.vue               # 主视图（若声明 mainView）
├── Settings.vue           # 设置片段（若声明 settingsView）
├── Actions.vue            # 搜索栏配件（若声明 searchBarAccessory，命名约定 Actions 后缀）
├── logic.ts               # 纯逻辑提取（可选，便于测试）
├── *.test.ts              # 测试（co-location）
└── native/                # Rust 后端（仅需要系统级能力时存在）
    ├── mod.rs             # pub fn init() -> TauriPlugin + Extension trait 实现
    └── ...                # 子模块（commands.rs / engine/ 等）
```

16 个扩展：含 native/ 的 9 个（clipboard、screenshot、awake、zsh-autosuggestions、window-manager、finder-ext、translate、agent、search），纯 TS 的 7 个（calculator、settings、ip、base64、time、uuid、currency）。

## 前端注册

`index.ts` 顶层 `export default defineExtension({...})`，由 `main.ts` 的 `import.meta.glob(['@ext/*/index.ts'], { eager: true })` 自动扫描注册。扩展 `setup()` 钩子在 Vue 挂载后由 `main.ts` 并行触发。

```typescript
import { defineExtension } from '@/runtime/extension-registry'

export default defineExtension({
  meta: { id: 'base64', name: 'Base64', icon: 'i-ri-code-s-slash-line', order: 100, keywords: ['编码'] },
  placeholder: '输入文本进行 Base64 编码',
  search: { dynamic: (query, ctx) => [...] },
  onExecute: (result) => { ... },
  mainView: () => View,
})
```

### 能力槽（按需声明，均有真实消费者）

| 槽                   | 用途                                                                                     | 消费者                                               |
| -------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `search`             | SearchProvider.dynamic 单通道召回                                                        | 见下「搜索集成」                                     |
| `onExecute`          | 搜索结果回车动作（扩展私有）                                                             | —                                                    |
| `mainView`           | 主视图组件                                                                               | 9 扩展                                               |
| `searchBarAccessory` | 搜索栏右侧配件                                                                           | 2：clipboard/agent                                   |
| `subviews`           | 扩展私有命名子视图                                                                       | 1：screenshot{ocr}                                   |
| `settingsView`       | 设置片段（**跨扩展契约**：settings 扩展 mainView 扫描聚合）                              | 3：clipboard/agent/translate                         |
| `windowViews`        | 独立窗口视图（key 须存在于 `tauri.conf.json` `windows[].label`，`-`/`*` 结尾为动态前缀） | 2：screenshot/window-manager                         |
| `globalShortcuts`    | 全局快捷键绑定                                                                           | 4：clipboard/screenshot/agent/translate              |
| `hints`              | 键盘提示（enter/multiSelect/delete）                                                     | enter 3：clipboard/ip/calculator；余各 1             |
| `placeholder`        | 搜索框占位提示（激活模块时显示）                                                         | 7：clipboard/currency/uuid/ip/time/base64/calculator |

生命周期：`setup?()`（启动钩子，无参）。3 承载字段过渡期保留：`disableSearchInput`（模块自管输入）、`listOptions.multiSelect`、`onOpenSubview`。

### 跨扩展通信

禁止扩展之间直 import 内部状态（如 `import { x } from '@ext/other'`）。需要跨扩展投递数据时走 Tauri 事件总线：发送方 `emit('ext-<event>', payload)`，接收方在 `setup()` 内 `listen('ext-<event>', ...)`。约定事件名前缀以目标扩展 id 开头（如 `translate-pending-text`），避免冲突。screenshot OCR → translate 待翻译文本即此模式。

### UI 规约补充

- **`order` 唯一性**：扩展 `meta.order` 在非 hidden 扩展间应唯一，避免模块列表稳定排序抖动。当前分配：clipboard=1 / calculator=2 / ip=5 / translate=8 / agent=9 / screenshot=11 / window-manager=12 / awake=50 / finder-ext=60 / zsh-autosuggestions=80 / base64=100 / uuid=110 / time=120 / currency=130；hidden 扩展 settings=998 / search=999。
- **clipboard 敏感内容过滤**：monitor 对源 app 为已知密码管理器（1Password/Bitwarden/KeePassXC 等）或内容匹配 secret 启发规则（`password=`/长 base64/PEM 等）的文本不入库，避免明文密码落 SQLite。ConcealedType marker 是第一道防线，此为兜底。

## 搜索集成

### SearchProvider（单通道）

```typescript
interface SearchProvider {
  dynamic(query: string, ctx: SearchContext): ProviderResult[] | Promise<ProviderResult[]>
}

interface SearchContext {
  signal: AbortSignal // 新查询覆盖旧查询时 abort
  moduleMode?: boolean // true=模块独占（进入模块），false=全局聚合（默认列表）
}
```

- **全局模式**（searchEngine）：并行调用所有扩展 dynamic，合流 keyword 模块入口 + dedupe + groupAndSort。
- **模块模式**（runModuleSearch）：只调激活扩展 dynamic，bypass groupAndSort 保留扩展返回序。dynamic 返回 Promise（异步网络/IPC）时进入即清空旧结果 + 显示 loading 占位（「先进去再加载」），返回 `ProviderResult[]`（同步）则即时填充无闪烁。
- `moduleMode` 区分调用场景：**全局空 query 时网络型扩展（ip/currency）应跳过网络请求返回 `[]`**，避免拖慢默认列表；模块内空 query 正常执行。
- 半静态内容（如 base64 选项）用模块级缓存自管，走 dynamic 返回。

### SearchResult

```typescript
{
  id: string                  // 扩展内 localId
  title: string               // 进拼音索引，框架统一打分
  module: string              // 框架自动注入（扩展禁填）= 产出扩展 meta.id
  description?, icon?, shortcut?, boost?,
  data: { kind, moduleId?, path?, ... }
  score?: number              // 仅框架填，扩展禁止填
}
```

- `kind` 严格枚举：`application | folder | file | module | clipboard | web`（folder/file 同组）。扩展须正确设置，否则分组错乱。
- `boost?`：扩展可选组内优先级提示（默认 0），`finalScore = fuzzy(title,query) + boost`。调整相关性**只能**通过 boost（score 框架独占）。
- 扩展返回 `ProviderResult`（Omit module），框架注入 module。

### 执行分派（框架内置契约）

搜索结果回车由 `data.kind` 分派：

- `data.kind === 'module' && data.moduleId` → **框架内置激活**（setActiveModule），不走 onExecute（模块入口结果，由 keywordSearchAll 产出）
- 其余 → 扩展 `onExecute` 槽，执行后框架回全局模式 + 隐藏窗口

### 管道层次（不可破坏）

去重 → 分组 → 组内排序 → 组间定序 → 组内限流。组间序由 `constants.GROUP_ORDER` 锁死（`application → file → module → clipboard → web`），不开放给扩展调整。扩展调整相关性的唯一通道：`data.kind` 归组（组间位）+ `boost`（组内位）。

## 扩展配置（defineConfig）

```typescript
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('clipboard', { maxDays: 30 })

// 响应式读写，变更自动持久化至 extensions/<id>/config.json（300ms 防抖）
config.maxDays // → 30
config.maxDays = 60 // 自动写盘
```

- store 实例缓存（模块级 `Map<extId, Store>`），watch 回调复用，禁止每次保存重新 `load()`。
- 加载异步竞态：`load()` 异步，扩展 setup 早期可能读 defaults。安全参数由 Rust clamp 兜底。
- 安全底线（agent 专属）：plain `BOUNDS` const 表达 floor/cap，**权威在 Rust `native/policy.rs`**，TS 仅 UI 镜像，详见 [agent.md](./extensions/agent.md)。
- 含 Rust 命令同步的配置（如开关类）：在 `View.vue` toggle 中显式 `invoke` + 错误反馈（成功才更新 config），勿用 `watch` 静默 invoke 吞错。

框架级配置（全局快捷键、AI Provider）在 `stores/settings.ts`，不在此系统。

## Rust 扩展（含 native/）

### 双注册

1. **编译期**：`pub fn init() -> TauriPlugin`（`plugin::Builder::new().build()`，**无 invoke_handler**）+ `#[tauri::command]` 函数由 `sync-extensions` 扫描，生成 `extensions.rs` 的 `configure_app!` 宏（单一全局 `generate_handler!`）。
2. **运行时**：`Extension` trait（`runtime/registry.rs`），在 `lib.rs` 的 `ExtensionRegistry` 注册，提供 `setup` 生命周期钩子（并行 bootstrap via `join_all`）。

> Tauri 2 插件命令需 `plugin:name|cmd` 格式，裸名只路由全局 `invoke_handler`，故扩展命令必须全局注册、不能放插件 `invoke_handler`。

### Extension trait

```rust
#[async_trait::async_trait]
impl Extension for ClipboardExtension {
    fn id(&self) -> &'static str { "clipboard" }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // 跨扩展可见的副作用：快捷键钩子、窗口配置、扩展级共享 State（app.manage）
        // 命令执行依赖的 State（DB 等）放 plugin .setup 内 app.manage
        Ok(())
    }
}
```

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

- `runtime::window`：主窗口 show/hide/move + panel 转换 + `pick_directory` / `get_home_dir`
- `runtime::shortcut`：快捷键注册 + 录制 + `register_shortcut_hook`（扩展钩子）
- `runtime::storage`：`TempHandle` RAII（new / Drop 自动清理 + `cleanup_temps_by_prefix` 启动扫残留）
- `runtime::permission`：系统权限薄壳
- `runtime::llm`：LLM 基础设施（`stream_openai_request` / `validate_ai_request` / `LlmMessage`），agent + translate 共享（`trim_conversation` 在 agent engine 内）
- `runtime::pasteboard`：框架命令薄壳（`pasteboard_write_text`；原语在 `platform::pasteboard`）
- `platform::focus`：焦点管理（`capture_frontmost` / `restore_captured` / `captured_pid`，PREV_FRONT_PID 唯一源）
- `platform::input`：键盘注入（`post_key(key_code, &[Modifier], Option<pid>)` 原语 / `post_combo` 字符串糖）
- `platform::pasteboard`：NSPasteboard 原语（read_text / string_for_type / data_for_type / has_type / change_count / snapshot / restore）
- `platform::selection`：AX 选中文本提取（`try_ax` / `poll_clipboard` / `init_ax_timeout`）
- `platform::path_guard`：路径安全校验（`validate(path)`，canonicalize + 拦系统致命前缀）
- `http::client()`：全局 reqwest 客户端
- `http_get` 命令：通用 HTTP GET（绕过 webview UA/Referer 反爬与 CORS，纯 TS 扩展消费）

## 纯 TS 扩展（无 native/）

前端注册即可。HTTP 走 `http_get` 命令（绕反爬/CORS），不用 webview `fetch`（对反爬站点如 ipwhois.app 会 403）。

## 测试

纯逻辑提取至 `logic.ts`，co-location 写 `logic.test.ts`（vitest 自动扫描）；Rust 用 `#[cfg(test)]` 内联。运行命令见 [AGENTS.md](../AGENTS.md)。abort cleanup 按资源型分流：持有非自动释放资源（事件订阅/子进程/连接池）的 provider 须补 abort 测试，纯 fetch+signal 透传型随 abort 自动释放免测试。
