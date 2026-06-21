# screenshot

区域截屏 + 标注 + OCR/二维码识别 + 钉图 + 滚动长截图。原生 CALayer 直接渲染 CGImage（零拷贝），WKWebView 透明覆盖做交互层。

## 架构

- 截图渲染走 ObjC++ 桥（`screenshot_overlay.mm`）把 CGImage 直接设为 `CALayer.contents`，按键到显示 ~20-30ms
- 标注/选区/工具栏走 Vue（`windows/Operation.vue` + `composables/`）
- `native/` 按职责分：session（截图会话）、ocr（Vision 调用）、pin（钉图窗口）、scroll_capture（滚动长截图）、crop（裁剪）、ffi（ObjC++ 桥）、setup（启动钩子）

## 约束

- 需**屏幕录制权限**（`CGDisplayCreateImage`），未授权返回中文错误
- `CGWindowListCopyWindowInfo` 枚举可见窗口矩形（智能吸附选区）
- Vision OCR 通过 `swift -e` 执行 `VNRecognizeTextRequest` + `VNDetectBarcodesRequest`（zh-Hans/Hant/en/ja 文字 + QR/条码），一次请求同时返回文字和二维码内容（`OcrResult { text, qr }`）
- Skylight `move_window_to_active_space` 跨 Space

## 数据存储

无持久化。临时文件落 `$TMPDIR/voidnix*`（picker.jpg 预览、ocr/clip/pin/scroll 中间 PNG、voidnix-icon- 图标缓存），由 `runtime::storage::cleanup_all_voidnix_temps()` 在 `lib.rs` setup 启动期统一清理（覆盖 `voidnix_*` / `voidnix-icon-*` / `voidnix/picker.jpg` 三个前缀族）。`save_png_safely()` 提供 create_dir_all + path_guard + write 统一接口，供 save_screenshot / save_scroll_result 共用。配置通过 `extensions/screenshot/config.ts`（defineConfig 自管 `savePath`）。

## 滚动截屏拼接算法

纯像素路径（`scroll_capture.rs`）：CGWindowList 选区抓帧（12ms/帧）→ 行签名位移匹配（`find_offset_from_sigs`）→ 追加 + 末尾刷新。

**行签名**：多项式滚动哈希（base 31），位置敏感、碰撞率远低于 RGB 求和。位移检测按精确匹配计数（match ratio），置信度阈值 0.25。

**顶部固定元素去重（差分掩码）**：选区内不随滚动的顶部固定元素（Chrome 工具栏、网页 sticky header）自动检测，避免 toolbar 在拼接图中重复堆叠。底部 footer 不做去重（会随 append 自然重复，保持可预测的完整段落）。

- `update_static_mask`：帧间存在位移（k>0）时，行哈希帧间精确匹配的行累计投票（+2/-1），达阈值（≥2）标记为固定区域。
- `append_frame`：每帧追加末尾 k 行（保持 buf 对齐）；末尾刷新逐行跳过固定行——固定行号位置不被刷成最新帧的固定元素，顶部 toolbar 不会被贴到 buf 末尾堆叠。
- 预览实时性由末尾刷新保证（每帧刷新 buf 末尾 h 行）。

**自动停止检测**：`capture_loop` 追踪 `static_streak`（连续无位移帧数），达到 20 帧（~240ms）时发 `screenshot-scroll-stopped` 事件，前端显示"已到底部"提示。

**工具栏布局**：scroll 模式下 AnnotationPalette 通过 `set_scroll_toolbar_rect` 命令把屏幕矩形同步给 Rust，`mouse_monitor` 的穿透洞排除该区域（`in_hole = in_sel && !in_toolbar`），工具栏即使落在选区内（insideBottom 布局）也可点击；显式 `pointerEvents:auto` 覆盖父级 `none`。
