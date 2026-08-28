# screenshot

区域截屏 + 标注 + OCR/二维码识别 + 钉图 + 滚动长截图。原生 CALayer 直接渲染 CGImage（零拷贝），WKWebView 透明覆盖做交互层。

## 架构

- 截图渲染走 ObjC++ 桥（`screenshot_overlay.mm`）把 CGImage 直接设为 `CALayer.contents`，按键到显示 ~20-30ms
- 标注/选区/工具栏走 Vue（`windows/Operation.vue` + `composables/`）
- 文字标注双样式：纯文本 / 底色模式（`input-method-line/fill` 字母图标切换，位于颜色选择器之前，后接字号滑杆；状态 `annotTextBg` 落 `Shape.textBg`），选中已有标注时参数（字号/颜色/底色）回灌、切换即改形（同 blurMode 范式，颜色实时作用于编辑中占位 shape）
- 底色模式：整块圆角底（宽度 = max(换行盒宽, 实际最大行宽) + 内边距——自适应时贴合内容，手动拉宽后保留手动宽度），文字按底色亮度自动取黑/白对比色；编辑态 textarea 同步底色/内边距/对比色所见即所得，外侧虚线拖动框圆角统一按「底色块圆角 + 4px 间隙」随字号缩放（底色模式与色块同心，纯文本同公式，两模式切换圆角不跳变），右侧控制点贴住虚线编辑框右边框（含内边距 + 触控边距，垂直中心按实时内容行数跟随）
- 文字框宽度自适应：初始 40（`TEXT_MIN_WIDTH`）、下限 16（`TEXT_AUTO_MIN_WIDTH`），measureText 实测最大行宽、直读 textarea 值使拼音组词期间实时跟随；宽度变化联动高度重算，拖手柄改宽时换行与编辑框实时贴合，编辑器挂载帧即校正高度（多行不闪单行），字号变化后宽度按实时内容重自适应并同步编辑输入框（提交用输入框宽度，不同步会造成进/出编辑态底色宽度跳变）；拖手柄手动调宽后自适应让位（重开编辑复位）；文字原点建框/拖动时取整（小数坐标下 DOM 与 canvas 栅格取整不同，会产生提交偏移）
- 编辑态↔提交态零位移：提交文字由 DOM 呈现层渲染（`pre-wrap` div，与编辑态 textarea 同渲染管线）；仅导出（复制/保存/钉图/OCR）时经 canvasText begin/end 临时烧录进标注 canvas（`alphabetic` 基线 + CSS 行盒 max 公式 + 实测基线补偿 `Shape.baselineAdjust`）
- 工具条 / 色板：`acrylic-bar`（与主搜索框同款，soft-surface 材质 + `--shadow-bar`）；贴图悬停条：`mica-bar`
- 放大镜底图：capture 成功后与 enter **并行** ImageIO 编码 `picker.jpg`（任务独立 Retain CGImage；原子 rename）；前端 `loadPickerImage` 轮询就绪（主屏 Retina 编码更慢，禁止单次读空即放弃）
- 选区阶段（`phase === 'select'`，尚无工具栏）底部居中轻量快捷键提示：`Esc` 取消 / `F` 全屏 / `C` 复制色值；有上次选区时追加 `R` 恢复（`mica-panel` + kbd 样式，与 `onKeyDown` 对齐）
- 提示条与标注工具栏进出场：`Transition` + 浮层范式（进 150ms `ease-out` opacity/translate-y/scale，出 100ms `ease-in` 反向；`appear` 首次挂载亦进场）
- `native/` 按职责分：session（截图会话）、ocr（Vision 调用）、pin（钉图窗口）、scroll_capture/（滚动长截图：state / encode / stitch / mouse / 命令）、crop（裁剪）、ffi（ObjC++ 桥）、setup（启动钩子）

## 约束

- 需**屏幕录制权限**（`CGDisplayCreateImage`），未授权返回中文错误

### 多屏捕获

- **捕获光标所在显示器**：`CGMainDisplayID` 初始化 → 安全 `CGGetDisplaysWithPoint` → 优先 `CGDisplayCreateImage`，失败改 `CGWindowListCreateImage` 同 bounds，**不回落主屏**
- **overlay 几何**：以 capture 时 `surface.display_id` 对应 `NSScreen` 为第一优先（enter 时点仅 fallback）
- **Space 绑定**：SkyLight 把窗口绑到**所有显示器** Current Space（副屏独立 Space）；`CanJoinAllSpaces`
- **不跨屏框选**

### 坐标

- 前端选区为**屏内本地坐标**
- `CaptureSurface.origin` 仅 native 出口换算

### 窗口与焦点

- enter 即 contentView **opacity=1**，`claim_key` 多拍 + `overlay_ready` 再 claim
- 换屏/冷启动会丢 key；`acceptsFirstMouse` 可重试安装；启动预热 WebView

### 指针注入

- **select 阶段指针由原生 NSEvent local+global monitor 注入 `__screenshotPointer`**：global 仅在未 key 时注入，避免与 local 双份；冷启动首击常被系统当激活 / 穿到下层，DOM mousedown 不可靠
- **annotate/scroll 仍走 DOM**

### 快捷键

- 分发路径**禁止提前 `activate_app`**（会让前台 app resign active 可见失焦）；activation 延迟到 capture 完成后 enter 阶段的 `voidnix_screenshot_claim_key`（activate → makeKey，此时 overlay 已全屏可见，失焦被遮住用户无感）
- 会话中再按截屏快捷键 = **取消/解卡**

### 智能吸附

- `CGWindowListCopyWindowInfo` 枚举可见窗口矩形（layer∈[0,24)，含本应用 Floating 主窗；截屏 overlay/钉图在 Status=25 已排除），与目标屏相交后减 origin 变本地（智能吸附选区）

### OCR / 跨 Space

- Vision OCR 通过 `swift -e` 执行 `VNRecognizeTextRequest` + `VNDetectBarcodesRequest`（zh-Hans/Hant/en/ja 文字 + QR/条码），一次请求同时返回文字和二维码内容（`OcrResult { text, qr }`）
- Skylight `move_window_to_active_space` 跨 Space

## 数据存储

`last-selection.json`（屏内本地坐标，上次确认选区）：动作执行（copy/save/pin/ocr/scroll）时经 `save_last_selection` 命令落盘到 ext_data_dir，启动时回灌内存，供下次截图 `R` 键恢复（跨屏/分辨率变化时 clamp 到当前屏）。其余临时文件落 `$TMPDIR/voidnix*`（picker.jpg 预览、ocr/clip/pin/scroll 中间 PNG、voidnix-icon- 图标缓存），由 `runtime::storage::cleanup_all_voidnix_temps()` 在 `lib.rs` setup 启动期统一清理（覆盖 `voidnix_*` / `voidnix-icon-*` / `voidnix/picker.jpg` 三个前缀族）。`save_png_safely()` 提供 create_dir_all + path_guard + write 统一接口，供 save_screenshot / save_scroll_result 共用。配置通过 `extensions/screenshot/config.ts`（defineConfig 自管 `savePath`）。

## 滚动截屏拼接算法

纯像素路径（`scroll_capture/stitch.rs` + `encode.rs`）：CG 选区抓帧（12ms/帧）→ 行签名位移匹配（`find_offset_from_sigs`）→ 追加 + 末尾刷新。

**行签名**：多项式滚动哈希（base 31），位置敏感、碰撞率远低于 RGB 求和。位移检测按精确匹配计数（match ratio），置信度阈值 0.25。

**顶部固定元素去重（差分掩码）**：选区内不随滚动的顶部固定元素（Chrome 工具栏、网页 sticky header）自动检测，避免 toolbar 在拼接图中重复堆叠。底部 footer 不做去重（会随 append 自然重复，保持可预测的完整段落）。

- `update_static_mask`：帧间存在位移（k>0）时，行哈希帧间精确匹配的行累计投票（+2/-1），达阈值（≥2）标记为固定区域。
- `append_frame`：每帧追加末尾 k 行（保持 buf 对齐）；末尾刷新逐行跳过固定行——固定行号位置不被刷成最新帧的固定元素。
- 预览实时性由末尾刷新保证（每帧刷新 buf 末尾 h 行）。

**自动停止检测**：`capture_loop` 追踪 `static_streak`（连续无位移帧数），达到 20 帧（~240ms）时发 `screenshot-scroll-stopped` 事件，前端显示"已到底部"提示。

**工具栏布局**：scroll 模式下 AnnotationPalette 通过 `set_scroll_toolbar_rect` 命令把屏幕矩形同步给 Rust，`mouse_monitor` 的穿透洞排除该区域（`in_hole = in_sel && !in_toolbar`），工具栏即使落在选区内（insideBottom 布局）也可点击；显式 `pointerEvents:auto` 覆盖父级 `none`。
