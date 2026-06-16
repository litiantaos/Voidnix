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
bun run test                       # 前端（Vitest + happy-dom）
bun run test:watch                 # 前端监听
bun run test:e2e                   # E2E（Playwright）
cd src-tauri && cargo test --lib   # Rust
```

E2E 对 Vite dev server。原生窗口行为（快捷键/焦点/隐藏）仍需人工验证。

## 开发扩展

**Tier 1（内置扩展）**：编译期 Rust + Vue，随应用分发。双注册：`configure_app!`（编译期命令注册）+ `Tier1Extension` trait（运行时 `on_setup` 钩子）。详见 [`docs/tier1-extensions.md`](docs/tier1-extensions.md)。

**Tier 2（第三方扩展）**：运行时纯 JS，Worker 沙箱 + 声明式 UI（5 原语）。详见 [`docs/tier2-extensions.md`](docs/tier2-extensions.md)。

现有扩展（12）：Tier1Extension — clipboard、screenshot、awake、zsh-autosuggestions、window-manager、finder-ext、translate、chat；插件型 — search、ip；纯前端 — calculator、settings。

复杂扩展文档：[zsh-autosuggestions](docs/extensions/zsh-autosuggestions.md)、[screenshot](docs/extensions/screenshot.md)、[search](docs/extensions/search.md)、[clipboard](docs/extensions/clipboard.md)、[translate](docs/extensions/translate.md)。

## 架构要点

**前后端通信**：前端优先用 `src/bindings.ts`（tauri-specta 自动生成；改 Rust 结构体后 `bun run sync:extensions && cd src-tauri && cargo test --features specta export_bindings -- --nocapture` 重新生成）。流式/事件用裸 `invoke()`，Rust 用 `app.emit()`，所有 Command 须在 `configure_app!` 注册。

**模块子视图**：`open_module_subview(moduleId, subviewId, payload)` → Rust 显示主窗口 + 发 `open-module-subview` 事件 → App.vue 激活模块、调用 `onOpenSubview`。模块通过 `subviews` 声明组件，通过 `appStore` 控制切换。

**窗口**：`LSUIElement=true` + `ActivationPolicy::Accessory` 隐藏于 Dock。`panel::convert_to_panel` 转 `NonactivatingPanel`，显示不抢 NSApp active，关闭时 `deactivate` + `activate_app_by_pid(prev_pid)` 还给原应用。

**全局快捷键**：`src-tauri/src/core/shortcut.rs`，四槽位：`main` / `clipboard` / `translate` / `chat`。

**搜索**：Rust 端只做数据召回（`mdfind` / app 扫描 / clipboard SQL），返回全量候选 + `use_count` 元数据；过滤排序统一在前端走 `src/utils/fuzzy.ts::scoreFields()`（基于 [pinyin-pro](https://github.com/zh-lx/pinyin-pro)，`precision: 'start'` + `continuous: true` + `v: true` 三开关锁死中文缩写/全拼/ü→v 语义）。`frequencyBoost(useCount)` 做 log 平滑的频次加权，全局排序层级：模块(+500) > 应用(+300) > 文件夹(+80) > 文件。

**searchItems**：模块声明 `searchItems: () => ModuleSearchItem[]`，框架自动调 `scoreFields` 做多通道模糊匹配（中文/拼音/英文，全词/单字/缩写），适合半静态内容；动态内容用 `onModuleSearch`。

**UI 槽位**（仅这些，不增不减）：`view`（主视图）、`searchBarAccessory`（搜索栏右侧）、`subviews`（命名子视图）。槽位组件 `Actions` 后缀，私有 UI 禁用 `Toolbar`/`Header`/`Footer`，用语义名如 `AnnotationPalette`。

**状态栏**：框架层全局组件 `StatusBar`，固定于 MainView 底部（`h-6`）。左侧显示瞬时消息（`appStore.showStatus(msg, duration)`，自动淡出）或搜索结果计数；右侧显示上下文快捷键提示（根据搜索状态/模块/子视图动态变化：有输入时 `esc 清空`，无输入时 `esc 关闭/返回`；文件结果追加 `⌘↵ 访问`）。扩展通过 `copyAndHide` / `copyAndShow`（`src/utils/clipboard.ts`）自动获得「已复制」反馈，Tier 2 通过 `ctx.clipboard.write` 自动传导，无需额外调用。模块可通过 `AppModule.enterHint`（如 `'粘贴'`/`'复制'`）自定义 ↵ 动作描述，`AppModule.multiSelectHint` 显示 `⇧/⌘ 多选` 提示，`AppModule.deleteHint`（如 `'删除'`）显示 `⌘⌫` 提示（仅控提示，快捷键由模块自绑）。

**zsh-autosuggestions**：纯 zsh 内核补全（无 SQLite/daemon/IPC，全部 hot path 在 zsh 内存中）。改 binary 内容必须 bump `BIN_VERSION`（`mod.rs`），否则不部署。算法/数据流/分发/Ctrl+C 拦截等深度细节详见 [`docs/extensions/zsh-autosuggestions.md`](docs/extensions/zsh-autosuggestions.md)。

```typescript
SearchResult { id, title, module; description?; icon?; score?; shortcut?; data?: { path?, kind?, icon?, ... } }
```

`kind` 权重：`application` > `folder`/`file` > `module` > `clipboard`

## 目录结构

```
src-tauri/src/
├── lib.rs / main.rs    # 入口
├── extensions.rs       # 自动生成（configure_app! 宏，勿手改）
├── type_gen.rs         # 自动生成（tauri-specta，specta feature-gated）
├── core/               # 核心模块（shortcut / window / tier1 / ext_* / keyword_match / permission）
├── infra/              # 基础设施（http / path / pinyin / sse）
└── macos/              # macOS 原生桥接（panel / skylight / text_selection / click_monitor / permission / mac_utils）

src/
├── components/
│   ├── ui/             # 原子组件（开发只用这些，禁止手写底层标签）
│   ├── layout/         # MainView / ContentView / StatusBar
│   └── declarative/    # Tier 2 声明式 UI（Host / List / Markdown / Form / Detail / Stream）
├── composables/
├── core/               # module-registry / module-helpers / async-view / tier2-registry / worker-sandbox
├── stores/             # app / settings（持久化 + namespace<T>）/ update
├── types/              # module / declarative / ext-manifest
└── utils/
```

新增文件按所属模块归位，勿新建顶层分类。

## UI 规范

UnoCSS + TailwindCSS 最佳实践，遵循官方规范。

只用 `@/components/ui/` 原子组件，禁止手写底层标签。主题色 `accent`；`rounded-md`（控件）/ `rounded-lg`（面板）；`h-7`；`text-sm` / `text-xs`；色阶 `text-tx-primary → secondary → subtle → muted → hint → faint`。

**禁止 arbitrary 值**：class 中禁止使用 `[10px]`、`[#ff3b30]` 等方括号任意值。颜色用 Tailwind 预设色（`red-500`、`gray-50`）或主题语义色（`accent` / `tx-*` / `surface`）+ 透明度修饰（`black/5`）；尺寸/间距用预设档位（`text-xs`、`gap-0.5`）。无合适预设时在 `uno.config.ts` theme 中定义，而非内联任意值。

**写法规范**：原生 HTML 元素使用 Attributify 模式（如 `flex items="center" gap="2" p="4" text="sm" bg="white" rounded="lg"`），Vue 组件 props 保持 `class`。常用 Shortcuts 放在 `class` 中。

**Attributify 禁用属性**：`animate` 等与 DOM 原生属性同名的特性禁止用 Attributify（Vue `shouldSetAsProp` 对 `key in el === true` 的属性走 property 赋值而非 `setAttribute`，导致 HTML 属性不存在、CSS 选择器失效），必须用 `class="animate-spin"` 代替。

**Shortcuts**：`ui-ctrl`（控件基础样式）、`ui-disabled`、`ui-active`、`flex-center`、`flex-col-full`、`flex-col-full-pb`、`form-label`、`input-base`、`action-footer`、`form-field`、`group-header`、`overlay-abs`

**Rules**：`hide-scrollbar`（`theme.css` 中定义，含 `::-webkit-scrollbar` 伪元素）

## 存储结构

扩展自管数据一律放各自 `extensions/<id>/`，无共享 `data/` 目录。

```
~/Library/Application Support/com.litiantao.voidnix/
├── config/settings.json              # 全局配置（分组结构，禁止平铺）
└── extensions/
    ├── clipboard/clipboard.db        # 剪贴板历史（SQLite WAL，伴随 -wal/-shm）
    ├── calculator/calc_history.json  # 计算器历史
    ├── finder-ext/commands/          # Finder 扩展 IPC：cmd_*.json 瞬时命令 + enabled 标志
    ├── zsh-as/                       # zsh 补全：bin/ index.cache signals.log enabled
    └── <tier2-id>/                   # Tier 2 运行时：manifest.toml + index.js + storage.json + assets/

~/Library/Caches/com.litiantao.voidnix/
├── extensions/search/icons/          # search 扩展应用图标缓存（启动时淘汰：上限 400 / 过期 90 天）
└── WebKit/                           # 系统托管 WKWebView 缓存（勿手动清）
```

**dev 镜像**：`com.litiantao.voidnix.dev` 同构（`tauri:dev` 用 dev bundle id）。

**系统托管路径**（macOS 自动产生，勿手动删）：`~/Library/WebKit/<bundle-id>/`、`~/Library/Containers/com.litiantao.voidnix.FinderExt/`、`~/Library/Application Scripts/`、`~/Library/Caches/<bundle-id>/WebKit/`。前端无 localStorage/IndexedDB，全走 Rust 端。

## 约定

- TypeScript 严格模式：`noUnusedLocals` + `noUnusedParameters`
- `isTauri()` 判断环境，非 Tauri 跳过原生调用
- 注释和回复用中文
- Release：`strip=true`, `lto=true`, `codegen-units=1`, `panic=abort`
- Git commit：`<type>(<scope>): <中文描述>`，不写详情，不主动执行 git 操作
- 文档不用表格，言简意赅
- 修改代码后必须同步更新 AGENTS.md 或对应 docs/ 文档中相关描述
- 自开发自用，极简主义、强迫症、精神洁癖，开发秉承彻底、一步到位的理念，不考虑任何兼容性或历史包袱
