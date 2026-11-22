# Voidnix 设计系统

浅色 / 深色双轨（`runtime/theme.ts` 写 `<html data-theme="dark|light">`，`auto` 跟随系统；原生 NSWindow appearance 经 `set_window_appearance` 同步驱动 WKWebView prefers-color-scheme）。**视觉已定型**：调参结果以当前界面为准；改规范/代码优先**复用 token**，禁止堆零散相近值。

参数源：`src/styles/theme.css`（`:root` 基元 + 面类）· `uno.config.ts`（组合）· 业务 scoped 只编排，不发明新色/阴影数。

**禁止**：业务裸 hex 当结构色、`black/*` / `white/*` 硬边硬底、状态语义写 `red-500` / `green-500`、业务侧再 `color-mix(accent …)` 浅染。

**例外**：功能遮罩 `mask-smoke`；标注调色板；文件类型/收藏等**内容辨识色**（palette）；图片预览叠加标识（image 序号徽标，主题无关的内容叠色）。

## 基元（先改这里）

- `--cool` / `--cool-deep`：冷相填充与 chip 描边
- `--shadow-ink`：elevation 中性墨
- `--ease-out` / `--ease-in` / `--ease-inout`（内容平移类，如列表视口跟随滚动） / `--ease-spring` · `--duration-fastest`（100，退场/微交互）/ `--duration-fast`（150）/ `--duration-normal`（200）/ `--duration-slow`（300，进度型）/ `--selection-follow-lag`（60，BaseList 视口跟随的滑块滞后起步，CSS/JS 同源）
- `--space`（12 = 全局 p-3）· `--space-soft`（6px 10px，step/notice）

## 分层

- **soft-surface**：容器（搜索栏 / 浮层 / 大输入底材）— 白边 + fill + blur/saturate **单轨 1.35**
- **soft-card**：抬升卡 = soft-surface + `radius-panel` + `--shadow-card`（助手消息 / system-status）
- **soft-chip**：控件 — 实白 + 1px 冷灰 solid border，无 elevation；focus 改边框色 `--focus-ring-color`
- **ext-tag**：搜索栏只读扩展名
- **ui-active**：列表选中色块 + 轻 blur；按钮选中（`soft-chip.ui-active`）以透明 1px 边占位保持盒模型恒定——防切换时内容盒 ±2px、同行按钮平移跳动。BaseList 行的色块由组件内脱流滑块承载（同 `--ui-active-fill`，选中切换 translateY 滑动，见 docs/extensions.md「选中高亮滑块」）
- **ui-btn-***：BaseButton variant 面类（primary 实心主钮 / ghost 透明 / danger 淡红底 + 红字 + 红边，hover 加深边色、active 加深底与边）。**加载态**统一走 BaseButton `loading` prop：前置 `i-ri-loader-4-line animate-spin` spinner（`text-sm`，与 icon 槽同大——loading ↔ idle 切换零尺寸跳变优先于环径视觉；与文字的间距同 icon 槽统一 `gap-2`）+ 禁交互（同 disabled），主钮面色降级（`.ui-btn-primary.ui-loading`：accent 实底降至 60% 透明，与「不可用」的纯 disabled 面色不动相区分）；文案可随进度切换（homebrew 滚动段范式）；处理任务的完成落位由任务行「完成」文案 + 满进度条承担（见处理态），不占用按钮；禁止业务侧手写 spinner icon 三元（`loading ? 'i-ri-loader-4-line…' : …`）。**图标尺寸单源**：icon/spinner 统一 `text-sm`（14px 配 12px 字的光学配对，与 BaseSelect 箭头同惯例）；覆盖须走 `!text-xs`（!important 确定胜出；裸尺寸类与组件类同 specificity，胜负取决于 UnoCSS 生成顺序，禁用）。**尺寸变体** `size="inline"`（`ui-btn-inline` shortcut）：表单内联图标钮（输入框行内眼睛钮等）随文字行高、贴边内距、muted 色，禁止业务侧手写 `!h-auto !px-1 …` 覆盖串。**无障碍**：icon-only 且传 `title` 时组件自动透传为 `aria-label`，icon-only 按钮必须传 `title`
- **dialog-\***：弹窗近实白（非 soft-surface）；标题/底栏为浮层 + 透明渐变，内容可滚入；高度随内容形态切换平滑重排（JS FLIP + `transition: height`）
- **fill-ctrl**：实底填充（kbd 等底材，非卡片壳）；进度轨用 `fill-active`（见处理态）

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

- 原子组件 `@/components/ui/`；BaseButton default = `ui-ctrl` + `soft-chip`（outline 与 default 同面），其余 variant 走 `.ui-btn-*` 面类（见分层）
- `ui-field`：大输入；`BaseInput panel`：soft-surface 白边（非 field）
- 图标井 `fill-mist`；仪表盘卡 `fill-ctrl`
- 搜索栏拆层 `search-bar` / `search-bar-surface` / `search-bar-content`
- 动作面板：`dropdown-panel` + `fixed bottom-3 right-3`（toast 已迁原生 NSPanel 浮窗，不再走 web 层——详见 AGENTS.md「toast 提示」节）
- `ActionMenuHint`：快捷键键帽，渲染于 BaseList 选中滑层（`.selection-hint`）内随焦点行平滑移动——运动由滑层 transform 承载，切换项零显隐（absolute、`right` 12px、flex 垂直居中，canvas 实底 + divider 细边，底部透明模糊遮罩 backdrop blur + mask 渐变、上下撑满行高、`pointer-events: none`）；显隐只表达「焦点行有无动作」的数据语义（actionHint 谓词），翻转时淡入淡出（无位移无缩放），opacity 落在 scrim/key 子元素自身（祖先 opacity 隔断 backdrop 采样致遮罩突现），退场完成后容器延迟交还 visibility，稳态未显示时整树跳过绘制；经 BaseList `actionHint` prop 供给（boolean / item 谓词），未传零渲染

## 处理态（任务行）

扩展主处理任务（video 转码 / archive 压缩解压 / image 抠图拼接）统一为「任务行」：任务行**常驻**，
固定在独立分组 `common.group.task`（分组标题「任务」）并**置为表单第一组**——主操作始终在首屏第一项，
不与选项混排；其下按需分组（`通用 / 文件 / 参数 / 输出`；archive 压缩另有 `过滤`），任务行表达
「任务名 + 状态 + 主操作」，不属于设置项。行由 `BaseSettingsList` 的 `title-{id}` /
`trailing-{id}` 槽位组合，**单行紧凑布局**（不再用标题下面压一条通栏进度条）：

- **左**：与选项行同构——video/image 无图标、标题从行内容左缘起（与其它行标题对齐）；
  archive 行本身都有图标井，任务行用**静态动作图标**（archive / unarchive）保持同构。**不要**
  额外放 spinner/对勾等状态图标（右侧进度条已承担活动信号）。标题后接 muted 后缀
  （批量 `第 i/n 个`、图片阶段文案、首个进度事件前的 `准备中…`）。
- **右**：6px 进度条（`BaseProgress`，固定 `w-40`）+ **紧贴条尾**的百分比（`tabular-nums`，
  无分母时不显示，条与百分比 `gap-2`）+ 取消按钮（可取消时，百分比与按钮 `gap-3` 留宽一档）；
  不设固定宽度百分比列（会与条脱开显得散）。
- **空闲**：标题 = 任务名，不用泛化的「操作」——video `视频压缩 / 格式转换 / 音频提取`；
  archive `创建压缩包 / 解压压缩包`；image `图片去背景 / 长图拼接 / 处理结果`。
- **主按钮**：需要启动一轮处理 → 统一「开始」（video / archive 压缩·解压 / image 去背景）；结果 /
  拼接预览已就绪 → 「复制 + 保存」（导出动作，不复述标题）。**未就绪时禁用**
  （无文件 / 核心缺失 / 名称为空 / 无图或不足两张）。
- **常驻**：任务行不随输入出现/消失，列表高度与选中索引稳定，主操作永远在第一位；archive 空态是入口选择
  （`通用` 模式 + `文件` 输入文件），进入表单后任务行常驻，选项按需分组（`文件 / 参数 / 输出 / 过滤`）。
- **不定进度**：后端无分母（bsdtar 解压、Vision 推理、Bria 三段）时进度值传 `null`，
  30% 宽填充 1.4s 循环（`prefers-reduced-motion` 停用），**绝不伪造百分比**（0 亦按不定进度处理）。
- **完成收尾**：满进度 + 「完成」文案停留 600ms，再执行扩展自身收尾
  （toast / 结果操作按钮 / 揭晓并关窗）。
- **失败**：任务行复位空闲态 + toast，不落常驻错误行（错误详情不塞列表）。
- **回车**：任务行回车 = 主按钮动作（处理中即取消；完成闪现期 / 未就绪不响应），与按钮同守卫。

### 进度载体全景

条状进度统一用 `BaseProgress`（`tone`：accent / success / warning / danger；`size`：md 6px / sm 2px；
`value: null` = 不定进度）。按「谁拥有这个操作」分工，不得越界：

| 场景                                        | 载体                                                                   | 消费者                                        |
| ------------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------- |
| 扩展主处理任务（可取消 / 批量 / 多阶段）    | 「任务」分组（表单第一组）任务行：`BaseProgress` + 紧贴百分比 + 取消   | video、archive、image                         |
| 控件拥有的长操作（依赖下载 / 包管理器命令） | 触发按钮内文案（百分比 / 步骤滚动），不占任务行                        | video FFmpeg、proxy 核心、homebrew 更新与服务 |
| 弹窗内全局操作（检查更新 / 下载安装）       | 弹窗内 `BaseProgress` + 紧贴百分比                                     | UpdateDialog                                  |
| 数据仪表（占用 / 配额，不是任务）           | `BaseProgress` 按阈值切 tone（系统占用）；文本 + SparkLine（模型配额） | system-status、ai-providers                   |
| 无分母加载（搜索 / OCR / 握手）             | `BaseButton` loading / `BaseEmptyState` spinner                        | 全局                                          |

- **任务行**：处理任务可批量、多阶段、可取消，需要同时表达「在做什么 / 到哪 / 怎么停」，且处理期间
  表单整体失活——进度属于整个视图，必须离开按钮；**不得**写成某行副标题纯文本（旧 video
  `进度 2/5 · 45%`：信息与载体脱钩、无取消入口）。
- **按钮内**：操作由触发按钮拥有——依赖下载（单一产物、字节可量化、无取消）写百分比
  （`min-w-*` + `tabular-nums` 防宽度抖动）；包管理器命令（homebrew update / upgrade / services）
  无分母、无取消，按钮内滚动步骤文案（`brew-roll`），不挪任务行。
- **数据仪表**：不是任务进度——系统占用按阈值切语义色（阈值见 `usageTone`），配额是文本 + SparkLine，
  不套取消 / 完成语义。
- **禁止手写轨道 / 填充**：`UpdateDialog` 与 system-status 已统一到 `BaseProgress`（各自的
  `.progress-track` 样式已删除）；新增进度场景只组合 `BaseProgress`，不复制条形样式。

## Agent

- 用户 bubble（`--color-bubble` accent 浅染 + `--accent-line-soft` 描边）· 助手 `soft-card` · step/notice `--space-soft` + mist/语义 soft
- 输入岛 `ui-field` + `--shadow-bar` · 浮钮 soft-surface + `--shadow-float*` + aurora
- 消息入场动画 `fill-mode: backwards`（禁 `both`：结束帧 transform 驻留 = 每块常驻合成层/IOSurface）

## 速查

- 主窗 `mica-shell` · 搜索栏 `acrylic-bar` · 控件 chip · 大输入 `ui-field`
- 选中 `ui-active` · 主钮 `ui-btn-primary` · 浮层 `dropdown-panel` · 条状进度/仪表 `BaseProgress`（tone/size）
- 弹窗 `.dialog-to` · 抬升卡 `soft-card` · 进度/分隔 `fill-active` / `border-divider`
