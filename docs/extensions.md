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

24 个扩展（16 含 native/ + 8 纯 TS，完整清单见 [AGENTS.md](../AGENTS.md)「开发扩展」）。

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
- `mainView`：主视图组件（17 扩展）
- `searchBarAccessory`：搜索栏右侧配件（6：clipboard/agent/translate/proxy/ai-providers/notes）
- `subviews`：扩展私有命名子视图（7：screenshot{ocr}、clipboard{config}、agent{config}、translate{config}、proxy{connections/rules/logs}、homebrew{detail}、notes{config}）
- `subviewTitle`：子视图显示名（id→中文名），激活子视图时搜索栏 placeholder 用「搜索{name}」（2：proxy、homebrew）
- `globalShortcuts`：全局快捷键绑定（6：clipboard/screenshot/agent/translate/finder-ext/notes）
- `placeholder`：搜索框占位提示，激活扩展时显示（7：clipboard/currency/ip/time/base64/calculator/homebrew）
- `windowHeight`：扩展激活时主窗口高度，三种声明语义：
  - **`number`**：固定高度，clamp `[MIN,MAX]`
  - **`'auto'`**：随内容自适应
  - **未声明**：默认高度
  - 共 7 消费者：agent/proxy=840、translate/system-status/video/finder-ext/image='auto'（notes 固定默认高度，输入区内部滚动）
- `subviewHeights`：subview 级高度覆盖，key→语义同 windowHeight（1：screenshot{ocr:'auto'}）

**高度机制**：统一由 `useExtensionHeight`（MainView 全局唯一调用）处理，扩展只需声明，View 不用管——高度解析、系统级动画与顶边锚定位置模型见 [AGENTS.md](../AGENTS.md)「窗口高度」。

生命周期：`setup?()`（启动钩子，无参）。3 行为槽：`disableSearchInput`（扩展自管输入，禁用主搜索框）、`listOptions.multiSelect`（标准列表多选）、`onOpenSubview`（子视图打开回调，如 OCR payload 转交）。三者与能力槽同等地位（见 `runtime/types.ts`）。

### 跨扩展通信

禁止扩展之间直 import 内部状态（如 `import { x } from '@ext/other'`）。同页跨扩展投递数据走 window CustomEvent：发送方 `window.dispatchEvent(new CustomEvent('ext-<event>', { detail: payload }))`，接收方在 `setup()` 内 `window.addEventListener('ext-<event>', ...)` 读 `event.detail`。约定事件名前缀以目标扩展 id 开头（如 `translate-pending-text`），避免冲突。**同步投递是硬要求**：跨扩展跳转（投递 + `setActiveExtension`）必须让 payload 先于目标视图首帧渲染就位——经 Tauri 事件总线的 IPC 往返会晚一拍，期间列表形状未定型（如有图才显示的操作行未插入），用户快速 ↓+Enter 会击中占位行误触（image 的 source 行即文件选择器入口）。接收方 View 的 pending watch 须带 `immediate: true`（LRU 驱逐重挂载时 watch 注册晚于写入）。消费者：finder-ext → image / video（选区路径）、screenshot OCR → translate（待翻译文本）。Rust → 前端或跨窗口投递仍走 Tauri 事件总线（`app.emit` + `listen`）。

### 菜单栏贡献（Rust 侧）

含 native/ 的扩展在 Rust `setup` 内 `menubar::register(MenuBarContribution)` 声明贡献段：

- `title: &'static str`：分组标题（disabled 项渲染，如「保持唤醒」/「代理」）
- `build: Arc<dyn Fn(&AppHandle) -> Vec<MenuEntry>>`：返回当前菜单快照。空 `Vec` = 该扩展当前不贡献（不参与菜单、不影响图标可见性）
- `on_event: Arc<dyn Fn(&AppHandle, &str)>`：收到所有点击的 item id，扩展自行过滤归属项（约定 id 以扩展 id 为前缀避免碰撞，如 `proxy_toggle`）

`MenuEntry` 四态：`Item{id,label,enabled}` / `CheckItem{id,label,checked}` / `Submenu{label,items}` / `Separator`。状态变更后调 `menubar::refresh(&app)` 触发重建。菜单渲染规则、托盘图标可见性开关与现有消费者（awake / proxy）见 [AGENTS.md](../AGENTS.md)「菜单栏」节。

### UI 规约补充

- **`order` 唯一性**：扩展 `meta.order` 在非 hidden 扩展间应唯一，避免扩展列表稳定排序抖动。当前分配：clipboard=10 / translate=20 / agent=30 / ai-providers=35 / proxy=40 / time=50 / ip=60 / uuid=70 / base64=80 / calculator=90 / currency=100 / notes=105 / screenshot=110 / video=115 / image=116 / window-manager=120 / finder-ext=130 / system-status=135 / zsh-autosuggestions=140 / clean-mode=150 / awake=160 / homebrew=170；hidden 扩展 settings=998 / search=999。
- **`disableSearchInput` 决策**：与 `mainView` 独立——mainView 扩展若仍用主搜索框过滤列表（如 clipboard）则不声明；自管输入或无需搜索框（agent/translate/settings 等）声明 `true`。uuid 有 search 但 disableSearchInput（进入后只展示即时结果）。
- **clipboard 敏感内容过滤**：monitor 对源 app 为已知密码管理器（1Password/Bitwarden/KeePassXC 等）或内容匹配 secret 启发规则（`password=`/长 base64/PEM 等）的文本不入库，避免明文密码落 SQLite。ConcealedType marker 是第一道防线，此为兜底。
- **View 根禁止与 ContentView 竞争的纵向双滚**：经 ContentView 渲染的 View（mainView/subviews）根及主内容流不得设 `overflow-y-auto`/`overflow-auto`。ContentView 的 `scrollContainer` 是页面级唯一滚动容器，再设 overflow 形成双层滚动，`BaseList` 键盘导航的 `el.closest('.overflow-y-auto')` 命中内层失效 → 选中框出视口。固定高度局部区域可自滚（如 OCR 图预览、notes 输入区——固定窗高下 ContentView 恒不滚，无竞争）。独立窗口（screenshot/snap-panel/pin，经各自 HTML 入口加载）不经 ContentView，不受此约束。
- **列表选中状态三态语义（BaseList 组件层统一承载）**：`ui-active` 高亮由 BaseList 内部 `localIndex` 驱动，跨会话转移（进入/退出/切换扩展，`watch(appStore.activeExtId)` 变化）统一归首项并 emit 同步父级镜像——仅作用于 KeepAlive 树内的自管列表（首次 activated 标记）；同扩展内 subview 往返（activeExtId 不变）与窗口隐藏唤起（hide/show 不动 activeExtId）保留导航位置——滚动侧对应 scrollKey watch 的 save/restore 与 `clearCache` 不卸载视图（content-visibility 释放 layer，DOM 冻结状态保留）。三个 View 侧补充约定：含 v-if 空态切换的 View 应保持 **BaseList 常挂**（`v-show` 切换；v-if 卸载会使 BaseList 在卸载期间错过 activeExtId 归零、重挂载不触发 activated（inKeepAliveTree 停 false），跨会话归零旁路——消费者 clipboard 的包裹层写法见 [clipboard.md](extensions/clipboard.md)「进入重置」）；动态置顶列表（新记录不断插入顶部，保留索引指向已漂移记录）可按 View 数据语义覆盖「窗口唤起保留」——**会话结束即归位**（窗口隐藏 `window-hiding` + 粘贴成功分支双锚，DOM 更新在隐藏期完成，消除 hide 不 orderOut 下唤起首帧的旧位置残影；粘贴命令经 Rust 隐藏不经前端 hideWindow，`invoke` 返回时窗口已隐藏），消费者 clipboard；**需瞬时归零（滑层瞬落 + 滚顶、不走长距离导航动画）一律用 BaseList expose 的 `reset()`**（与 `reveal` 同族的视图复位契约）——不要在 View 侧拼装「改 selectedIndex + 引用替换」：归零经 props→watch 二跳会被调度拆成两次联动触发（先 items 后 index，各自落在动画路径或 moved 早退），fetch 缓存命中时 applyFilters 空过滤也原样返回旧引用。标准列表（KeepAlive 外，受控于 MainView）不参与归零：其 selectedIndex 转移链（exitExtension 的 savedToolIndex 恢复）自洽，子组件回写会覆盖同步恢复值。自管列表的 View 传 `v-model:selected-index`（或 `:selected-index` + `@select`）保持镜像传导——只接事件不传 prop 时父级镜像会与高亮错位（菜单/执行读镜像分裂）。设置类列表用 `BaseSettingsList`（内部已闭环）。items 替换时的越界/归零策略属各 View 数据语义（身份跟随或归零），BaseList 不介入。
- **列表键盘响应的停用判定（BaseList）**：`canNavigate` 按键时沿组件父链查 `isDeactivated` 实时判定是否处于停用 KeepAlive 子树（Vue `registerKeepAliveHook` 钩子守卫同源语义，状态由框架维护）。不能用 `onDeactivated` 置位的 isActive 镜像：停用视图的响应式 watcher 仍活跃，其内部 v-if 分支翻转会使列表在停用树内卸载后重挂载（如剪贴板 history 随全局 query 过滤清空再回填），重挂载不触发 activated/deactivated 钩子对，镜像停在初值 true 会成为后台仍消费 ↑↓/Enter 的「僵尸列表」——用户在其它扩展内回车误触剪贴板粘贴的根因。
- **列表键盘消费规则（BaseList）**：多选有选区时 ESC 先清选区不退出扩展——经捕获相 document 监听先行于 `useResultNavigation` 的 bubble 退出分派（扩展视图列表晚于 MainView 挂载，注册顺序不保证先行，捕获相是唯一确定性通道）；列表缩短致选中越界时 Enter 不派发（无有效目标不消费按键，方向键 `wrapIndex` 自愈）。
- **选中滑层（BaseList）**：聚焦行色块与 Cmd+Enter 徽标由两层脱流滑层承载（`.selection-indicator` 色块层绘制序在行文本之下、`.selection-hint` 徽标层置于 DOM 尾部盖行尾内容之上——滑块含 transform 形成 stacking context，徽标无法在单元素内同时满足两种层级，两层同参数驱动），选中切换经 `transform: translateY` + `height` 过渡滑动与渐变（变高行间 morph，视口内为 `--duration-fast` / `--ease-out`；height 虽属布局属性，滑块脱流 + 容器 `contain: layout` 使 reflow 不传播到行——同 BaseDialog 内容高 FLIP 的 height 过渡先例）；挂载 / items 引用替换（含同 flush 的「结果替换 + 归零」）/ 尺寸变化（ResizeObserver）/ KeepAlive 重挂一律瞬时落位，不从过期位置滑来。行自身 `ui-active` 保留文本色 / aria 语义，背景经 `.list-focus-row` 透明让位滑块；多选（selectedIds）行不挂此类，保留自身静态色块。徽标随滑层整体移动（切换项零显隐、与色块同帧同位），显隐只由 actionHint 谓词按焦点行翻转（Cmd+Enter 面板按焦点行打开，多选行不再各自显示）。
- **视口跟随滚动（BaseList）**：滚动与滑块由选中移动 watcher 统一编排，按行是否在视野内分档——视口内滑块常规滑动、不滚动；跨界导航**滚动先行、滑块滞后跟随**：滚动按 `--duration-normal` + `--ease-inout`（软起步）rAF 逐帧写 `scrollTop`（参数读 :root token，解析失败回退同值常量），滑层经 `.follow` 类以同曲线滞后跟随（滞后量 `--selection-follow-lag` token 单一定义于 theme.css，CSS 过渡延迟与 JS 连击窗口同源消费；位移与高度同曲线同延迟）。滚动先行而非同步：滑块钉在视野边缘会让焦点视觉静止、读作卡顿；同曲线 + 滞后构造性保证滑层进度恒 ≤ 滚动揭示量（不滑入未揭示的裁剪区），体感为「列表先滚一点、选中框随即移动到新行」。items 替换两者一致瞬时跳变；wrap 首↔末项（末→0 / 0→末，显式判定非距离阈值）所有列表一致瞬时跳变——穿行整列表无导航意义（2 项列表除外：0↔1 即相邻步进，按常规动画，判定退化会让短列表永不滑动）；连击步进窗口 = 本步动画包络 × 0.8 同源推导——视口内滑动 = `--duration-fast` × 0.8、跨界跟随 = （`--selection-follow-lag` + `--duration-normal`）× 0.8，token 调参随动：重定目标发生在动画完成 80% 之后时残余滞后不超过一步的 20%（读感为连续滑行而非追帧漂浮），保留动画；快于此的输入（按住 repeat / 极速连点）瞬时步进；仅选中移动触发滚动（items 单独变化只重落滑块）；每帧回读 `scrollTop` 检测外源写入（用户滚轮/拖拽/滚动锚定）即中断让权；`reveal` 居中滚动同滚动曲线；`prefers-reduced-motion` 下滑层与滚动全部退化为瞬时（逐次读取，偏好切换即时生效）。
- **需 API 配置扩展的未配置空态用 `BaseSetupState`**（`components/ui/`）：key 图标 + 扩展自述标题 + 统一「去配置」主按钮（`common.goConfigure`），点击 emit `configure` 由扩展决定去向——agent 直达 ai-providers 扩展（`setActiveExtension`），translate 进自身设置子视图（`openSubview('config')`，自有 Key 与中枢选用都在此）；按钮文案可经 `actionText` 覆盖（ai-providers 中枢空态用「添加提供商」直达创建弹窗）。新增依赖 API 凭证的扩展复用同款空态，不各自手写。

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

- **全局模式**（`searchEngine.search`）：召回管道（流式增量、首帧合批窗口 + rAF 合帧、dedupe/groupAndSort）与过滤规则（空 query `finalScore>0` / 非空 query 查找型 `fuzzy>0`，即时答案靠 `finalScore>0` 穿透）见 AGENTS.md「搜索引擎」。扩展侧约定：

  - **流式**：扩展可选调用 `ctx.emit(partial)` 多次产出部分结果（如 search 扩展应用 emit 秒出、文件 return 后补），不调用的扩展走一次性 return 行为不变。框架按 `extId:id` 去重，emit 与 return 重叠不产生重复项；但已 emit 的内容不应放入 return——emit 产首批、return 补充，避免多余打分计算
  - **keyword 合流**：入口打分 `scoreExtensionEntry`（name/id/description 正向 + keywords 双向，与 `/` 工具列表共用）；按 query 记忆化——同 query 结果不变，增量 flush 复用缓存免重算
  - **入口抑制**：dynamic 产出相关 tool 型结果（kind=extension，finalScore > 0）的扩展抑制其入口（即时答案优先）；clipboard 等数据型 kind≠extension 不抑制

- **扩展模式**（同一 `searchEngine.search`，`setActiveExtension` 后）：

  - **召回**：只调激活扩展 dynamic，bypass groupAndSort 保留扩展返回序
  - **超时/abort/模式快照**：同样受保护，机制见 AGENTS.md「搜索引擎」
  - **UX**：外壳（`useSearchInput`）延迟 50ms 显示 loading，同步 dynamic 不闪、网络型才占位

- `extensionMode` 区分调用场景：**全局即时答案 calculator / currency / base64**（base64 仅解码，设 minLength 门槛过滤短词误触）；ip / time / uuid 等须 `if (!ctx?.extensionMode) return []`，仅扩展内响应。网络型（currency）全局空 query 仍应跳过请求返回 `[]`，避免拖慢默认列表。
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

去重 → 分组（带过滤）→ 组间定序 → 组内排序 → 组内限流。组间序由 `constants.GROUP_ORDER` 锁死（`application → extension → file → clipboard → web`），不开放给扩展调整。扩展调整相关性的唯一通道：`data.kind` 归组（组间位）+ `boost`（组内位）。

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
- 写盘深克隆 + race 保护：序列化与并发变更互不竞争；启动期 `isLoading` 抑制 watch 冗余写；退出 `onCloseRequested` flush 防抖窗口内变更。
- 不订阅 plugin-store `onChange`：其 `set` 会向本进程回放 `store://change`（无来源标识），回灌会以旧快照覆盖 emit 到达前已 mutate 的新值（实测复现）；所有 config 仅在 main 窗口持有（子窗口纯内存 reactive），无跨窗口同步需求。
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
- `runtime::permission`：系统权限薄壳（含公证状态检测；open_privacy_settings 发起授权会话——设置激活置顶、主窗避让并排钉住至授权完成；perm_drag_hint 录屏手动添加拖拽指引浮窗，授权路径按公证状态分流，见 AGENTS.md 签名一节）
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
