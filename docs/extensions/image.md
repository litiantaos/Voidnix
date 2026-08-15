# 图片处理（image）

移除背景与拼接长图。macOS 原生 Vision / CoreGraphics 实现，零外部依赖，扩展视图操作。

## 能力

- **移除背景**：macOS Vision 前景实例分割（`VNGenerateForegroundInstanceMaskRequest`，macOS 14+ 内置模型，与「照片」抬起主体同一引擎），对任意前景物体（人 / 物 / 动物）分割，背景置透明
- **拼接长图**：横向 / 纵向合成，支持间距（正值）与重叠（负值，如电影截图台词拼接避免截断字幕），统一尺寸（宽度 / 高度等比缩放，消除异型图参差）
- 支持 macOS 原生解码格式：PNG / JPEG / HEIC / HEIF / WebP / TIFF / BMP / GIF
- 预览（棋盘格背景直观展示透明区域）、复制到剪贴板、保存到文件、在访达中显示
- 输出默认与源文件同目录，命名 `{stem}.{nobg|stitch}.png`；可改输出目录

## UI

搜索栏右侧配件（Actions.vue）为工具选择器（移除背景 / 拼接长图），与 mainView 共享 `tool` 状态。`disableSearchInput` + `windowHeight: 'auto'`。

- **移除背景**：选输入图 → 棋盘格预览区显示原图 → 处理后透明结果淡入覆盖（600ms 过渡）→ 复制 / 保存 / 访达
- **拼接**：实时预览（列表即预览，多图按布局合二为一）；添加多图（首次按文件名升序，后续追加末尾）；方向 / 统一尺寸（500 / 1000 / 2000）/ 间距参数；选中条目可上移 / 下移 / 移除；**惰性生成**——复制 / 保存时才拼接，文件或参数变更（指纹）后自动重新生成

## 核心

模型内置于系统，GPU 加速，移除背景通常 <1s。所有 ObjC 操作在 `autoreleasepool` 内执行，命令在 `spawn_blocking` 中调用（Vision `performRequests` 与 CoreGraphics 位图合成均同步阻塞）。

### 移除背景（remove_bg.rs）

NSImage 加载 → CGImage → `VNImageRequestHandler` → `VNGenerateForegroundInstanceMaskRequest` → `VNInstanceMaskObservation` → `generateMaskedImageOfInstances`（背景已置透明黑 CVPixelBuffer）→ CIImage 渲染 → PNG 编码。alloc+init 对象在返回前逐一 release，错误路径交 autoreleasepool 回收。

### 拼接（stitch.rs）

逐张加载 →（可选）统一尺寸等比缩放 → 计算布局（纵向宽度取最大值水平居中，横向高度取最大值垂直居中）→ RGBA 位图上下文逐张绘制 → PNG。`gap` 正值=间距、负值=重叠。**逆序绘制**（painter's algorithm 后绘制者覆盖先绘制者——首张图最后绘制 = 最上层，重叠时后续图仅露出底部台词）。

### 共用（shared.rs）

`load_image`（NSImage → CGImage，path_guard 安全校验）/ `encode_png`（NSBitmapImageRep）/ `build_result`（PNG 字节 → 写临时文件 `voidnix_image_` 前缀 + base64 data URL 预览，启动期 `cleanup_all_voidnix_temps` 自动清理）/ `save_png_safely`。

全局 `BUSY` AtomicBool：同时仅允许一个图片处理任务，互斥移除背景与拼接。

## 命令

- `image_remove_bg`（inputPath）
- `image_stitch`（inputPaths / direction / gap / resize）
- `image_read_preview`（返回 data URL，WKWebView 系统解码器预览全格式）
- `image_save_result`（tempPath → outputPath，复用临时文件不重复处理）
- `image_copy_to_clipboard`（PNG 含透明通道）

框架：`pick_files`（扩展名白名单）/ `pick_directory`。

## 配置

`extensions/image/config.json`（defineConfig）：

- `outputDir`（空 = 与源文件同目录）

## 目录

```
extensions/image/
├── index.ts / config.ts / logic.ts / locales.ts / View.vue / Actions.vue
└── native/
    ├── mod.rs       # 命令入口 + BUSY 锁
    ├── remove_bg.rs # Vision 前景分割
    ├── stitch.rs    # CoreGraphics 位图合成 + 布局计算
    └── shared.rs    # 加载 / PNG 编码 / 临时文件 / 结果构建
```
