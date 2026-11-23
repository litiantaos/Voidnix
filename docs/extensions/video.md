# 视频处理（video）

压缩、格式转换、提取音频。FFmpeg 子进程，扩展视图操作。

## 能力

- 选输入视频（框架 `pick_files` 多选，扩展名白名单），多文件批量处理
- **跨扩展接收**：finder-ext 等经同页 window CustomEvent `video-pending-input-path` 同步投递路径数组（访达多选区全量带入），`pendingInputPaths` watch（immediate）后自动加载并逐个 probe
- 探测元数据（时长 / 分辨率 / 编码 / 体积）；多文件副标题汇总总时长 · 总大小
- **压缩**：质量预设 high / balanced / small
- **格式转换**：mp4 / mov / mkv / webm / gif
- **提取音频**：m4a / mp3
- 可选缩放：original / 1080 / 720 / 480
- 输出默认与源同目录，命名 `{stem}.{compressed|converted|audio}.{ext}`；可改输出目录
- 批量队列：单文件失败跳过继续下一个，结束汇总 toast（`完成 x/n，y 个失败`）；取消终止整批；全部成功后清空选择回到初始态，失败/取消保留列表可重试（重试重跑全部文件，输出冲突自动加 `-1/-2` 后缀）
- 重新进入扩展即新会话：未在处理中的文件选择清空重置（处理中保留进度上下文）
- Rust 端保持单任务模型（BUSY 互斥）；队列在前端串行发起（复用 `video_run`，终态经 Channel 事件推进——invoke resolve 不区分成败）。批量状态（paths / metas / busy / 进度计数）放 View.vue 模块级 reactive，KeepAlive LRU 驱逐卸载后队列继续跑、重开面板计数不丢（窗口隐藏走 content-visibility 冻结，状态原样保留）；WebContent navigate 重载会丢队列（当前文件跑完即止，重开面板退化为孤儿观察模式）
- 可取消；窗口隐藏后任务继续

## UI

处理态任务行**常驻表单第一组**（「任务」分组，主操作始终首项，见 [design.md](../design.md)「处理态」）；「模式」选择移到其后的通用组（同 image 扩展）。空闲标题 = 任务名（`视频压缩` / `格式转换` / `音频提取`），行尾主按钮「开始」（核心未就绪、下载中或无文件时禁用）；处理中标题 = 动作进行时（`正在压缩` / `正在转换格式` / `正在提取音频`）+ muted 后缀（批量 `第 i/n 个`，首个 progress 事件前 `准备中…`），右侧 6px 进度条 + 紧贴条尾的百分比（条与数字 gap-2、数字与取消 gap-3）+ 取消；孤儿观察态 paths 为空但 busy 置起时仅渲染「取消」，保住取消入口。完成闪满进度 600ms 后清空选择，任务行回到空闲任务名（不消失）。回车执行主按钮动作（处理中 = 取消，完成闪现期 / 未就绪不响应）。

「输入视频」行承载核心 + 选文件：

- 核心未就绪：副标题提示 +「下载 FFmpeg」（回车同触发下载）
- 下载中：副标题 `正在下载 FFmpeg…` + 按钮内百分比——依赖下载归按钮，不进任务行（分工见 [design.md](../design.md)「进度载体全景」）
- 就绪未选文件：副标题 `核心版本：FFmpeg x.x.x` +「选择」
- 已选单文件：标题文件名、副标题仅元数据（不显示核心版本）
- 已选多文件：标题 `{n} 个视频`、副标题总时长 · 总大小（探测完成度渐进）
- 进行中：副标题保持文件元数据（进度归任务行，不再写成副标题纯文本）
- 完成后 toast（单文件原文案、多文件汇总），全部成功时任务行完成闪后清空选择回到初始态（任务行常驻，标题回到空闲任务名、按钮禁用），不常驻输出结果行

「剪辑」入口行**「高级」分组收在列表末尾**（流水线之外的能力）：它是独立剪辑工作台的跳转入口，不是流水线的设置项/模式，组边界声明类别差异（同「任务」组为任务行单开的先例）；末尾位稳定（不随任务行状态跳动）、不打断表单阅读流，无副标题。

## 核心

优先系统 `PATH` 与 Homebrew 常见路径（`/opt/homebrew/bin`、`/usr/local/bin`）中的 `ffmpeg` / `ffprobe`。

未找到则按需下载 [eugeneware/ffmpeg-static](https://github.com/eugeneware/ffmpeg-static) 静态构建：

- 版本 `b6.1.1`，arm64/x64，sha256 校验
- 先 `gh-proxy.com` 镜像，失败回退直连 GitHub
- ffmpeg + ffprobe 两段进度累计上报
- 写入 `ext_data_dir/video/`；下载走 `runtime/binary_fetch` 统一流式落盘——async 任务内逐 chunk 同步 write_all，避免整包读入内存（旧实现 ffmpeg gz ~100MB 常驻 Vec）

### 编码策略

- 压缩模式用码率封顶（避免 VT `q:v` 撑大体积）
- 转换模式优先 `h264_videotoolbox`；硬件路径任意非取消失败即软重试 `libx264` 一次（不依赖 stderr 子串）
- H.264 输出带 `-pix_fmt yuv420p`
- webm/gif 走软编
- 整段 encode（进度循环 + wait）6h 超时

## 命令

- `video_core_status` / `video_ensure_core`
- `video_probe`
- `video_run`（`Channel<VideoEvent>`：started / progress / done / error）
- `video_cancel` / `video_job_status`

框架：`pick_files`（与 `pick_directory` 并列，NSOpenPanel 泛化）。

## 配置

`extensions/video/config.json`（defineConfig）：

- `defaultMode` / `defaultQuality` / `defaultFormat` / `defaultScale`
- `outputDir`（空 = 与源同目录）
- `preferHardware`

## 目录

```
extensions/video/
├── index.ts / config.ts / logic.ts / locales.ts / View.vue
├── windows/ClipWindow.vue        # 独立剪辑窗口（clip.html 入口）
├── windows/clip/clip-logic.ts    # 帧网格 / 时间码 / 段运算（纯逻辑 + co-location 测试）
└── native/
    ├── mod.rs           # 命令入口
    ├── core.rs          # 解析 / 下载 ffmpeg
    ├── args.rs          # 预设 → argv
    ├── job.rs           # 任务 / 进度 / 取消
    ├── clip.rs          # 剪辑窗口 / 元数据 / 时间轴 / 静音分析
    ├── clip_export.rs   # 逐段重编码 + concat 导出
    └── media_server.rs  # 本地 HTTP Range 预览服务
```

## 剪辑（独立剪辑窗口）

轻量时间线剪辑：**素材自动上轨、删段涟漪、拖拽排序、逐段倍速**。单轨、键盘 + 拖拽主导，
不是全功能 NLE——三大核心操作「删除片段 / 移动片段 / 倍速」做到极致顺手。

**模型**：`TimelineSegment[]`（源素材的一段实例：源帧网格入出点 + 倍速）是成片唯一真相源，
**顺序即播放序**；成片时长 = Σ(段源时长 / 倍速)（虚拟时间线）。素材载入即整段上轨（每源一段
[0, 全长]）；删除/切分/排序都是列表变换——**涟漪前移由列表语义天然承载**，无独立轨道状态。
预览为虚拟时间线播放：跨段自动切源、`playbackRate` = 段倍速（监看即所得；段边界有 ~百毫秒
切源间隙）。所有编辑入口统一走 `syncAfterEdit` 收口（钳制播放头 + 重装载 + 续播）。

**帧精确呈现**（两处刀口纪律，源都是「双精度秒 × 帧网格」的舍入）：

- **显示 seek 永远落在帧呈现窗口内部**（`clip-logic.ts::displaySeekUs`：网格点 + 半帧，
  出点回钳到末帧窗口中点），不贴帧 PTS 边界——非整微秒帧网格（30000/1001、30/24fps）的
  帧点经 `sourceUs / 1e6` 换算会落在真实 PTS 之下 ε，播放器呈现的是**前一帧**：逐帧步进
  「按一下不走、再按跳两帧」、seek 到刀口呈现已删帧都是它
- **双解码槽轮换（live / stage）**：两个 `<video>` 叠层承载虚拟时间线——live 可见、
  驱动播放头；stage 隐藏（opacity，不用 display——保留解码优先级）**预滚**下一段
  （seek 到入点后 2ms 停驻，首帧已解码在层上，NLE「decoder preroll」的 web 同构）。
  到刀口：预滚就位 → **同任务换层**（换可见性 + 原速率续播，零 seek 等待零定格）；
  未就位 → 退回「先停表再冷切」保底；末段停在成片终点。预滚失败的后果只是回退
  冷切，不构成正确性依赖
- **刀口纪律（两条路径共同的不变量）**：媒体时钟永不越过段出点。rVFC
  （`requestVideoFrameCallback`）在末帧**呈现时**触发切换（比删除帧（PTS = 出点）
  的呈现早整整一个帧槽；rAF 侧保留「末帧窗口过半即切」兜底），切换先 `pause()` 停表
  （或换层到已停在入点的预滚槽）——出点后的帧物理上无法上屏。不能在播放中直接对
  `<video>` 赋 `currentTime`：引擎（AVPlayer）异步接住 seek 之前时钟仍在走，会越过
  刀口把已删帧送上屏，播放中的前跳甚至按就近关键帧落位（关键点常在被删区间内）。
  播放态下的一切 seek（步进/方向键/擦洗）同纪律：先 pause，`seeked` 落位后续播
  （擦洗会话中途不续播，抬手统一续）；rVFC 守卫链经 token 作废在飞回调（`load()` /
  换层 / 冷切）

### 形态

独立 `WebviewWindow`（`clip.html` 入口，1180×780 内容区、最小 900×600、可缩放、
**overlay 标题栏**——`TitleBarStyle::Overlay` + `hidden_title`：系统红绿灯悬浮在内容左上、
标题文本隐藏、无标题栏色条，内容顶到窗口顶部；头部 32px 红绿灯安全区兼作拖动区
（空白 mousedown → `startDragging`，权限经 `capabilities/clip.json`）；
appearance 跟随应用主题：创建时 `apply_cached_appearance` 补设 + 全局 `set_window_appearance`
覆盖所有窗口），不受主窗 720 宽与 auto 高度约束。**激活策略随窗口生命周期切换**：
开窗时 `setActivationPolicy(Regular)`——Accessory（LSUIElement）应用的窗口在台前调度中是
常驻浮层（不进 stage、切走不收起、悬浮其它应用上方），Regular 让剪辑窗拥有独立 stage /
Dock 图标 / ⌘Tab 归属， behave like 普通文档应用；关窗（Destroyed）还原 Accessory 回到
启动器形态（Dock 图标消失）。dev 裸二进制无 bundle 图标，切 Regular **之后**以
`setApplicationIconImage` 补设编译期内置的应用图标（顺序实测敏感：Accessory 态下先设会被
策略切换时的 Dock tile 重初始化抹掉、Dock 图标消失，故必须后设）（prod 有 bundle 不受影响）。注意该
API 仅作用于 **Dock tile**（Apple 文档明示）；台前调度缩略图角标读 LaunchServices 注册的
bundle 官方图标，dev 裸二进制无 bundle 可查、角标显示通用文件图标属**预期边界**（无公开
API 可为运行中的无 bundle 进程补注册 LS 图标），勿再排查。间距两档：模块 12
（`px-3` 外距 + `pb-3` 行距 + `gap-3` 卡距 + 卡内 `p-3` + 条栏 `px-3`）、元素 8
（`gap-2`：按钮组 / 片段卡间隙与卡内边距（横纵同值）/ 卡内元素；kbd 微簇 6）。工作台布局：
**无顶栏**（顶部仅 32px 红绿灯安全区拖动带，素材信息在素材页签、导出在底栏）→
监视器一体卡（近黑媒体井 + 内嵌传输条：ghost 24px 步进/播放（accent 着色）/时间码/
去停顿/入出点 chip——全部 24px 等高基线；传输键簇（步进/播放/步进）绝对居中，左右
flex-1 分置时间码与打点）→ 右侧玻璃片段面板——**检查器定位，双页签（片段 / 素材，页签 =
BaseButton default 变体（soft-chip 带框），选中 = `!text-accent` 文本着色（ui-active 的 11%
accent 填充实测判读为灰底过素，定稿文本着色），计数为浅色纯文本无底框）**：
片段页 = 段卡列表（圆角 + 默认细边框、选中 accent 边框、**双行信息**——关键行 =
序号 + 源区间（源 < 1h 剥前导 00: 时段）+ 成片时长（右缘不截断）、
次行 = 源名 + 倍速标记（≠1 时 accent）；序号即拖拽把手（合二为一：fill-4 底框、min-width+padding
自适应长数字；**卡身 hover 时序号淡出、拖拽把手图标浮现**——交叉淡入盒型恒定，按下拖动排序）；
**播放进度底**——播放头所在段卡背景以 accent 浅染按段内进度填充；选中态仅 accent
边框无填充底色；**拖拽排序仅经序号把手**——整卡拖拽易误选文本；倍速/删除入口在底部
常驻操作栏（见下「通用组件消费」））；选中/
当前播放段联动高亮 + 滚动跟随，操作主入口在时间轴，面板兼作窄段的稳定命中兜底；
素材页 = 源视频行（自绘双行，与段卡同构无图标——BaseListItem 的 36px 图标框在
无真缩略图时是凭空占位，已弃用：首行 = 文件名 + 时长，次行 = 分辨率·帧率·体积 +
在轨段数（元数据常态远短于行宽，不产生省略号）；点击跳到该源首个段实例并选中——不切页签，监视器即时响应；
段已剪空置灰「未上轨」；当前播放源 accent 边框高亮）；**无总计底栏**——成片
时长即传输条 total，不重复承载 → Canvas 单轨时间轴 120px（时间标尺擦洗带 + 轨道槽
（段块圆角细边框、选中 accent 边框、点选/拖拽排序）+ I/O 区间浅染 + accent 播放头手柄 +
深色实底时间码气泡（悬停与擦洗均跟随指针，轨道区附段信息行））→ 状态栏（左：去停顿
审阅 / 导出进度结果 / 快捷键提示——时间码与入出点由传输条承载，底栏不重复；**右缘：
导出入口**（原顶栏迁入，进行中变取消））。

**通用组件消费**：面板底部常驻操作栏 = 纯图标控件——倍速为 `BaseSelect`
**触发器插槽**（新增 `#trigger` slot：任意内容可作触发器——纯图标/图标+值/自定义
chip（箭头等装饰由消费者自决，纯图标场景配 `!w-7 !px-0 flex-center` 与 BaseButton 图标钮同盒型（方形），soft-chip 边框已示意控件身份、无需箭头），
默认触发器（选中值文本 + 箭头）回落；自定义触发器过窄时下拉不贴合宽度
（matchWidth 关闭）；键盘 ↑↓/Enter/Space/Esc、hover 跟随、外点关闭、浮层定位与文本
模式**完全一致**——曾以 `icon` prop 收窄实现后按通用性要求改为插槽，更早的手搓
Teleport 弹窗因 hover/键盘行为不一致被整体替换）；删除为
`BaseButton` 图标钮（default 变体 soft-chip，与 select 触发器同族观感）；均作用于选中段
（= 播放头段，无段禁用）。历史方案均已废弃：段卡内展开预设行 → ⋯ 二级菜单
（Teleport + BaseDropdownItems + backdrop）→ 手搓图标弹窗 → BaseSelect 图标模式；
段卡从 `<button>` 转 `<div>`（button 内不可嵌交互控件）。其余 BaseButton / BaseEmptyState /
BaseProgress / `formatBytes`（utils）均复用框架件；加载态副标题多素材显示
「N 个素材」计数。**不做 panel 化**：
NonactivatingPanel 形态虽能在 app 未激活时收键盘，但 tao 销毁自定义 NSWindow 子类时会抛
ObjC 异常直接 abort 进程（红绿灯/Esc 关闭都触发）。

焦点链路：先隐藏主窗（焦点还给原 app）→ 创建剪辑窗口 → 主线程 `activate_app`
（`activateIgnoringOtherApps` + `NSRunningApplication.activateWithOptions(ActivateAllWindows)`，
macOS 14+ 前者可能被静默忽略）→ `makeKeyWindow` + `setFocus`；前端加载后再
`getCurrentWebview().setFocus()` 多拍重试（依赖 `capabilities/clip.json` 授权
`core:webview:allow-set-webview-focus` 与事件监听）。反向（先 open 后 hide）会被主窗 hide 的
`restore_captured` 抢走焦点。每次 `bootstrap`（新输入）重置播放头/打点/段列表/导出结果，
窗口复用不残留上次会话（单调序号防挂载 take 与复用窗口输入事件交叠）。入口：video 面板
末尾「高级」组的「剪辑」行（已选视频直接带入，否则先选文件）；另有截图扩展录屏结束后经
同页 CustomEvent `video-clip-open` 直接投递录制文件（`extensions/video/index.ts` setup
监听，不经面板中转，见 [screenshot.md](screenshot.md)「录屏」）；交接前查
`video_core_status`，FFmpeg 核心未就绪时只 toast 提示（剪辑窗 bootstrap 依赖 ffprobe，
开窗必然失败）。

### 预览

`<video>` + **本地 HTTP Range 媒体服务**（`native/media_server.rs`，axum `127.0.0.1:随机端口`，
token→路径映射、单区间 206、流式返回）。不用 asset protocol：macOS/WKWebView 自定义 scheme
的媒体 Range 有 1MB 上限与花屏/请求风暴等已知问题（tauri#4135/#7355/#10426），且 responder
无法流式。配套改动：CSP `media-src 'self' asset: blob: http://127.0.0.1:*` +
`img-src` 放行 `http://asset.localhost`（dev 模式 asset 协议域名）、Info.plist
`NSAllowsLocalNetworking`。

可直放：H.264 mp4/mov、HEVC `hvc1`、ProRes、webm VP8/VP9；不可直放（MKV、`hev1` tag、AV1）
明确提示「需先重新封装或转码」，不静默失败。时间轴缩略图走 ffmpeg 一次进程 `fps+scale+tile`
出横向精灵图（缓存于 `$TEMP/voidnix/clip/`，经 asset 协议加载）。

### 时间轴

单个 Canvas 绘制**成片虚拟时间线**：段块（宽 ∝ 虚拟时长，缩略图按源时间采样铺块、源边界
分隔线、≠1 倍速角标、选中描边）+ 播放头 + 打点区间（warning 浅染）+ 去停顿审阅高亮 +
时间刻度；DOM 侧段列表为检查器（选中/播放段联动，操作主入口在时间轴）。播放头跟随
`requestAnimationFrame` 读 `video.currentTime` 按段倍速换算到虚拟时间线；打点/切分全部
量化到源帧网格（整数微秒 + 有理数 fps：`fpsNum/fpsDen`，杜绝浮点积累漂移）。

**直接操纵（点击即定位，选中派生）**：全幅点击（标尺带 / 轨道区 / 播放头手柄拖动）都把
播放头移到点击位置（擦洗时实底气泡跟随指针，虚线与播放头重合故省略）；**选中段 =
播放头所在段（纯派生 computed，无独立选中状态）**——指针在哪哪段选中，⌫/`-`/`=`/
⌘↑↓/Tab 全部经 seek 驱动选中，选中态的全部维护代码（索引修正/重置/守卫）随之删除。
轨道区按住拖过 4px 阈值 = 拖拽排序（指针按下 `preventDefault()` 防选择；拖中**幽灵块
跟随指针**居中钳制轨道内、原块原地半透明、accent 插入线随最近段边界，drop
复用 `moveSegment` 纯列表变换；原位边界不画线，防「拖了没反应」）。绘制与
命中共用 `segLayouts` 累计几何。**窄段纪律**：宽度 < 8px 的段块（去停顿碎段常成串）命中
按 ±8px 窗口内中心就近（宽块只经包含命中参与，防其中心抢走贴邻窄块），选中描边取最小
可视宽度（居中扩展压邻块、与轨道槽同心圆角），悬停/擦洗气泡在轨道区附「序号·源名」辨认
行，Tab 段间跳转兜底——窄段的完整退路 = 键盘 + 片段面板检查器。段块默认圆角细边框
（divider 色、块自身周界裁剪，取代旧的段间分隔线），选中的块画 accent 边框（覆盖层
之后绘制，压过打点/审阅浅染）。

**段卡拖拽（面板内）**：仅经序号把手触发（hover 时序号换拖拽图标；pointerdown `preventDefault()` 根断
文本选择——user-select:none 之外 WebKit 拖拽手势仍可拉出选区；4px 阈值）；拖拽中
**幽灵卡跟随指针**（Teleport 单行卡影 + accent 边框，原卡原地半透明）+ **插入线落
在两项之间**（按各卡中点二分求边界 0..n，含末尾追加位；accent 满列表宽，与时间轴
插入线同语义，原位边界不画线），拖后抑制一次 click，与时间轴段块同范式——HTML5 DnD 在 WKWebView 的
button 上不可靠（draggable 属性不发 dragstart，`-webkit-user-drag` 修复亦不稳定），
实测仍失效后弃用。drop 语义 = 插到目标卡**之前**（与指示线一致）：下向拖拽时目标在
移除后前移一位（`to = target - 1`）；原位目标（自身 / 紧邻下一位）不画指示线。

### 键盘与指针

| 键                   | 动作                                                   | 键             | 动作                                                                  |
| -------------------- | ------------------------------------------------------ | -------------- | --------------------------------------------------------------------- |
| Space / L            | 播放/暂停（跨段续播，rate = 段倍速）                   | I / O / X      | 打点入点 / 出点 / 清除（成片时间线）                                  |
| K                    | 暂停                                                   | ← / →          | ±1 帧（Shift ±1s、⌘ ±5s）                                             |
| J                    | 后退 0.5s                                              | ⌫ / Delete     | **统一删除**（优先打点区间，否则选中段，涟漪）                        |
| S                    | 播放头处**切分**（段一分为二）                         | 顶带双击       | 窗口缩放/还原（startDragging 吞 click/dblclick，手动双击检测接管）    |
| ⌘↑ / ⌘↓              | 移动选中段（与拖拽同效）                               | 段块/段卡拖拽  | **排序**（段块：最近段边界 accent 插入线；段卡：drop 目标顶部指示线） |
| Tab                  | 段间跳转（Shift 反向；选中随播放头派生）               | ⌘D             | 一键去停顿（源级检测，审阅式）                                        |
| 底部操作栏 / `-` `=` | 操作栏倍速预设 + 删除（选中段）/ 预设档循环（0.25–4×） | ⌘Enter         | 导出                                                                  |
| ⌘W                   | 关闭窗口（Esc 不关窗，只收菜单/审阅）                  | 时间轴全幅点击 | 播放头定位（选中=所在段）；轨道区按住拖 = 排序                        |

### 一键去停顿

ffmpeg `silencedetect=noise=-35dB:d=0.3`（默认参数对口播过宽；`-vn` 跳过视频解码，
分析只消费音轨），解析静音区间后映射到虚拟时间线高亮，状态栏显示「将删除 N 段 · Xs
（保留气口 150ms）」；Enter 经 `applySourceCuts` 把废段从该源的**所有段实例**中减除
（子段继承倍速、涟漪前移）、Esc 取消。非破坏：源文件不变，只改时间线段列表。

### 导出

导出计划 = 时间线段原样（含倍速），交给 `native/clip_export.rs`（复用 `job.rs` 的
ffmpeg 外壳：spawn/进度/取消/超时）：

- **精确档（默认）**：逐段 `-ss/-to` + `libx264 -preset veryfast -crf 18` 重编码（实测 1.03s/10s
  1080p，快于 VT），统一规格（以第一段显示尺寸为基准 `scale+pad+setsar`、帧率取第一段、
  音轨混合时整片去音频），再 `concat -c copy` 合并（同规格流拷贝安全）。**倍速**：
  视频 `setpts=PTS/{speed}` + `-r` 按目标帧率重采样（2x 丢帧 / 0.5x 补帧，输出恒定帧率
  保 concat 规格）；音频 `atempo` 链保音高（单实例 [0.5,2]，越界链式分解）。进度与
  `Done` 时长按虚拟时长（= out_time 同侧）。段文案 Channel 事件 started/progress/done/error。
- **快速档**：单段 `-c copy`（输入侧 `-ss`，mp4/mov 保留 edit list，画面精确到请求帧、43ms）；
  多段带裁剪的 copy 段不能直接 concat（隐藏 GOP 前缀显形），故快速档仅单段（命令已支持
  `mode: "copy"`，UI 未暴露）。
- 输出默认源文件旁 `{stem}-cut.mp4`（冲突加 `-1` 后缀）；完成 toast + 状态栏「在 Finder 显示」。
- 错误回报只经 Channel `error` 事件（命令返回 `Ok`，与 agent_run 同范式，防 channel 回调与
  invoke reject 双弹 toast）；取消经前端 `cancelRequested` 标志区分「已取消导出」文案。
- 窗口销毁（Rust `Destroyed` 钩子，权威取消点——前端卸载回调随 webview 销毁不可靠）自动取消
  导出、停媒体服务、清理 `$TEMP/voidnix/clip/`（精灵图 + 导出工作目录）；启动期
  `storage::cleanup_all_voidnix_temps` 兜底崩溃残留。

### 已知边界

- HDR/10bit/多音轨：copy 档可保留、精确档转 SDR 并只保留首条音轨（v1 不做 HDR 精确保留）。
- VFR 素材按平均帧率量化（打点可能轻微对齐到最近帧）；rotation 在 ffmpeg 侧自动应用。
- finder-ext 投递到剪辑未接线（当前经 video 面板选文件进入）。
- 快速档（单段 copy）与拼接预览代理未接 UI；跨段连续预览需导出后查看。

### 命令

- `video_clip_open` / `video_clip_close` / `video_clip_take_inputs`（窗口生命周期 + 待处理输入）
- `video_clip_probe`（元数据：fps/时长/旋转/音频/可直放性）
- `video_clip_timeline`（缩略图精灵图）
- `video_clip_analyze`（静音区间）
- `video_clip_export`（`Channel<ClipExportEvent>`）/ `video_clip_cancel`
- `video_media_open` / `video_media_close`（本地媒体预览服务）
