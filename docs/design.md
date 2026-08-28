# Voidnix 设计系统

浅色 / 深色双轨（`runtime/theme.ts` 写 `<html data-theme="dark|light">`，`auto` 跟随系统；原生 NSWindow appearance 经 `set_window_appearance` 同步驱动 WKWebView prefers-color-scheme）。**视觉已定型**：调参结果以当前界面为准；改规范/代码优先**复用 token**，禁止堆零散相近值。

参数源：`src/styles/theme.css`（`:root` 基元 + 面类）· `uno.config.ts`（组合）· 业务 scoped 只编排，不发明新色/阴影数。

**禁止**：业务裸 hex 当结构色、`black/*` / `white/*` 硬边硬底、状态语义写 `red-500` / `green-500`、业务侧再 `color-mix(accent …)` 浅染。

**例外**：功能遮罩 `mask-smoke`；标注调色板；文件类型/收藏等**内容辨识色**（palette）；图片预览叠加标识（image 序号徽标，主题无关的内容叠色）。

## 基元（先改这里）

- `--cool` / `--cool-deep`：冷相填充与 chip 描边
- `--shadow-ink`：elevation 中性墨
- `--ease-out` / `--ease-in` / `--ease-spring` · `--duration-fastest`（100，退场/微交互）/ `--duration-fast`（150）/ `--duration-normal`（200）/ `--duration-slow`（300，进度型）
- `--space`（12 = 全局 p-3）· `--space-soft`（6px 10px，step/notice）

## 分层

- **soft-surface**：容器（搜索栏 / 浮层 / 大输入底材）— 白边 + fill + blur/saturate **单轨 1.35**
- **soft-card**：抬升卡 = soft-surface + `radius-panel` + `--shadow-card`（助手消息 / system-status）
- **soft-chip**：控件 — 实白 + 1px 冷灰 solid border，无 elevation；focus 改边框色 `--focus-ring-color`
- **ext-tag**：搜索栏只读扩展名
- **ui-active**：列表选中色块 + 轻 blur
- **ui-btn-***：BaseButton variant 面类（primary 实心主钮 / ghost 透明 / danger 淡红底 + 红字 + 红边，hover 加深边色、active 加深底与边）
- **dialog-\***：弹窗近实白（非 soft-surface）；标题/底栏为浮层 + 透明渐变，内容可滚入
- **fill-ctrl**：实底填充（进度轨 / kbd 等，非卡片壳）

### elevation（仅三档 + 两特化）

- `--shadow-bar`：搜索栏 / 输入岛
- `--shadow-panel`：下拉 / 动作浮层
- `--shadow-dialog`：弹窗
- `--shadow-card`：微环 + 近无偏移柔和扩散（助手卡 / system-status，衬边界不抬升）
- `--shadow-float` / `-hover` / `-active`：浮钮 3 层同结构（可插值）

### accent 浅染（markdown 等）

- `--accent-wash` / `--accent-line` / `--accent-line-soft` / `--accent-wash-grad`
- `--focus-ring-color`：控件/大输入聚焦边色（按钮 `:focus-visible`；`BaseInput` 等 `ui-input` 用 `:focus-within`）

## 色

深色模式：`:root[data-theme="dark"]` 覆盖（浅色值仍为 `:root` 默认）。与浅色同 token 体系：文本反相、fill 档改白色基底、阴影 ink→纯黑并提对比（第一层用白环衬边界）、语义色略提亮（soft 档 12% → 16%）、accent 浅染提高饱和。accent `#3d82f0` 与 mist 冷相双轨不变。

### 基础与派生

- canvas = surface `#f8f8f9`
- accent `#3d82f0`（theme + uno 字面同值）
- mist / bubble / 文本阶 / fill-4…18（均派生 `--cool`）
- border / divider / smoke / dialog-\*

### 语义色

- danger / warning / success + soft（soft 浅色 12% / 深色 16%）

### 特化

- 窗壳 mica + 雾；获焦 `.mica-fog-run`
- Agent aurora：`--agent-aurora-warm*`

## 字体 / 圆角 / 间距

- `--font-sans` = mono 优先 + cjk；代码显式 `--font-mono`
- 圆角：ctrl 6 / panel 10 / window 16
- 容器 `p-3`（=`--space`）；Dialog `p-4`；元素间距 gap 全值→场景见 AGENTS.md「元素间距」（3/2/1.5/1/0.5）

## 遮罩

- `chrome-fade` 顶（冷相两点）· `chrome-fade-bottom` 底（多段无冷蓝）· `mask-smoke`

## 组件

- 原子组件 `@/components/ui/`；BaseButton default = `ui-ctrl` + `soft-chip`，其余 variant 走 `.ui-btn-*` 面类（见分层）
- `ui-field`：大输入；`BaseInput panel`：soft-surface 白边（非 field）
- 图标井 `fill-mist`；仪表盘卡 `fill-ctrl`
- 搜索栏拆层 `search-bar` / `search-bar-surface` / `search-bar-content`
- toast / 动作面板：`dropdown-panel` + `fixed bottom-3 right-3`；toast `z-9999`

## Agent

- 用户 bubble（`--color-bubble` accent 浅染 + `--accent-line-soft` 描边）· 助手 `soft-card` · step/notice `--space-soft` + mist/语义 soft
- 输入岛 `ui-field` + `--shadow-bar` · 浮钮 soft-surface + `--shadow-float*` + aurora
- 消息入场动画 `fill-mode: backwards`（禁 `both`：结束帧 transform 驻留 = 每块常驻合成层/IOSurface）

## 速查

- 主窗 `mica-shell` · 搜索栏 `acrylic-bar` · 控件 chip · 大输入 `ui-field`
- 选中 `ui-active` · 主钮 `ui-btn-primary` · 浮层 `dropdown-panel`
- 弹窗 `.dialog-to` · 抬升卡 `soft-card` · 进度/分隔 `fill-active` / `border-divider`
