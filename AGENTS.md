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

**zsh-autosuggestions daemon**：独立 Rust 二进制（`extensions/zsh-autosuggestions/native/daemon/`），通过 Unix socket + SQLite 与 zsh 通信，不依赖主程序运行。纯前缀补全（无 fuzzy）。三信号加权排序：frecency（半衰期 7d，sigmoid 归一化）+ 序列预测（bigram + 3d 时效衰减）+ 目录亲和度（父目录回溯 + 深度衰减）。退出码感知：失败率 >0 的命令按 `fail_rate × 0.5` 惩罚。导入时按 30min 时间戳间隔切分会话边界过滤跨会话噪声 bigram。daemon 内存增量更新 stats（不全量重载），序列缓存 LRU 淘汰（500 容量）。daemon binary 随主程序分发（`src-tauri/Cargo.toml` `[[bin]]`），`on_setup` 启动时检测版本变化自动替换 + kill 旧进程，无需用户开关。

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
│   ├── layout/         # MainView / ContentView
│   └── declarative/    # Tier 2 声明式 UI（Host / List / Markdown / Form / Detail / Stream）
├── composables/        # useSearchCommand / useScrollPosition / useInputControl / useSettingsInput / useTauriListener / useShortcutConfig / useStreamOutput / useFloating
├── core/               # module-registry / module-helpers / async-view / tier2-registry / worker-sandbox
├── stores/             # app（窗口/弹窗）/ settings（持久化 + namespace<T>）/ update
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
- Git commit：`<type>(<scope>): <中文描述>`，不主动执行 git 操作
- 文档不用表格，言简意赅
- 修改代码后必须同步更新 AGENTS.md 中相关描述
