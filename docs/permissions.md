# TCC 权限与授权会话

AGENTS.md「发布管道与代码签名」节记录公证分流结论与防弹窗纪律；本文档承载 `platform/permission.rs` 授权会话的实现细节（时序、主窗避让、重启弹窗检测、前端钉住链路）与 `platform/perm_drag.rs` 拖拽指引浮窗。

## 两档现状（macOS 15+）

未公证应用（Apple Development 证书签名——未购 Apple Developer Program 时的唯一选择，或 adhoc）经 API 请求（`AXIsProcessTrustedWithOptions` / `CGRequestScreenCaptureAccess`）写入的辅助功能/屏幕录制条目**无效**（开关打开也不生效，列表还被坏条目污染）；系统设置手动「+ 添加」的条目**有效**（DR 匹配正常，Apple Development 证书一年内稳定），但屏幕录制可能被周期性要求重授权。

购入 Program 后换 Developer ID Application 证书 + 配齐公证凭证，分流自动切回 API 路径，无需改代码。完全磁盘与「文件与文件夹」权限不查公证，不受此影响。

## 分流判定

`check_app_notarized` 命令（`stapler validate` 本地检测 stapled ticket + 一次性缓存）：

- 公证版：先走 API 请求（一键允许）
- 手动档（未公证三权限，或完全磁盘——无 API 请求路径）：跳过请求直达系统设置

直达设置统一走 `open_privacy_settings` **授权会话**。

## 授权会话时序（每步原语均经探针实测）

1. 主窗先降普通层级并置顶
2. `open` 面板 URL（设置窗口冷启动映射/warm 抬升自然落在主窗之上）
3. 激活兜底（bundle id 跨版本漂移——Ventura+ 为 `com.apple.systemsettings`、macOS 27 实测回落 `com.apple.systempreferences`，按序探测取命中）
4. 主窗避让到设置窗旁（`CGWindowList` bounds 无需屏幕录制权限，Quartz 坐标经主屏高翻转为 Cocoa；并排两侧优先，小屏放不下时取与设置窗重叠最小的边缘位置）
5. `open` 重发置顶（对已激活 app `activate` 是空操作，LaunchServices 重发恒能把设置窗抬到主窗之上；`orderWindow:relativeTo:` 跨 app 排序实测无效勿用）
6. 主窗常态浮动层级——present 每次重申 `NSFloatingWindowLevel`，普通层设置窗即使激活也盖不过浮动窗，会话结束复位浮动层级并前置

1s 轮询授权状态，完成或 10min 超时 emit `perm-flow` 事件。

### 重启确认弹窗（完全访问/录屏）

这两项授权须重启才对运行中进程生效——check 恒 false、授权检测不会命中，会话终点改为重启确认弹窗的取消：

- 弹窗形态双路检测：设置自持 sheet/模态窗以独立窗口呈现于窗口列表，出现 = 设置进程新窗或非零层级窗（`settings_windows` 清单差分，主窗恒 layer 0），取消 = 该窗消失；跨进程承载（`proc_pidpath` 判系统进程）= 前台稳定转出再回设置
- 取消即 emit perm-flow（收浮窗 + 解除钉住）并 makeKey 补聚焦
- 检测命中（重授权等 live 生效场景）则先 emit 收尾（不夺 key）并继续跟踪弹窗取消后补聚焦（弹窗显示晚于开关 >1s，提前夺 key 会使其降为非激活、双击才点到按钮）

### 事件与前端钉住

- 会话起点 emit `perm-session`（kind，前端据此置钉——Rust 直发的会话如 finder-ext 辅助功能引导与 `startPermGrant` 前端入口共享同一钉住/linger 链路）
- 前端（`systemStore.startPermGrant`）会话期间钉住主窗（失焦/点击外部/前台切换均不隐藏），事件到达即刷新
- 授权后钉住转入 linger：完全访问/录屏会弹系统重启确认，弹窗失焦与其关闭后的 frontmost-changed 藏窗路径须继续让位；**滑动续期**——被让位事件每次续 15s，静默 15s 解除，不随 focus 事件解除（弹窗期点击主窗/自动聚焦都会产生 focus、误解除会复活藏窗路径）
- 会话结束 Rust 复位主窗浮动层级并前置：设备控制同时 makeKey 取键盘聚焦（panel 语义不激活 NSApp）；完全访问/录屏在弹窗存续期不夺 key（否则弹窗降为非激活、双击才点到按钮），取消后补聚焦

## 拖拽指引浮窗（perm_drag.rs）

三个权限的授权会话均显示独立原生拖拽指引小窗（NonactivatingPanel + 浮动层级，悬浮于设置窗口底部外侧 8px、水平对准内容区中心（右移估值 220pt 避开左侧分类栏））：两行说明（文案含 \n 换行）+ 应用图标原生拖拽源，图标高度与两行文本高度一致（label sizeToFit 测得动态定高）。

- 显隐随会话统一驱动——`startPermGrant` / `perm-session`（Rust 直发会话）显示、`perm-flow`（完成/超时）收起，前端不再逐入口管理
- HTML5 拖拽无法跨应用携带 file URL，拖进设置列表等价点 + 添加
- 定位复用授权会话的设置窗探测（show/hide 带代数防残窗）

## dev 构建 TCC 恢复

dev 构建（adhoc 裸二进制）同属未公证档，恢复法：`tccutil reset Accessibility com.litiantao.voidnix.dev`（或 `ScreenCapture`）。
