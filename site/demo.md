# Demo 动画

首页 Hero 区域内嵌一段实时动画演示（非视频），由 `DemoStage.astro` 组件驱动——一个 1280×720 的拟物 macOS 桌面舞台，840 帧 / 28 秒，纯 JS 驱动的确定性动画（弹簧物理 + 打字机 + 淡入淡出 + 光标插值），完美循环。`ResizeObserver` 自适应缩放到容器宽度，首页和独立预览页共用同一组件。

## 架构

```
src/components/DemoStage.astro   动画组件（HTML + CSS + JS 自包含）
src/pages/demo.astro             独立预览页（壳子 + <DemoStage />）
src/components/Hero.astro        首页（文案区居中 + <DemoStage /> 全宽展示）
```

`DemoStage.astro` 包含舞台全部 HTML、CSS（`.demo-stage` 作用域前缀，不污染首页）、JS（动画引擎 + 自适应缩放）。CSS 用 `<style is:global>` 但所有选择器限定在 `.demo-stage` 下。

## 独立预览页

`/demo` 页面提供调试入口：

- `requestAnimationFrame` 循环按 `FPS`（30）播放，`performance.now()` 驱动帧号
- 键盘控制：`Space` 暂停切换、`←/→` 逐帧步进（暂停态下步进，恢复后回到时间驱动）

**减少动效**：系统「减少动态效果」开启时，不启动动画循环，静态展示搜索结果满载帧（frame 190）。

**捕获模式**（`?capture=1`）——截图脚本逐帧调用：

- 页面暴露 `window.__renderFrame(f)`：调 `renderFrame(f)` 后 `await requestAnimationFrame` 确保 paint 完成
- 初始渲染 frame 0

## 首页集成

Hero.astro 文案区（标题 + chips + 下载按钮）居中布局，DemoStage 在下方展示，max-width 与正文一致（`--content-max: 1100px`，左右各留 clamp(16px,4vw,48px)），圆角 + 阴影裁切。舞台自适应缩放到容器宽度。

## 动画系统

所有动画确定性（无随机），保证实时播放帧帧一致：

- **弹簧物理**（`spring()`）：阻尼振动解析解（stiffness / damping / mass），入场为过阻尼平滑趋近、键帽弹出为欠阻尼微弹，用于元素入场（启动器 materialize、结果行弹入、Agent 气泡、键帽弹出）
- **淡入淡出**（`fadeInOut()`）：进场 + 离场各 `dur` 帧（默认 15），线性插值
- **打字机**（`typeSlice()`）：帧区间内按进度截取文本子串
- **缓动**（`easeOut` / `easeInOut`）：进场缩放与光标位移插值
- **线性插值**（`lerp()`）：窗口吸附位移、光标移动（配合 `easeInOut`）

雾团（Mica 冷蓝光晕）用正弦函数连续漂移，周期对齐 `TOTAL` 实现完美循环。

## 桌面舞台

所有场景在拟物 macOS 桌面上展开，两层常驻（z-index 递增）：

- **壁纸层**（`.fog`）：与产品 Mica 同源的冷蓝渐变雾团（复用 `--mica-fog-a/b` token），正弦漂移
- **菜单栏**（`.menubar`）：顶部 28px 半透明条，左侧苹果 logo（CSS mask 矢量），右侧搜索图标 + 时间

桌面在所有场景始终在场（截图场景时被全屏 overlay 覆盖，收尾时淡入背景）。

## 场景字幕

底部居中浮层（`.caption`），每个场景入场后延迟 3 帧淡入一句简短中文说明（`updateCaption` 按 `CAPTIONS` 帧区间表匹配），引导观众理解当前演示的功能。

## 快捷键键帽动效

每个场景触发前浮现对应快捷键的键帽按下动效（`⌥Space` / `⌥C` / `⌥A` / `⌥S` / `⌥F`），弹簧放大 + 淡出，持续 20 帧（约 0.67s）。`kbdPopAt(f, start, opt, main)` 按帧号偏移驱动，把「功能靠快捷键触发」的核心交互具象化。

## 分镜时间线

帧号区间，TOTAL=840 / 30fps = 28s：

```
desktop     0–840    桌面始终在场（菜单栏 / 壁纸雾团）
summon      0–82     ⌥Space 键帽 → 启动器 materialize
search      82–210   打字 "code" → 应用/文件结果逐行入场 + 选中高亮
gap         210–226  ⌥C 键帽 → 启动器消失 → 重新呼出（场景间过渡）
clipboard   226–344  剪贴板扩展模式 → 历史条目
gap         344–360  ⌥A 键帽 → 启动器消失 → 重新呼出（场景间过渡）
agent       360–485  ⌥A 键帽 → Agent → 用户消息打字 + 工具调用 + 结果
screenshot  488–595  ⌥S 键帽 → 全屏 overlay 覆盖桌面 → 选区框出 → 工具条 → 截图闪光
snap        598–705  光标上移顶部 → snap 面板滑下 → 左右分屏 → 双窗口吸附
finder      703–770  Finder 窗口 + ⌥F 键帽 → 访达面板 → 拷贝路径选中
outro       765–840  Voidnix wordmark + ⌥Space 循环
```

相邻场景帧区间有少量重叠，离场淡出与进场淡入交叉过渡。

### 截图全屏 overlay（488–595）

全屏 overlay 覆盖整个桌面（`box-shadow: 0 0 0 4000px rgb(0 0 0 / 0.42)` 充当选区外遮罩），模拟产品截图模式。截图期间桌面应用窗口（VS Code + 终端）随 overlay 同步淡入作为截图内容：

- 帧 488：overlay + 桌面窗口淡入压暗桌面
- 帧 495：选区弹簧放大入场（accent 边框 + 四角白色手柄 + 左上尺寸标签 `960 × 480`）+ 工具条同时出现（选区左下角，acrylic-bar 材质）
- 帧 568：选区白色闪光脉冲一次 → 截图完成
- 帧 585：overlay + 选区 + 窗口淡出，回归桌面

### 窗口管理 snap（598–705）

桌面出现两个应用窗口（VS Code + 终端），鼠标光标从桌面中部上移至屏幕顶部触发 snap 面板：

- 帧 598：VS Code 窗口（侧栏 + 代码骨架行）+ 终端窗口淡入
- 帧 622：CSS 拟 macOS 箭头光标从 `(720,430)` `easeInOut` 上移至 `(640,10)`（屏幕顶部中心）
- 帧 638：snap 面板从顶部滑下（产品形态：横向五组分区——四角 / 上下半 / 左右半 / 居中环 / 自定义，镂空环用 box-shadow 技法还原）
- 帧 665：光标垂直下移到「左右半」分区 → 该分区 `fill-18` 高亮
- 帧 672：双窗口弹性吸附分屏（`lerp` + `easeOut`）：VS Code → 左半 `(16,36) 616×628`，终端 → 右半 `(648,36) 616×628`
- 帧 682：snap 面板 + 光标淡出

### 访达工具（703–770）

Finder 窗口在桌面展开（标题栏 + 侧栏 + 文件列表），⌥F 键帽唤起访达操作面板：

- 帧 703：Finder 窗口淡入（extensions 目录，首行 search 选中态）
- 帧 712：`⌥F` 键帽弹出
- 帧 727：访达面板在 Finder 窗口上方唤起（启动器形态：搜索栏 + ⌥F 标签 + 操作列表——拷贝路径 / 在终端打开 / 新建文件 / 切换隐藏文件，首项高亮 + `↵`）
- 帧 760：面板淡出

## 视频导出（可选）

实时动画为主，如需导出 MP4/WebM 视频文件（如社交媒体分享），`capture-demo.mjs` 仍可用：

```bash
bun run generate:demo    # = node scripts/capture-demo.mjs
```

流程：启动 dev server → Playwright 2x retina 逐帧截图（840 帧约 9 分钟）→ ffmpeg 编码 H.264 MP4（`crf 18`）+ VP9 WebM（`crf 30`）→ 产物写入 `public/demo.mp4` + `public/demo.webm`。仅捕获浅色主题（`colorScheme: 'light'`）。

## 常见问题

**端口 4399 被占用**：脚本已用进程组信号回收 dev server 整棵子树（`bun → astro → vite`），仅在被 SIGKILL 强杀时可能残留。`lsof -ti:4399 | xargs kill -9` 清理后重试。

**改了 DemoStage 后 preview 没变化**：`bun run preview` 服务的是 `dist/` 构建产物。改完源码需先 `bun run build` 再 preview，或直接 `bun run dev` 用 dev server 预览。

**Playwright chromium 未安装**：`npx playwright install chromium`。
