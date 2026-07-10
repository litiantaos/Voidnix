# Voidnix 设计系统

Voidnix 自身的设计系统（仅浅色）。本文档是色值、材质、排版、形态、动画的单一参数源；AGENTS.md「UI 规范」为应用规范、本文档为参数参考。

设计目标：信息密度高、视觉静稳、层次清晰；取 macOS 原生能力实现，不依附任何外部设计规范。所有色值经 WCAG 验证。

## 颜色

双入口、同一数值：`uno.config.ts` theme（Attributify / utility）+ `src/styles/theme.css` CSS 变量（scoped 样式）。扁平化语义色名（`text-primary` / `bg-surface` / `border-black/10`）；scoped 写 `var(--color-*)`，禁止再写裸 rgba。

**文本色阶**（3 档，基于 `#fafafa` 背景 WCAG 验证）：

- `primary` / `--color-text-primary` = `rgba(0,0,0,0.89)`（16.4:1）—— 主要文本、列表标题、数值、强调内容
- `secondary` / `--color-text-secondary` = `rgba(0,0,0,0.60)`（5.7:1）—— 副文本、表头、控件标签、辅助说明、元信息
- `muted` / `--color-text-muted` = `rgba(0,0,0,0.40)`（2.9:1）—— placeholder、分组标题、分隔点、序号、禁用态

**选档原则**：问「这是主内容 / 辅助内容 / 非内容」一个三选一问题。一致性优先于层次细分——同一语义场景跨组件必须同档。`primary` 与 `secondary` 满足 WCAG AA；`muted` 是非内容层（装饰/占位），不参与正文阅读故无 AA 要求。

**基础色**：

- `surface` / `--color-surface` = `#fafafa` —— content layer 底色（窗口根容器）
- `accent` / `--color-accent` = `#3b82f6` —— 强调色（激活态/链接/进度/选中）；取鲜活观感的纯蓝，不用饱和度偏低的传统系统蓝

**层级背景**（Uno `fill-*` + CSS `--color-fill-N`，同数值）：

- `fill-ctrl` / `bg-black/4` / `--color-fill-4` —— 控件底
- `fill-hover` / `bg-black/5` / `--color-fill-5` —— hover / 子层 / markdown 代码底
- `fill-active` / `bg-black/8` / `--color-fill-8` —— active / 强调按压
- `--color-fill-12` / `--color-fill-18` —— 滑轨、snap 强 hover 等（scoped 变量）

**描边与分隔**：

- 描边 `border-black/10` / `--color-border` —— 面板/控件边缘
- 分隔线 `border-black/5` / `--color-divider` —— 卡片内分隔、组间细线
- Smoke 遮罩 `--color-smoke` = `rgba(0,0,0,0.5)`

**语义色**（不进 theme，按需直接写 UnoCSS 预设）：

- 红 `text-red-500` —— 危险/错误
- 绿 `text-green-500` —— 成功态（仅个别场景，toast 主要用 accent 对勾）
- 黄 `text-yellow-600` —— 警告（proxy 日志 level）

## 材质

材质分三层：**原生 Mica**（跨窗口磨砂）、**Acrylic**（WebView 内 backdrop-filter）、**chrome-fade**（渐隐遮罩）。全部经 `uno.config.ts` shortcuts / `theme.css` 抽离，禁止组件内手写 blur/tint/高光环配方。

### Mica（窗口底材）

通透实时磨砂玻璃，强高斯模糊透出壁纸、染浅白。主窗口 + snap-panel。

**原生**（`platform/window.rs::apply_mica_material`，corner_radius 对齐圆角 token：主窗口 `20` = `radius-window` / snap-panel `12` = `radius-panel`）：

- NSWindow `setOpaque:NO` + `clearColor`
- contentView 圆角裁剪 + `NSVisualEffectView`（`Popover` + `behindWindow` + aqua 锁浅色）垫底
- 子视图 layer 非透明（否则盖住材质）

材质选择：不用 `UnderWindowBackground`(21) / `WindowBackground`(12) / `HUDWindow`(13)；用 `Popover`(6)。

**前端壳**（叠在原生材质之上）：

- `mica-tint`：`bg-white/30` 薄白染
- `mica-ring`：inset 高光环（顶 2px 受光 + 全周 1px 细环）
- `mica-shell` = `mica-tint` + `mica-ring` + `radius-window` + `overflow-hidden`（主窗口根）
- snap-panel 根：`mica-tint` + `radius-panel`（原生圆角 12，无主窗 inset 环以免双层）

### Acrylic（WebView 内磨砂，仅外框）

WKWebView 内 `backdrop-filter` 只能模糊 WebView 内已绘制内容。**只用于外壳**（搜索栏 / 浮层），内嵌元素禁止再叠半透明磨砂，否则与外壳糊成一片、可读性崩溃。

- `acrylic`：`bg-white/70 backdrop-blur-2xl backdrop-saturate-150` —— 磨砂基底
- `glass-ring`：inset 顶 2px 白高光
- `acrylic-bar`：`acrylic` + `glass-ring` + `radius-panel` —— 搜索栏、工具条
- `acrylic-panel` / `dropdown-panel`：浮层外壳

### 内嵌实色填充（可读性）

叠在 Mica / Acrylic 上的内容面用灰阶实色，**无 backdrop-filter**：

- `fill-ctrl`：`bg-black/4` —— 按钮 / 输入 / 模块标签 / 图标井默认底
- `fill-hover`：`bg-black/5` —— 列表选中 / hover
- `fill-active`：`bg-black/8` —— 按压强调

`ui-ctrl` = 尺寸 + `fill-ctrl` + `radius-ctrl`；`ui-active` = `fill-hover`（选中宜轻，忌 /8 过深）。

未实现：exclusion blend、noise 纹理。

### chrome-fade（渐隐遮罩）

悬浮栏下自上而下模糊渐变透明。实现在 `theme.css` `.chrome-fade`：

- `backdrop-filter: blur(var(--chrome-fade-blur))` + 白染色渐变
- `mask-image` 自上而下透明
- `pointer-events: none`；高度 `--chrome-fade-height`（MainView 用 `WINDOW.CHROME_FADE_HEIGHT` 覆盖）

任何悬浮顶栏可复用：`<div class="chrome-fade" :style="{ '--chrome-fade-height': h + 'px' }" />`。

### Smoke（模态遮罩）

模态遮罩专用。`BaseDialog` 遮罩 `var(--color-smoke)`，主体实色白 + `radius-panel`（模态非磨砂，强对比聚焦）。

## 排版

**字体栈**（`src/styles/theme.css`）：`'SF Pro Display', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif`。`-webkit-font-smoothing: antialiased` + `text-rendering: optimizeLegibility`。

**字号**（UnoCSS 预设）：

- `text-xs`（12px）：主体字号（列表标题/副标题/控件/标签）
- `text-sm`（14px）：对话框标题、模块正文、强调说明
- `text-base`（16px）：极少，搜索栏输入
- `text-lg` / `text-2xl`：仅 system-status 数值大字、screenshot 加载图标

**字重**：`font-medium`（500，控件/强调）、`font-semibold`（600，数值）、`font-bold`（700，对话框标题）。不使用 light/thin。

## 形态

**圆角**（shortcut → `rounded-[var(--radius-*)]`，值源 `theme.css`；禁止散写 `rounded-md/lg/xl/[Npx]`；内小外大）：

- `radius-panel`（12px）：外框——搜索栏、列表选中行、浮层、dialog、卡片
- `radius-ctrl`（8px）：框内嵌元素——模块标签、图标井、按钮/输入、下拉行
- `radius-window`（20px）：仅主窗口 contentView（原生 corner 20）；snap-panel 原生 12 对齐 panel
- `rounded-full`：圆形小图标按钮、进度条、状态点（保留）

**阴影**（4 级层级）：

- `shadow-sm`：低（控件 hover）
- `shadow-md`：中（dialog、下拉）
- `shadow-lg`：高（浮层强调）
- `shadow-2xl`：最高（浮层强调）
- 主窗口 / snap-panel：原生 NSWindow 阴影（CSS box-shadow 会被窗口 `masksToBounds` 裁剪，故窗口级外阴影走原生）

**描边**：统一 1px。`border` solid 用于面板边缘，`border-black/5` 用于分隔线；主窗口面板边缘高光环走 `mica-ring`。

**间距**：遵循 4px 网格。

- **容器边距**统一 `p-3`（12px）：搜索栏 `inset-x-3 top-3`、列表 `p-x-3 pb-3`、模块内容根、dialog、textarea、浮层 `bottom-3 right-3`、floating 避让、设置页 `flex-col-full-pb`；栏底 gap 与 chrome 常量同步 12px
- **元素间距**三档：`gap-1.5`（6px，控件内紧凑）/ `gap-2`（8px，默认行内行间）/ `gap-3`（12px，区块级）。禁止 `gap-1` / `2.5` / `4`；`gap-0.5` 仅限柱状条/分屏格子等微密 UI

## Shortcuts

`uno.config.ts` shortcuts（全仓统一样式入口）：

**圆角**：`radius-ctrl` / `radius-panel` / `radius-window`

**材质（外框）**：`mica-tint` / `mica-ring` / `mica-shell` · `acrylic` / `glass-ring` / `acrylic-bar` / `acrylic-panel` / `dropdown-panel`

**内嵌填充**：`fill-ctrl` / `fill-hover` / `fill-active`

**控件**：`ui-ctrl` / `ui-disabled` / `ui-active`

**布局 / 表单 / 杂项**：`flex-center` / `flex-col-full` / `flex-col-full-pb` / `form-label` / `form-field` / `input-base` / `group-header` / `overlay-abs`

**CSS 类**（`theme.css`）：`chrome-fade` / `hide-scrollbar`

## 动画

统一 easing，单一源 `uno.config.ts` `transitionTimingFunction`：

- 进场 `ease-out`（`cubic-bezier(0,0,0.2,1)`）：`opacity-0 translate-y-2 scale-95` → `opacity-100 translate-y-0 scale-100`，duration-150
- 离场 `ease-in`（`cubic-bezier(0.4,0,1,1)`）：反向，duration-100
- snap-panel 进出场为原生 NSAnimationContext（alpha + frame 缩放单 group 同步，Mica + 内容整体动画），不走 CSS 过渡
