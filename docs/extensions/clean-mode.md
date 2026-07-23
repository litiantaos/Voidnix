# clean-mode（清洁模式）

全屏黑窗 + 键鼠锁定，长按鼠标 / 触控板 2s 退出。用于清洁屏幕 / 键盘时防止误触。

## 退出可靠性

退出机制是核心安全点：

- **长按左键 2s**：`CGEventTap callback` + `NSView mouseDown` 双路检测（共用 `LEFT_DOWN_AT` 原子时间戳）。tap 正常时 callback 设值；tap 静默失效（inert）时事件穿透到 NSView 由 mouseDown 设值。任一路失效另一路兜底。
- **退出动作 = 主线程 `disable_clean_mode`**：poll 线程检测到长按达标后，通过 `APP_HANDLE.run_on_main_thread` 派发优雅关闭（关窗 / 停 tap / 解冻光标，**app 继续运行**），并 `emit("clean-mode-exit")` 通知前端同步状态。

## tap 可靠性

tap 注册到 **main run loop + `kCFRunLoopCommonModes`**（参照 Hammerspoon / Scroll Reverser，不挂独立线程——`kCFRunLoopDefaultMode` 会在 event-tracking / modal 切换时饿死 source 导致 tap 被判定慢而静默禁用）。poll 线程兼作 watchdog，每 2s 检查 `CGEventTapIsEnabled`，被禁用则 `CGEventTapEnable` 重启。

## 光标冻结

- `CGAssociateMouseAndMouseCursorPosition(0)`：解除鼠标硬件与光标位移的关联
- poll 线程周期 `CGWarpMouseCursorPosition`：钉回主屏中心（CGAssociate 兜底）
- `CGDisplayHideCursor`：隐藏指针
- 吞 `MouseMoved` 会被系统强制重新关联光标，必须放行，靠 CGAssociate 冻结

## 修饰键处理

修饰键（Shift / Cmd / Option / Control）通过 `CGEventSetFlags(event, 0)` 清零 flags 后放行——吞掉 FlagsChanged 不能阻止 HID 驱动更新 modifier state，清零让系统认为无修饰键按下。

## 已知限制

macOS 安全模型固有约束——Apple 未提供禁用键盘的官方 API，CGEventTap 本意是给输入法 / 无障碍 / 重映射用的：

- **系统级多指手势**（三/四指滑动切 Space、四指捏合 Launchpad / Mission Control）由 Dock / WindowServer 在底层处理，mask 未含手势位（加入会触发权限弹窗 / 破坏系统手势），可能不被屏蔽
- **固件级按键**（fn 切换功能键行 / 语音听写键 / Caps Lock LED）由键盘固件直接处理，不经过 HID 事件系统，CGEventTap 拦不住：
  - 功能键（F1-F12）本身已被 tap 吞掉，fn 切换解释方式不产生实际效果
  - Caps Lock 输入法切换功能已被 `CGEventSetFlags` 清零拦截，仅 LED 灯亮（固件控制）
- 显示器热插拔期间不自动补窗
