# screenshot

区域截屏 + 标注 + OCR + 钉图 + 滚动长截图。原生 CALayer 直接渲染 CGImage（零拷贝），WKWebView 透明覆盖做交互层。

## 架构

- 截图渲染走 ObjC++ 桥（`screenshot_overlay.mm`）把 CGImage 直接设为 `CALayer.contents`，按键到显示 ~20-30ms
- 标注/选区/工具栏走 Vue（`windows/Operation.vue` + `composables/`）
- `native/` 按职责分：session（截图会话）、ocr（Vision 调用）、pin（钉图窗口）、scroll_capture（滚动长截图）、crop（裁剪）、ffi（ObjC++ 桥）、setup（启动钩子）

## 约束

- 需**屏幕录制权限**（`CGDisplayCreateImage`），未授权返回中文错误
- `CGWindowListCopyWindowInfo` 枚举可见窗口矩形（智能吸附选区）
- Vision OCR 通过 `swift -e` 执行 `VNRecognizeTextRequest`（zh-Hans/Hant/en/ja）
- Skylight `move_window_to_active_space` 跨 Space

## 数据存储

无持久化。临时文件落 `$TMPDIR/voidnix*`（picker.jpg 预览、ocr/clip/pin/scroll 中间 PNG），`cleanup_temp_files()` 启动时清残留。设置走全局 `useSettingsStore`（`screenshotSavePath`）。
