# Voidnix 设计系统

Voidnix 自身的设计系统（仅浅色）。本文档是色值、材质、排版、形态、动画的单一参数源；AGENTS.md「UI 规范」为应用规范、本文档为参数参考。

设计目标：信息密度高、视觉静稳、层次清晰；取 macOS 原生能力实现，不依附任何外部设计规范。所有色值经 WCAG 验证。

## 颜色

单一色源 `uno.config.ts` theme，扁平化语义色名（`text-primary` / `bg-surface` / `border-black/10`）。

**文本色阶**（3 档，基于 `#fafafa` 背景 WCAG 验证）：

- `primary` = `rgba(0,0,0,0.89)`（16.4:1）—— 主要文本、列表标题、数值、强调内容
- `secondary` = `rgba(0,0,0,0.60)`（5.7:1）—— 副文本、表头、控件标签、辅助说明、元信息
- `muted` = `rgba(0,0,0,0.40)`（2.9:1）—— placeholder、分组标题、分隔点、序号、禁用态

**选档原则**：问「这是主内容 / 辅助内容 / 非内容」一个三选一问题。一致性优先于层次细分——同一语义场景跨组件必须同档。`primary` 与 `secondary` 满足 WCAG AA；`muted` 是非内容层（装饰/占位），不参与正文阅读故无 AA 要求。

**基础色**：

- `surface` = `#fafafa` —— content layer 底色（窗口根容器）
- `accent` = `#3b82f6` —— 强调色（激活态/链接/进度/选中）；取鲜活观感的纯蓝，不用饱和度偏低的传统系统蓝

**层级背景**（`bg-black/N` 灰阶）：

- `bg-black/4` —— 控件底（按钮/输入/标签默认底）
- `bg-black/5` —— hover / 子层容器 / 卡片
- `bg-black/8` —— active / 强调按压

**描边与分隔**：

- 描边 `border-black/10` —— 面板/控件边缘
- 分隔线 `border-black/5` —— 卡片内分隔、组间细线

**语义色**（不进 theme，按需直接写 UnoCSS 预设）：

- 红 `text-red-500` —— 危险/错误
- 绿 `text-green-500` —— 成功态（仅个别场景，toast 主要用 accent 对勾）
- 黄 `text-yellow-600` —— 警告（proxy 日志 level）

## 材质

### Mica（窗口底材）

不透明壁纸染色材质，active 时微妙染壁纸色（系统仅采样一次，非实时模糊）。主窗口 + snap-panel 使用。

macOS 实现（`platform/window.rs::apply_mica_material`，corner_radius 参数化：主窗口 16 / snap-panel 12）：

- NSWindow `setOpaque:NO` + `clearColor` 底色透明
- contentView `wantsLayer` + `cornerRadius:16` + `masksToBounds` 圆角裁剪
- 嵌入 `NSVisualEffectView`（`material=UnderWindowBackground` + `blendingMode=behindWindow` + `state=.active` + 强制 aqua appearance 锁浅色）作为 contentView 最底层子视图（`NSWindowBelow` 位于 WKWebView 之下），系统 GPU 合成（零 CPU）
- 遍历 contentView 子视图置 `layer.opaque:NO`（Tauri `transparent:true` 只让 WKWebView canvas 透明，CALayer 默认仍 opaque 会盖住材质）

材质选择：不用 `WindowBackground`(12)（Apple 定性 opaque 无模糊）；不用 `HUDWindow`(13)（偏深色 HUD 语义不符）；`UnderWindowBackground`(21) 模糊窗口后内容、近不透明微染壁纸色，契合 Mica 静态质感。

前端 `MainView` 根 `bg="surface/72"` 透出 ~28%（可读性优先；窗口固定居中不可拖动，无「跟移动变化」副作用）。snap-panel 窗口尺寸精简为面板大小，前端面板根透明透出原生材质 + 原生阴影（CSS box-shadow 会被窗口裁剪）。

### Acrylic（浮层磨砂）

半透明磨砂玻璃，用于 transient / light-dismiss 浮层（dropdown / toast / 动作菜单）。WKWebView 内 `backdrop-filter` 只能模糊 WebView 内已绘制内容（等价应用内磨砂，非跨窗口磨砂）。

`dropdown-panel` shortcut 配方：

- blur：`backdrop-blur-2xl`（≈40px 高斯模糊）
- tint：`bg-white/70`（70% 不透明白着色）
- 饱和度：`backdrop-saturate-150`

未实现：exclusion blend（CSS 难精确还原，影响可读性）、noise 纹理（SVG 模拟易显脏，收益低）。

### Smoke（模态遮罩）

模态遮罩专用。`BaseDialog` 遮罩 `rgba(0,0,0,0.5)`，配合主体实色白形成层级（模态非磨砂，强对比聚焦）。

## 排版

**字体栈**（`src/styles/theme.css`）：`'SF Pro Display', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif`。`-webkit-font-smoothing: antialiased` + `text-rendering: optimizeLegibility`。

**字号**（UnoCSS 预设）：

- `text-xs`（12px）：主体字号（列表标题/副标题/控件/标签）
- `text-sm`（14px）：对话框标题、模块正文、强调说明
- `text-base`（16px）：极少，搜索栏输入
- `text-lg` / `text-2xl`：仅 system-status 数值大字、screenshot 加载图标

**字重**：`font-medium`（500，控件/强调）、`font-semibold`（600，数值）、`font-bold`（700，对话框标题）。不使用 light/thin。

## 形态

**圆角**：

- `rounded-md`（6px）：控件（按钮、输入框、标签、shortcut input）
- `rounded-lg`（8px）：面板（dropdown-panel、dialog、列表容器）
- `rounded-xl`（12px）：snap-panel 预览
- `rounded-full`：圆形小图标按钮、状态点
- `16px`：主窗口 contentView（原生 CALayer cornerRadius）

**阴影**（4 级层级）：

- `shadow-sm`：低（控件 hover）
- `shadow-md`：中（dialog、下拉）
- `shadow-lg`：高（主窗口 `MainView` 根）
- `shadow-2xl`：最高（浮层强调，snap-panel 改用原生 NSWindow 阴影）

**描边**：统一 1px。`border` solid 用于面板边缘，`border-black/5` 用于分隔线。

**间距**：遵循 4px 网格（UnoCSS 预设 `p-1`=4px / `p-2`=8px / `p-3`=12px / `p-5`=20px / `gap-2`=8px）。

## Shortcuts

`uno.config.ts` shortcuts（全仓统一样式入口）：

- `ui-ctrl`：控件基础态（`h-7 px-3 rounded-md text-xs font-medium bg-black/4 text-primary` + focus ring）
- `ui-disabled`：`opacity-50 cursor-not-allowed`
- `ui-active`：`bg-black/5`
- `flex-center`：`flex items-center justify-center`
- `flex-col-full` / `flex-col-full-pb`：模块 View 根布局惯例
- `form-label` / `form-field` / `input-base`：表单
- `group-header`：分组标题（`text-xs text-muted tracking-wider uppercase`）
- `overlay-abs`：`pointer-events-none absolute`
- `dropdown-panel`：浮层 Acrylic（见材质系统）

## 动画

统一 easing，单一源 `uno.config.ts` `transitionTimingFunction`：

- 进场 `ease-out`（`cubic-bezier(0,0,0.2,1)`）：`opacity-0 translate-y-2 scale-95` → `opacity-100 translate-y-0 scale-100`，duration-150
- 离场 `ease-in`（`cubic-bezier(0.4,0,1,1)`）：反向，duration-100
- snap-panel 进出场为原生 NSAnimationContext（alpha + frame 缩放单 group 同步，Mica + 内容整体动画），不走 CSS 过渡
