# Demo 动画

首页 Hero 区域内嵌一段实时动画演示（非视频），由 `DemoStage.astro` 组件驱动——一个 1280×720 的拟物 macOS 桌面舞台 + 下方控制栏。

**分段架构**：6 个独立 demo 段（搜索 / 剪贴板 / Agent / 截图 / 窗口管理 / 访达工具），各段 160–350 帧可变（5–12s），共用统一入场节奏（快捷键 → 窗口出现 → 交互 → 关闭 → 桌面静默）。通过控制栏按钮单段切换播放，也可连续拼接播放。首尾帧均为干净桌面，段间可无缝衔接。

## 架构

```
src/components/DemoStage.astro   动画组件（HTML + CSS + JS 自包含）
src/pages/demo.astro             独立预览页（壳子 + <DemoStage />）
src/components/Hero.astro        首页（文案区 + <DemoStage /> 全宽展示）
```

`DemoStage.astro` 包含舞台全部 HTML、CSS（`.demo-stage` 作用域前缀）、JS（分段引擎 + 播放器）。CSS 用 `<style is:global>` 但所有选择器限定在 `.demo-stage` 下。

## 独立预览页

`/demo` 页面提供调试入口：

- `requestAnimationFrame` 循环按 `FPS`（30）播放，`performance.now()` 驱动帧号
- 键盘控制：`Space` 暂停切换、`←/→` 逐帧步进（暂停态下步进，恢复后回到时间驱动）
- 控制栏：6 个段按钮单段切换、连续播放切换、暂停/播放、进度条

**减少动效**：系统「减少动态效果」开启时，不启动动画循环，静态展示搜索段满载帧（frame 104）。

**捕获模式**（`?capture=1`）——截图脚本逐帧调用：

- 页面暴露 `window.__renderFrame(globalFrame)`：`globalFrame` 映射到 `(segIdx, localFrame)`，调 `renderFrame` 后 `await requestAnimationFrame` 确保 paint 完成
- 初始渲染 frame 0（搜索段首帧）

## 首页集成

Hero.astro 文案区（标题 + chips + 下载按钮）居中布局，DemoStage 在下方展示，max-width 与正文一致（`--content-max: 1100px`），圆角 + 阴影裁切。舞台 + 控制栏自适应缩放到容器宽度。

首页传 `controls={false}` 不渲染控制栏，播放器自动走连续播放模式（6 段循环）。预览页默认 `controls={true}`（段按钮 + 连续播放 + 暂停 + 进度条）。

## 统一节奏

入场节奏统一（KBD 6–26 / APPEAR 24–52 为全局常量，所有段共用），但各段时长与 dismiss 帧因交互复杂度不同而可变（定义在 `demo-utils.ts` 的 `SEG_DIS`）：

- search：dur 160 / dismiss 132
- clipboard：dur 190 / dismiss 116（启动器先消失，编辑器窗口 + toast 延续到 ~180）
- agent：dur 200 / dismiss 168
- shot：dur 350 / dismiss 312（标注 / 滚动截屏 / OCR 三阶段，最复杂）
- snap：dur 160 / dismiss 132（光标驱动，无快捷键）
- finder：dur 160 / dismiss 132

统一时间模板（KBD / APPEAR 固定，INTERACT / DISMISS 因段而异）：

```
KBD          6–26        快捷键键帽弹出（20f / 0.67s）
APPEAR       24–52       功能窗口 / 浮层出现（28f，弹簧入场，与 KBD 尾部重叠）
INTERACT     52–dismiss  段特定交互（时长因段而异）
DISMISS      dismiss–dur 功能窗口 / 浮层消失
```

首尾帧均为干净桌面，段间无缝拼接（连续播放模式无跳变）。snap（窗口管理）段无快捷键（光标驱动）；finder 段 KBD 延迟至 18 起（等 Finder 窗口先淡入）。

## 动画系统

所有动画确定性（无随机），保证实时播放帧帧一致：

- **弹簧物理**（`spring()`）：阻尼振动解析解，用于元素入场（启动器 materialize、结果行弹入、Agent 气泡、键帽弹出）
- **打字机**（`typeSlice()`）：帧区间内按进度截取文本子串
- **缓动**（`easeOut` / `easeInOut`）：进场缩放与光标位移插值
- **线性插值**（`lerp()`）：窗口吸附位移、光标移动

雾团（Mica 冷蓝光晕）用正弦函数连续漂移，周期对齐各段 `dur` 实现段内无缝循环。

## 桌面舞台

所有场景在拟物 macOS 桌面上展开，两层常驻（z-index 递增）：

- **壁纸层**（`.fog`）：与产品 Mica 同源的冷蓝渐变雾团（复用 `--mica-fog-a/b` token），正弦漂移
- **菜单栏**（`.menubar`）：顶部 28px 半透明条，左侧苹果 logo（CSS mask 矢量），右侧搜索图标 + 时间

桌面在所有段始终在场（截图段时被全屏 overlay 覆盖）。

## 启动器窗口（复刻真实应用）

启动器窗口忠实复刻产品真实界面（`src/components/layout/MainView.vue` + `ContentView.vue` + `BaseList.vue` + `BaseListItem.vue`）：

- **尺寸**：720×480（与产品 `WINDOW.WIDTH` / `DEFAULT_HEIGHT` 一致）
- **窗壳**（`.launcher-shell`）：`soft-surface-fill` + `backdrop-filter blur(40px) saturate(1.35)` + `radius-window: 16px` + `mica-ring-shadow`
- **chrome-fade**：顶部 76px 渐隐遮罩（`CHROME_HEIGHT` = search bar top 12 + height 52 + gap 12）
- **搜索栏**（复刻 `acrylic-bar`）：`absolute top-3 inset-x-3 h-13`，毛玻璃底 + `radius-panel` + `shadow-bar`；内含扩展标签（`ext-tag`：`fill-5` + `h-7` + `text-xs`）和输入文本
- **结果列表**（复刻 `BaseList` + `BaseListItem`）：`px-3 pb-3 gap-1.5`；每项 `radius-panel` wrapper + `p-3 gap-3` 内部；图标 `h-9 w-9 radius-ctrl fill-mist`；标题 `text-sm`；副标题 `text-xs muted`
- **分组标题**（复刻 `group-header`）：`text-xs muted font-medium px-3 min-h-7`
- **选中态**（复刻 `ui-active`）：`background: var(--ui-active-fill)` + accent 文字色

Agent 段额外复刻：用户气泡（`accent` 浅染实底）、助手卡（`soft-card`）、工具步骤（`agent-step` 结构）、底部输入栏（`agent-footer` + textarea shell）。

## 6 个段

### 1. 搜索（⌥Space → 打字 → 结果）

```
6–26    ⌥Space 键帽弹出
24–52   启动器 materialize（弹簧 + blur 入场）
54–72   打字 "code"（打字机）
70–88   结果逐行入场（弹簧 stagger：应用组 → 文件组，共 7 行）
96–104  VS Code 选中高亮
132–156 启动器消失
```

### 2. 剪贴板（⌥C → 扩展模式 → 历史）

```
6–26    ⌥C 键帽弹出
24–52   启动器 materialize（剪贴板 ext-tag 同步出现）
56–80   5 条历史记录逐行入场（弹簧 stagger）
88–96   第一条选中高亮
116–128 启动器消失
126–170 编辑器窗口出现（粘贴目标）
134+    粘贴文本 "const FPS = 30" + 光标闪烁
138–180 「已粘贴」toast
```

### 3. Agent（⌥A → 工具调用 → 回复）

```
6–26    ⌥A 键帽弹出
24–52   启动器 materialize（Agent ext-tag + 底部输入栏同步出现）
54+     用户消息出现（弹簧渐入，全文直接设值非打字机）
66+     助手卡 + 工具步骤入场（弹簧）
80+     工具结果入场
92–132  助手回复文本打字（打字机）
168–192 启动器消失
```

### 4. 截图（⌥S → 标注 → 滚动截屏 → OCR）

三阶段（dur 350，最长段）：

```
— 标注阶段 (0–120) —
6–26     ⌥S 键帽弹出
0–14     全屏 overlay 淡入（压暗桌面，VS Code + 终端作为截图内容）
28–56    选区拉出（easeInOut 160×120 → 960×480，accent 边框 + 四角手柄 + 尺寸标签）
28–52    十字标光标 + 放大窗跟随
56+      标注工具条出现（选区底部左侧）
82–108   红色矩形标注拉出

— 滚动截屏阶段 (120–215) —
120–136  选区移至 VS Code 代码区（缩窄至 430×327）
132–195  滚动截屏按钮高亮
140–205  右侧实时预览面板增长（100 → 480px）
150–195  代码区滚动（viewport 上移 180px）

— OCR 阶段 (215–350) —
225–238  OCR 按钮高亮
238–246  overlay 消失
250+     启动器出现（OCR 结果面板，弹簧入场）
254+     OCR 文本 + 操作项入场
312–326  启动器消失
```

### 5. 窗口管理（光标驱动 → snap 面板 → 分屏）

```
8–22    VS Code + 终端窗口淡入
28–48   光标 easeInOut 上移至屏幕顶部
42–50   snap 面板滑下
58–66   光标下移到「左右半」分区
68–88   双窗口弹性吸附分屏（lerp + easeOut）
86–92   snap 面板 + 光标淡出
112–128 窗口淡出
```

### 6. 访达工具（⌥F → Finder → 操作面板）

```
0–10    Finder 窗口淡入
18–38   ⌥F 键帽弹出（延迟到 18 起，等 Finder 先出现）
46+     访达操作面板出现（弹簧入场，搜索栏 + ⌥F 标签 + 操作列表）
124–132 面板淡出
132–144 Finder 窗口淡出
```

## 控制栏

画布下方（不缩放），提供段切换与播放控制：

- **段按钮**（6 个）：点击切换单段循环播放，当前段 accent 高亮
- **连续播放**：切换单段 / 全段连续播放模式（全段顺序播放，末段结束自动循环回首段）
- **暂停 / 播放**：暂停切换（暂停态下 ←→ 逐帧步进）
- **进度条**：当前段内进度，可拖动定位（点击 / 拖拽跳转到段内任意帧，自动暂停）

## 段切换重置

切换段时调用 `resetStage()` 将所有舞台元素归零到隐藏态（启动器 / 各面板 / 截图 overlay / 窗口 / 光标 / snap 面板 / 访达 / 键帽 / 字幕），确保段间无残留。

## 视频导出（可选）

实时动画为主，如需导出 MP4/WebM 视频文件，`capture-demo.mjs` 仍可用：

```bash
bun run generate:demo    # = node scripts/capture-demo.mjs
```

流程：启动 dev server → Playwright 2x retina 逐帧截图（1220 帧 / 6 段约 12 分钟）→ ffmpeg 编码 H.264 MP4（`crf 18`）+ VP9 WebM（`crf 30`）→ 产物写入 `public/demo.mp4` + `public/demo.webm`。仅捕获浅色主题。

## 常见问题

**端口 4399 被占用**：脚本已用进程组信号回收 dev server 整棵子树，仅在被 SIGKILL 强杀时可能残留。`lsof -ti:4399 | xargs kill -9` 清理后重试。

**改了 DemoStage 后 preview 没变化**：`bun run preview` 服务的是 `dist/` 构建产物。改完源码需先 `bun run build` 再 preview，或直接 `bun run dev` 用 dev server 预览。

**Playwright chromium 未安装**：`npx playwright install chromium`。
