# Requirements Document

## Introduction

Voidnix 是基于 Tauri 2 + WKWebView 的 macOS 启动器，使用模式与 Spotlight / Raycast
一致：用户随时按全局快捷键唤起，按 Esc 或失焦立即隐藏。Tauri 默认未对这种"频繁
显示/隐藏"的形态做任何 WKWebView 行为驯化，导致以下可观察问题：

1. 唤起瞬间偶发显示上一次会话的陈旧内容或闪一帧白底；
2. 失焦隐藏（应用整体被 hide）后，前端的 `requestAnimationFrame`、CSS 动画、
   `setTimeout` 被 WebKit 节流，再次唤起时 UI 状态不及时；
3. 主窗口在"列表 ↔ 扩展面板 ↔ 设置"切换时尺寸变化，过程中出现一两帧白边或抖动；
4. 含 emoji 的视图（剪贴板、聊天）首次出现 emoji 时存在明显的字体回退停顿。

本特性按 Raycast 公开披露的 WKWebView 驯化技巧，在 Voidnix 的 macOS 原生外壳
（`src-tauri/native/` + `mac_utils.rs` + 主窗口 setup 路径）上实现一组针对启动器
形态的呈现层优化，使主窗口在唤起、隐藏、尺寸变化、emoji 渲染等场景下达到原生
启动器水平的视觉表现。本特性只覆盖与"窗口呈现 + WKWebView 行为驯化"直接相关的
能力，不包含模块懒加载、文件索引、类型化 IPC、原生 popover 等后续 spec 的范围。

## Glossary

- **Voidnix_Shell**：Voidnix 的 macOS 原生外壳，包含 NSWindow、其 contentView、以及
  Tauri 注入的 WKWebView 子视图，由 Rust 端在 `src-tauri/src/lib.rs` 的 setup 阶段
  创建并配置。
- **Main_Window**：Tauri label 为 `main` 的主窗口（启动器主入口）。本特性只对
  Main_Window 生效，不影响 `screenshot` 窗口。
- **Web_View**：Main_Window 的 NSWindow.contentView 内挂载的 WKWebView 实例。
- **Presentation_Coordinator**：本特性新增的原生组件，负责在唤起 Main_Window 时
  等待 Web_View 完成首帧呈现后再让窗口进入屏幕可见状态。
- **Throttling_Suppressor**：本特性新增的原生组件，负责让 Web_View 在 Main_Window
  被视觉隐藏期间仍被 WebKit 视为"可见"，以避免 rAF / 计时器 / CSS 动画被节流。
- **Frame_Animator**：本特性新增的原生组件，负责接管 NSWindow 的尺寸变化动画，
  用 Core Animation 隐式动画替代 AppKit 默认的同步 resize 动画。
- **Webview_Frame_Pin**：本特性新增的策略，使 Web_View 的 frame 始终保持等于
  Main_Window 在当前会话中可能达到的最大尺寸，由 NSWindow 自身在外侧进行视觉裁剪。
- **Emoji_Warmer**：本特性新增的原生组件，负责在应用启动后预热系统 emoji 字体，
  使首次出现 emoji 时不出现可观察的字体回退停顿。
- **Tuning_Toggle**：本特性新增的开关，允许在开发期通过环境变量整体关闭驯化逻辑，
  以对照默认 Tauri 行为。
- **Stale_Frame**：Web_View 上一次进入隐藏前的最后一帧像素，被复用到下一次唤起
  且与当前应有 UI 状态不一致的现象。
- **Apparent_White_Gap**：Main_Window 在尺寸变化或唤起过程中可见的、非 UI 内容的
  白色或透明矩形条。

## Requirements

### Requirement 1: 唤起首帧无闪烁与陈旧内容

**User Story:** 作为 Voidnix 用户，我希望按下全局快捷键唤起窗口时立即看到最新的、
应该展示的 UI，而不会先看到上一次会话残留的内容或一帧空白闪烁，这样我能在不打断
心流的情况下立刻输入指令。

#### Acceptance Criteria

1. WHEN Main_Window 从隐藏状态被请求显示，THE Presentation_Coordinator SHALL 先令
   Web_View 完成至少一次呈现更新，再令 Main_Window 在屏幕上变得视觉可见。
2. WHEN Presentation_Coordinator 等待 Web_View 完成呈现更新的耗时达到 80ms，
   THE Presentation_Coordinator SHALL 终止等待并立即令 Main_Window 视觉可见。
3. WHEN Main_Window 完成一次唤起，THE Voidnix_Shell SHALL 不在屏幕上呈现 Stale_Frame。
4. WHEN Main_Window 完成一次唤起，THE Voidnix_Shell SHALL 不在 Web_View 区域呈现
   Apparent_White_Gap。
5. THE Presentation_Coordinator SHALL 仅作用于 Main_Window，并保持 `screenshot` 窗口
   现有的 `setFrame_display(_, true) + alpha=0 + orderFrontRegardless` 行为不变。
6. IF Presentation_Coordinator 因等待呈现更新超过 80ms 而提前令 Main_Window 视觉
   可见，THEN THE Voidnix_Shell SHALL 在该次唤起中显示一个空内容或加载占位
   （透明背景配合骨架/进度提示），而不是 Stale_Frame，且 SHALL 在 Web_View 完成下
   一次呈现更新后立即将 UI 切换为最新内容。

### Requirement 2: 隐藏期间保留前端时间驱动状态

**User Story:** 作为 Voidnix 用户，我希望窗口在隐藏期间仍能正常推进倒计时、轮询、
预取动画等后台逻辑，再次唤起时不会看到一段卡顿后才追上的过渡，这样我能信赖启动器
随开随用。

#### Acceptance Criteria

1. WHEN 前端调用 `hide_window`，THE Voidnix_Shell SHALL 在 100ms 内将 NSWindow 的
   alphaValue 由当前值降为 0，并维持窗口在活动屏幕上的原坐标与原尺寸不变，且不
   调用 NSWindow.orderOut 或 NSApplication.hide。
2. WHILE Main_Window 处于"视觉不可见"状态，THE Throttling_Suppressor SHALL 令
   NSWindow 的 windowOcclusionDetectionEnabled 保持为 false。
3. WHILE Main_Window 处于"视觉不可见"状态，THE Web_View SHALL 持续派发
   `requestAnimationFrame` 回调并推进 CSS 动画，使 `requestAnimationFrame` 的平均
   触发频率不低于 30 次每秒，且相邻两次回调间隔不超过 100ms。
4. WHILE Main_Window 处于"视觉不可见"状态，THE Web_View SHALL 触发 `setTimeout`
   与 `setInterval` 回调，使每次回调相对于设定时刻的延迟不超过 50ms。
5. WHILE Main_Window 处于"视觉不可见"状态，THE Voidnix_Shell SHALL 通过将 NSWindow
   的 ignoresMouseEvents 设为 true（或等价机制），令窗口不响应鼠标点击、滚动、
   悬停以及拖拽事件。
6. WHILE Main_Window 处于"视觉不可见"状态，THE Voidnix_Shell SHALL 不在 Mission
   Control 缩略图、Cmd+Tab 应用切换器以及 Dock 中出现自身条目。
7. WHEN 前端请求将 Main_Window 从"视觉不可见"恢复为视觉可见，
   THE Throttling_Suppressor SHALL 先向 Web_View 投递一次 `requestAnimationFrame`
   信号，并在该信号投递之后、不晚于 16ms 内将 NSWindow 的 alphaValue 由 0 提升至 1。
8. IF 当前 macOS 版本不允许在不调用 orderOut 的情况下保持
   windowOcclusionDetectionEnabled 为 false，THEN THE Throttling_Suppressor SHALL
   改为调用 NSWindow.orderOut 完成隐藏，并向诊断日志输出一条记录，内容指明触发
   回退的 macOS 版本与受限 API 名称。

### Requirement 3: 尺寸切换无白边与无停帧

**User Story:** 作为 Voidnix 用户，我希望主窗口在列表 ↔ 扩展面板 ↔ 设置之间切换尺寸
时是连续平滑的，不会先出现一段白边再被填满，也不会在动画过程中卡住一两帧，这样
界面切换感觉与原生应用一致。

#### Acceptance Criteria

1. THE Webview_Frame_Pin SHALL 令 Web_View 的 frame 在整个会话中等于 Main_Window
   在该会话中可能达到的最大尺寸，并由 NSWindow 在外侧进行视觉裁剪。
2. WHEN Main_Window 接收到尺寸变化请求（包括从前端 invoke 的 set_size、模块切换
   触发的尺寸调整），THE Frame_Animator SHALL 用 Core Animation 隐式动画替代
   NSWindow 默认的同步 resize 动画。
3. WHILE Main_Window 处于尺寸切换动画过程中，THE Web_View SHALL 持续提交渲染帧。
4. WHEN Main_Window 完成一次尺寸切换，THE Voidnix_Shell SHALL 不在窗口圆角内侧
   呈现 Apparent_White_Gap。
5. WHEN Main_Window 完成一次尺寸切换，THE Voidnix_Shell SHALL 保持 contentView
   的圆角与 masksToBounds 设置不被尺寸变化重置。
6. WHEN 同一会话中 Web_View 的所需最大尺寸提升（例如启用了原本未启用的扩展面板），
   THE Webview_Frame_Pin SHALL 一次性扩大 Web_View 的 frame 至新最大尺寸，
   并保持后续尺寸变化只动 NSWindow。

### Requirement 4: Emoji 首次渲染无可观察停顿

**User Story:** 作为 Voidnix 用户，我希望在剪贴板历史、聊天回复等含 emoji 的视图中，
第一次出现 emoji 时不会卡一下，这样含表情的内容也能随打随显。

#### Acceptance Criteria

1. WHEN 应用完成启动并令 Main_Window 首次进入"视觉不可见"待命状态，
   THE Emoji_Warmer SHALL 在后台触发系统 emoji 字体的一次预加载。
2. WHEN Emoji_Warmer 执行预加载，THE Emoji_Warmer SHALL 不阻塞主线程超过 8ms。
3. WHEN 含 emoji 的文本第一次出现在 Main_Window 中，THE Web_View SHALL 在该帧内
   完成 emoji 字形的渲染，不出现以后续帧才补绘 emoji 字形的现象。
4. IF Emoji_Warmer 在当前 macOS 版本上不可用或预加载失败，THEN THE Voidnix_Shell
   SHALL 跳过预加载并向日志 target `webkit_tuning` 输出原因，而不阻塞应用启动。

### Requirement 5: macOS 版本兼容与回退

**User Story:** 作为 Voidnix 用户，我希望驯化逻辑在我手上的 macOS 版本（包括较新
的 macOS 26 Tahoe / Liquid Glass）都能稳定生效，而不是在新版本上崩溃或退化，这样
升级系统不会让启动器变差。

#### Acceptance Criteria

1. THE Voidnix_Shell SHALL 在 macOS 13、macOS 14、macOS 15、macOS 26 上加载并启用
   驯化逻辑，且不引发崩溃。
2. IF 某项驯化所依赖的私有或 SPI 方法（例如 `_doAfterNextPresentationUpdate:`）
   在当前 macOS 版本上不可用或返回错误，THEN THE Voidnix_Shell SHALL 回退到 Tauri
   默认行为执行该步骤，并向日志 target `webkit_tuning` 输出已回退的步骤名与原因。
3. WHEN 驯化中任一步骤抛出 Objective-C 异常或 selector 不存在，
   THE Voidnix_Shell SHALL 拦截该异常并继续完成剩余的驯化步骤与正常的窗口生命周期。
4. THE Voidnix_Shell SHALL 通过运行时 selector 探测而不是版本号字符串来判定 SPI
   可用性。

### Requirement 6: 资源占用约束

**User Story:** 作为 Voidnix 用户，我希望驯化逻辑不会让启动器在后台持续吃 CPU 或
膨胀内存，这样启动器可以一直驻留也不影响电池续航。

#### Acceptance Criteria

1. WHILE Main_Window 处于"视觉不可见"状态且前端无活跃定时器，THE Voidnix_Shell
   SHALL 不引入新的、持续占用 CPU 的原生轮询线程。
2. THE Voidnix_Shell SHALL 不因本特性的常驻原生组件令应用主进程的常驻内存
   （RSS）相对关闭 Tuning_Toggle 时增加超过 10MB（启动后稳定 60 秒采样）。
3. WHILE Main_Window 处于"视觉可见"状态且前端处于 idle，THE Frame_Animator SHALL
   不持续提交不必要的 Core Animation 事务。
4. IF 本特性新增的任一原生组件需要监听 NSNotification 或 KVO，THEN THE
   Voidnix_Shell SHALL 在窗口或应用销毁时移除对应监听，不残留观察者。

### Requirement 7: 调试开关与可观测性

**User Story:** 作为 Voidnix 维护者，我希望能在开发期一键关闭驯化逻辑、对比 Tauri
默认行为，并能在日志里看到每一步驯化是否生效，这样定位回归问题不靠猜。

#### Acceptance Criteria

1. WHERE 进程启动时读取的环境变量 `VOIDNIX_DISABLE_WEBKIT_TUNING` 的字符串值精确
   等于 `1`，THE Voidnix_Shell SHALL 跳过 Presentation_Coordinator、
   Throttling_Suppressor、Frame_Animator、Webview_Frame_Pin、Emoji_Warmer 的全部
   初始化代码路径，并对 Main_Window 的显示、隐藏、尺寸变化与字体加载完全沿用
   Tauri 默认行为，不再调用任何驯化组件提供的接口。
2. WHERE 进程启动时读取的环境变量 `VOIDNIX_DISABLE_WEBKIT_TUNING` 未设置或其字
   符串值不等于 `1`，THE Voidnix_Shell SHALL 按正常流程初始化
   Presentation_Coordinator、Throttling_Suppressor、Frame_Animator、
   Webview_Frame_Pin、Emoji_Warmer 并启用其对 Main_Window 的处理。
3. WHEN Presentation_Coordinator、Throttling_Suppressor、Frame_Animator、
   Webview_Frame_Pin、Emoji_Warmer 中任一组件的初始化过程结束，THE Voidnix_Shell
   SHALL 向日志 target `webkit_tuning` 输出恰好一条记录，该记录包含该组件的名称，
   以及取值为 `启用`、`已回退`、`已禁用` 三者之一的生效状态字段。
4. WHEN Main_Window 完成一次显示、一次隐藏或一次尺寸切换中的任一事件，
   THE Voidnix_Shell SHALL 在该事件的回调返回前向日志 target `webkit_tuning` 输出
   一条诊断记录，记录该事件名称与本次所经历的驯化步骤名列表，且单条记录的写入
   耗时不超过 10 毫秒。
5. WHILE 当前为 release 构建且进程环境变量 `RUST_LOG` 未显式包含
   `webkit_tuning=debug` 或更详细级别（`trace`），THE Voidnix_Shell SHALL 不向
   stdout 与 stderr 输出 target 为 `webkit_tuning` 的任何日志记录。
6. WHERE 进程环境变量 `RUST_LOG` 显式包含 `webkit_tuning=debug` 或更详细级别
   （`trace`），THE Voidnix_Shell SHALL 将 target 为 `webkit_tuning` 的日志记录
   输出到 stderr。
