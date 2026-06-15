# Voidnix

macOS 效率启动器。Tauri 2 + Rust + Vue 3。三层扩展架构：

- **Tier 0（框架）**：编译期，Rust + Vue，随应用分发，代码在 `src-tauri/` 和 `src/`
- **Tier 1（内置扩展）**：编译期，Rust + Vue，随应用分发，目录 `extensions/<name>/`
- **Tier 2（第三方扩展）**：运行时，纯 JS，声明式 UI，Worker 沙箱，目录 `extensions/<name>.vnext/`

## 开发命令

```bash
bun install                  # 安装依赖
bun run tauri:dev            # 开发模式（sync → lint → 启动）
./deploy.sh                  # 打包部署（tauri build → 替换 .app → 嵌入 Finder 扩展）
bun run sync:extensions      # 同步扩展注册（自动扫描 → 生成 extensions.rs）
bun run lint                 # Prettier + ESLint（含 UnoCSS class 排序）
```

内部命令（tauri.conf.json 自动调用）：`bun run dev`（Vite）、`bun run build`（sync → lint → typecheck → vite build）

## 自动化测试

Co-location：`*.test.ts` 同目录；Rust `#[cfg(test)]` 内联。

```bash
bun run test                       # 前端（Vitest + happy-dom，111 tests）
bun run test:watch                 # 前端监听
bun run test:e2e                   # E2E（Playwright，8 tests）
cd src-tauri && cargo test --lib   # Rust（11 tests）
```

- 工具函数：`src/utils/*.test.ts` — fuzzy / format / icons / dom / error / provider
- Store：`src/stores/*.test.ts` — app / settings / update
- 核心：`src/core/*.test.ts` — module-registry
- Composable：`src/composables/*.test.ts` — useSearchCommand
- 组件：`src/components/**/*.test.ts` — BaseDialog
- Rust：`ext_manifest::tests` / `tier1::tests`
- E2E：Playwright 对 Vite dev server。原生窗口行为（快捷键/焦点/隐藏）仍需人工验证
- 配置：`vitest.config.ts`、`playwright.config.ts`

## 开发 Tier 1 扩展

内置扩展与主程序同编译同签名。每个扩展自包含：前端 `index.ts` + Vue 组件，Rust 后端 `native/`。

```
extensions/<name>/
├── index.ts              # AppModule 定义 + registerModule()
├── *.vue                 # Vue 组件（defineAsyncComponent 懒加载）
└── native/
    ├── mod.rs            # Tier1Extension 实现 + 模块声明
    └── *.rs              # commands / db / monitor / setup（按需）
```

**注册机制**：

- 前端：`main.ts` glob 自动注册 `@ext/*/index.ts`，无需手动导入
- 后端：`bun run sync:extensions` 扫描 `extensions/*/native/` 和 `src-tauri/src/core/`，生成 `extensions.rs`（`#[path]` 引用源文件不复制，`configure_app!` 宏统一注册命令和插件）
- 连字符自动映射：`finder-ext` → `finder_ext`

**双注册模型**：

- `configure_app!` 宏 → 编译期 API 注册（`#[tauri::command]` + `init() -> TauriPlugin`）
- `Tier1Extension` trait → 运行时生命周期钩子（`on_setup`：窗口初始化、监听器、资源预热、binary 自动部署）

```rust
pub trait Tier1Extension: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn on_setup(&self, _app: &AppHandle) -> tauri::Result<()> { Ok(()) }
}
```

**新增步骤**：`mkdir extensions/my-ext` → 写 `index.ts` + `native/mod.rs` → `bun run sync:extensions` → `bun run tauri:dev`

**现有扩展**（12 个）：

- Tier1Extension（8）：clipboard、screenshot、awake、zsh-autosuggestions、window-manager、finder-ext、translate、chat
- 插件型（2）：search（应用发现 + 缓存）、ip（HTTP 客户端）
- 纯前端（2）：calculator（数学表达式）、settings（声明式 searchItems）

## 开发 Tier 2 扩展

纯 JS/TS，零构建工具，一个文件即可。无 HTML/Vue，UI 由宿主用 5 个声明式原语独占渲染。Web Worker 沙箱 + CSP 锁定，只能通过 host API 访问系统能力。

### 包格式

```
my-ext.vnext/
├── manifest.toml         # 必需
├── index.js              # 必需，单文件 ESM
├── README.md             # 可选
├── i18n/                 # 可选
└── assets/               # 可选
```

### manifest.toml

```toml
[extension]
id = "my-ext"
name = "我的扩展"
version = "1.0.0"
description = "描述"
author = "作者"
icon = "i-ri-puzzle-line"
keywords = ["kw"]
voidnix_api = "^1"

[entry]
main = "index.js"

[capabilities]
required = ["clipboard.write"]
optional = ["storage", "http"]

[ui]
preferred_view = "list"          # list / markdown / form / detail / stream
search_placeholder = "输入内容"

# [settings]    # 可选：扩展设置项
# [shortcuts]   # 可选：快捷键
# [signature]   # 可选：签名验证
```

### 模块协议

```typescript
export default {
  id: string,
  onInit?(ctx): void | Promise<void>,
  onActivate?(ctx): void | Promise<void>,
  onDeactivate?(ctx): void | Promise<void>,
  onSearch?(query, ctx): View | Promise<View>,
  view?(): View | Promise<View>,
  onAction?(actionId, payload: { item?, form?, text? }, ctx): void | Promise<void>,
  subviews?: Record<string, (payload?) => View | Promise<View>>,
}
```

`export default` 前可声明顶层变量/函数（setup code），Worker bootstrap 自动分离。

**list 视图 execute 默认语义**：item 未声明 `actions` 数组且 `title` 非空时，框架直接复制 title 并隐藏窗口，不转发 worker `onAction`。扩展如需自定义 execute 行为，给 item 声明 `actions` 数组（DeclarativeList 会以上抛 primary 或首项 action.id 代替 `'execute'`，框架不再拦截），或把 `title` 留空让框架回落到转发 `onAction`。其他视图（form/detail/markdown/stream）的 action 一律转发到 worker。

### host API

Capability 在 manifest 声明，运行时按需注入，未声明为 `undefined`。

```typescript
ctx.ui.hide()                          // 隐藏窗口
ctx.ui.setView(view)                   // Push 模式更新 UI
ctx.clipboard.write(text)              // 复制到剪贴板（需 clipboard.write）
ctx.http.fetch(url, init)              // HTTP 请求（需 http）
ctx.storage.get(key) / .set(key, val)  // 持久存储（需 storage）
```

### 声明式 UI 原语

扩展返回 View 描述对象，宿主 `DeclarativeHost` 独占渲染：

- `list`：标题 + 副标题 + 图标 + actions
- `markdown`：富文本
- `form`：类型化输入字段
- `detail`：主体 + 侧栏元数据
- `stream`：append-only 流式 markdown

类型定义：`src/types/declarative.ts`，组件：`src/components/declarative/`

### 沙箱架构

Worker 通过 Blob URL 创建，CSP 锁定（无 DOM/网络）。宿主 `worker-sandbox.ts` 代理所有 host API 调用，JSON-RPC 2.0 over `postMessage`。`tier2-registry.ts` 将 Tier 2 扩展桥接为 `AppModule` 适配器。

- Worker 生命周期：首次调用 spawn，5 分钟未活跃 terminate，禁用/卸载立即 terminate
- CSP：`worker-src 'self' blob:`；Capability 强制：安装时检查 required，运行时只注入已声明的 API
- ID 与 Tier 1 冲突时 Tier 2 被跳过

### 加载与开发

- **生产路径**：`~/Library/Application Support/com.litiantao.voidnix/extensions/<id>/`
- **开发加载**：debug 构建自动扫描项目 `extensions/*.vnext/`；release 构建设置 `VOIDNIX_DEV_EXTENSIONS` 环境变量
- **正式版测试开发扩展**：`VOIDNIX_DEV_EXTENSIONS=~/Code/Voidnix/extensions /Applications/Voidnix.app/Contents/MacOS/Voidnix`
- **安装/卸载**：`ext_install`（zip 包）/ `ext_uninstall` 实时生效无需重启

## 架构要点

**前后端通信**：前端优先用 `src/bindings.ts`（tauri-specta 自动生成；改 Rust 结构体后 `bun run sync:extensions && cd src-tauri && cargo test --features specta export_bindings -- --nocapture` 重新生成）。流式/事件用裸 `invoke()`，Rust 用 `app.emit()`，所有 Command 须在 `configure_app!` 注册。

**模块子视图**：`open_module_subview(moduleId, subviewId, payload)` → Rust 显示主窗口 + 发 `open-module-subview` 事件 → App.vue 激活模块、调用 `onOpenSubview`。模块通过 `subviews` 声明组件，通过 `appStore` 控制切换。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。`panel::convert_to_panel` 转 `NonactivatingPanel`，显示不抢 NSApp active，关闭时 `deactivate` + `activate_app_by_pid(prev_pid)` 还给原应用。

**全局快捷键**：`src-tauri/src/core/shortcut.rs`，四槽位：`main` / `clipboard` / `translate` / `chat`。

**搜索**：Rust 端只做数据召回（`mdfind` / app 扫描 / clipboard SQL），返回全量候选 + `use_count` 元数据；过滤排序统一在前端走 `src/utils/fuzzy.ts::scoreFields()`（基于 [pinyin-pro](https://github.com/zh-lx/pinyin-pro)，`precision: 'start'` + `continuous: true` + `v: true` 三开关锁死中文缩写/全拼/ü→v 语义）。`frequencyBoost(useCount)` 做 log 平滑的频次加权，全局排序层级：模块(+500) > 应用(+300) > 文件夹(+80) > 文件。

**searchItems**：模块声明 `searchItems: () => ModuleSearchItem[]`，框架自动调 `scoreFields` 做多通道模糊匹配（中文/拼音/英文，全词/单字/缩写），适合半静态内容；动态内容用 `onModuleSearch`。

**UI 槽位**（仅这些，不增不减）：`view`（主视图）、`searchBarAccessory`（搜索栏右侧）、`subviews`（命名子视图）。槽位组件 `Actions` 后缀，私有 UI 禁用 `Toolbar`/`Header`/`Footer`，用语义名如 `AnnotationPalette`。

**状态栏**：框架层全局组件 `StatusBar`，固定于 MainView 底部（`h-6`）。左侧显示瞬时消息（`appStore.showStatus(msg, duration)`，自动淡出）或搜索结果计数；右侧显示上下文快捷键提示（根据搜索状态/模块/子视图动态变化：有输入时 `esc 清空`，无输入时 `esc 关闭/返回`；文件结果追加 `⌘↵ 访问`）。扩展通过 `copyAndHide` / `copyAndShow`（`src/utils/clipboard.ts`）自动获得「已复制」反馈，Tier 2 通过 `ctx.clipboard.write` 自动传导，无需额外调用。模块可通过 `AppModule.enterHint`（如 `'粘贴'`/`'复制'`）自定义 ↵ 动作描述，`AppModule.multiSelectHint` 显示 `⇧/⌘ 多选` 提示。

**zsh-autosuggestions**：纯 zsh 内核 + stateless rebuild kernel（`extensions/zsh-autosuggestions/native/src/`）。无 SQLite、无 daemon、无 IPC、无 socket、无 launchd。binary 仅做"读 .zsh_history + signals.log → 算 frecency → 写 sourceable zsh cache"，全部 hot path 在 zsh 内存中。

- **数据流**：zsh 启动 → `source index.cache`（<5ms，零解析，版本校验）→ 按键纯内存前缀匹配（sorted 数组扫描，前 N 命中即停）→ precmd 钩子 `print >> signals.log`（零 spawn，条件 append）+ stale 检测（`$HISTFILE -nt $ZSH_AS_CACHE`）触发后台 `zsh-as rebuild`。三个路径完全解耦。
- **binary 命令**（3 个）：`init`（输出 zsh 集成脚本，无模板替换）、`rebuild`（读 .zsh_history + signals.log → 先 rotate+compact signals → 写 index.cache，atomic rename）、`stats`（诊断，支持 `--half-life-days` / `--fail-penalty` 覆盖默认参数；未检测到 extended_history 时提示 `setopt EXTENDED_HISTORY`）。
- **保留算法**：frecency（`(count+1)^0.7 * exp(-dt/half_life)` + K=10 归一，半衰期默认 7d 可配）+ 前缀匹配（`${(b)buf}` 转义 glob 元字符）+ 失败率惩罚（`sqrt(fail_rate.clamp(1.0)) × fail_penalty`，默认 0.8；clamp 防止 fail_count 逾越 history count 导致 score 钳 0）+ 接受率加权（`0.7 + 0.3 × accept_rate`，accept_rate=1.0 不衰减）。
- **文件布局**（`~/Library/Application Support/<bundle-id>/extensions/zsh-as/`）：
  - `index.cache` —— sourceable zsh：`typeset -ga _zsh_as_sorted`（按 score 降序）+ `_ZSH_AS_IDX_VERSION`（zsh 端 source 后校验 `==1`，不匹配则视为格式错误）
  - `signals.log` —— append-only TSV：`<exit>\t<state>\t<cmd>`（3 字段；state：0=无 suggestion 互动，1=accepted，2=rejected；仅 `exit!=0 || state!=0` 时 append 控制体积；rebuild 入口 rotate+compact：>1MB 或含无效行时保留最后 10000 有效行 atomic 写回）
  - `enabled` —— on/off 标志位
  - `bin/zsh-autosuggestions` —— binary（版本号比对复制，见「分发」）
  - `bin_version` —— 已部署 binary 版本号（与 binary 同目录）
- **history 解析**：zsh extended_history 格式 `: <ts>:<dur>;<cmd>`；非 extended 库（无 ts）fallback 用文件 mtime（续行被当作独立命令，已知限制）。多行命令（for/heredoc）折叠为单行（`\n` → 空格）。含控制字符（`< 0x20` 或 `0x7f`）的命令不入 cache（行结构完整性 + 终端安全）。
- **cache reload**：zsh precmd 用 `zstat +mtime` 检测 cache 变化，变化才重新 source（source 前重置 `_ZSH_AS_IDX_VERSION=0`，source 后校验 `==1`）。冷启动 cache 不存在时同步 rebuild（<5MB history，~150ms）或异步 rebuild（>5MB，避免阻塞启动）。rebuild 节流 5 秒避免高频回车 fork bomb。
- **接受信号采集**：`_zsh_as_suggest` 显示 suggestion 时置 `_ZSH_AS_LAST_SUGGESTED=1`（空 suggestion 置 0）；`_zsh_as_accept`/`_zsh_as_execute` 置 `_ZSH_AS_LAST_ACCEPTED=1`。precmd 推导 state（suggested→2，accepted 覆盖→1），仅在有信息量（失败或 suggestion 互动）时 append（cmd 经 `[[:cntrl:]]` strip 与 Rust `is_safe` 对齐），写后清零。精确区分"显示但拒绝"vs"未显示 suggestion"。
- **分发**：binary 随主程序 `[[bin]] zsh-autosuggestions` 编译，打入 `.app/Contents/MacOS/`（Tauri 自动打包 `[[bin]]` target）。`on_setup` 用**版本号比对**（编译期常量 `BIN_VERSION` 写入 `bin_version` 文件）从 .app 复制到 `extensions/zsh-as/bin/`，已部署版本匹配才跳过；并幂等刷新 .zshrc 行。**改 binary 内容必须 bump `BIN_VERSION`（`mod.rs`），否则不部署——init.zsh 经 `include_str!` 嵌入 binary，改 init.zsh 也算改 binary。** .zshrc 行：`export ZSH_AS_BIN=... ZSH_AS_CACHE=... ZSH_AS_SIGNALS=...; eval "$("$ZSH_AS_BIN" init)"`（行尾 marker `# voidnix zsh-autosuggestions` 用于精确 remove）。.zshrc 写入走原子 tmp+rename + `.zshrc.voidnix-bak` 备份。关闭扩展时清理 `index.cache` + `signals.log`（保留 binary 避免反复复制），`set_zsh_autosuggestions_enabled` 返回 `Result`，失败时前端 revert 状态并 `showStatus` 提示。
- **history 路径解析**：zsh 端 `_zsh_as_histfile` 统一解析 rebuild 目标 history：优先 `$HISTFILE`；`.historynew`（macOS Terminal session 副本）或 `$HISTFILE` 未设置时回落 `~/.zsh_history`。cold start 与 precmd stale 检测共用。
- **并发**：`SETUP_LOCK` 串行化 `on_setup` / `set_zsh_autosuggestions_enabled` 路径，poison 时也恢复。
- **Ctrl+C 拦截**：Ctrl+C（SIGINT）不走任何 ZLE widget，POSTDISPLAY 会残留在重绘的新行；且 POSTDISPLAY 是 ZLE 特殊变量，TRAPINT（非 widget 上下文）中只读无法修改。解决方案：`zle-line-init` 时 `stty intr undef` 让 `^C` 作为普通按键进入 ZLE，绑定 `zsh-as-ctrl-c` widget（清空 POSTDISPLAY/高亮/状态 + `zle .send-break` 中断当前行）；`zle-line-finish` / `zshexit` 恢复 `stty intr '^C'` 保证命令执行期间 `^C` 走 SIGINT。

```typescript
SearchResult { id, title, module; description?; icon?; score?; shortcut?; data?: { path?, kind?, icon?, ... } }
```

`kind` 权重：`application` > `folder`/`file` > `module` > `clipboard`

## 目录结构

```
src-tauri/src/
├── lib.rs              # 入口
├── main.rs             # macOS 入口
├── extensions.rs       # 自动生成（configure_app! 宏）
├── type_gen.rs         # 自动生成（tauri-specta，specta feature-gated）
├── core/               # 核心模块（shortcut / window / tier1 / ext_* / keyword_match / permission）
├── infra/              # 基础设施（http / path / pinyin / sse）
└── macos/              # macOS 原生桥接（panel / skylight / text_selection / click_monitor / permission / mac_utils）

src/
├── App.vue             # 根组件
├── main.ts             # 入口（glob 注册 → initAllModules → loadTier2Extensions → preloadAllViews）
├── bindings.ts         # 自动生成
├── components/
│   ├── ui/             # 原子组件（BaseButton / BaseDialog / BaseEmptyState / BaseInput / BaseList / BaseListItem / BaseSelect / BaseSlider / BaseTextarea / ShortcutInput）
│   ├── layout/         # MainView / ContentView / StatusBar
│   └── declarative/    # Tier 2 声明式 UI（Host / List / Markdown / Form / Detail / Stream）
├── composables/        # useSearchCommand / useScrollPosition / useInputControl / useSettingsInput / useTauriListener / useShortcutConfig / useStreamOutput / useFloating
├── core/               # module-registry / module-helpers / async-view / tier2-registry / worker-sandbox
├── stores/             # app（窗口/弹窗/状态栏消息）/ settings（持久化 + namespace<T>）/ update
├── types/              # module / declarative / ext-manifest
├── utils/              # clipboard / tauri / events / dom / provider / error / format / icon-cache / icons
└── styles/
```

## UI 规范

只用 `@/components/ui/` 原子组件，禁止手写底层标签。主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint`。

**UnoCSS 写法规范**：原生 HTML 元素使用 Attributify 模式（如 `flex items="center" gap="2" p="4" text="sm" bg="white" rounded="lg"`），Vue 组件 props 保持 `class`。常用 Shortcuts 放在 `class` 中。

**Attributify 禁用属性**：`animate` 等与 DOM 原生属性同名的特性禁止用 Attributify（Vue `shouldSetAsProp` 对 `key in el === true` 的属性走 property 赋值而非 `setAttribute`，导致 HTML 属性不存在、CSS 选择器失效），必须用 `class="animate-spin"` 代替。

**Shortcuts**：`ui-ctrl`（控件基础样式）、`ui-disabled`、`ui-active`、`flex-center`、`flex-col-full`、`flex-col-full-pb`、`form-label`、`input-base`、`action-footer`、`form-field`、`group-header`、`overlay-abs`

**Rules**：`hide-scrollbar`（`theme.css` 中定义，含 `::-webkit-scrollbar` 伪元素）

## 存储结构

```
~/Library/Application Support/com.litiantao.voidnix/
├── config/settings.json     # 分组结构，禁止平铺
├── data/                    # clipboard.db / calc_history.json
└── extensions/              # Tier 2 运行时 + Tier 1 自管数据（finder-ext/ zsh-autosuggestions/）

~/Library/Caches/com.litiantao.voidnix/icons/
```

## 约定

- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- `isTauri()` 判断环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，不写详情，不主动执行 git 操作
- 文档不用表格，言简意赅
- 修改代码后必须同步更新 AGENTS.md 中相关描述
