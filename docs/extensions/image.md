# 图片处理（image）

移除背景与拼接长图。移除背景默认 macOS 原生 Vision，可选接入云端 Bria RMBG-2.0（质量对齐 remove.bg 级模型）；拼接为 CoreGraphics 本地实现。

## 能力

- **移除背景**：两种引擎，设置页切换
  - **系统 Vision**（默认）：`VNGenerateForegroundInstanceMaskRequest`（macOS 14+ 内置模型，与「照片」抬起主体同一引擎），本地、免费、无网络；对毛发 / 半透明 / 复杂前景弱于专用模型
  - **Bria RMBG-2.0**（云端）：Bria `remove_background` 接口，新账户 100 次免费，之后 $0.018/张；需要 API Token（存本地扩展配置）；云端输入最长边默认 2048（可切 4096 / 1024 / 原始），>2MB 的图上传前重编码 JPEG q0.9；处理中显示「准备 / 上传并处理 / 下载结果」三阶段
  - 选型背景：remove.bg API 于 2026-12-01 迁移到 Leonardo（$0.1047/张、无免费层），故接入 Bria RMBG-2.0（同档质量、100 次免费 + $0.018/张）
- **拼接长图**：横向 / 纵向合成，支持间距（正值）与重叠（负值，如电影截图台词拼接避免截断字幕），统一尺寸（宽度 / 高度等比缩放，消除异型图参差）
- 支持 macOS 原生解码格式：PNG / JPEG / HEIC / HEIF / WebP / TIFF / BMP / GIF（Bria 引擎本地转码后上传）
- 预览（棋盘格背景直观展示透明区域）、复制到剪贴板、保存到文件
- 输出默认与源文件同目录，命名 `{stem}.{nobg|stitch}.png`；可改输出目录

## UI

列表结构：**通用组**（第一项「模式」+ 第二项「操作」——removeBg **有图才显示**、拼接**两张起显示**（可操作性与显示同步，按钮零数量禁用态），回车执行主按钮动作；removeBg 未处理 = 移除背景（primary，**有结果后去掉**），有结果 / 拼接 = 复制 + **保存（primary）**）→ 文件组（source 行：选择 / 添加）→ 该模式参数 → 输出组，全程 ↑↓ + Enter 同质键盘流。`disableSearchInput` + `windowHeight: 'auto'`。

输入行小字与 video 同范式：标题表达「是什么」（单文件 = 文件名 / 多文件 = 计数），副标题表达「关键信息」——未选 = 格式支持提示（拼接版追加「选择多张」）；removeBg 已选 = `宽×高 · 大小`（从预览 data URL 前端推算，零 probe 命令）；拼接多图 = 总大小（从缩略图 data URL 渐进累计，数字 Map 不随 LRU 驱逐）。

预览区高度按屏高统一比例 30%（`currentMonitor` 逻辑高度，进入扩展 + 窗口获焦时刷新——窗口每次 show 定位光标屏，扩展保持激活下跨屏唤起不触发 deactivate/activate 周期、由获焦事件补刷新屏基数；模式/方向切换零高度跳变）。预览区**直接渲染、无布局动画**（高度一步到位，列表随布局直接落位）；窗口高度变化经 `animate_frame` 系统动画。

**共享输入集合**：两工具单一图片源（`imageFiles`）——removeBg 单选 / finder-ext 投递的图进集合（去重追加），拼接「添加」同源；removeBg 处理集合中的「当前张」（切工具时自动解析：当前张仍在集合则保持，否则取拼接选中项、首张兜底），移除当前张时清 removeBg 侧状态、切回取剩余首张。切换工具不重选图。

**跨扩展接收**：finder-ext 等经同页 window CustomEvent `image-pending-input-path` 同步投递路径数组（访达多选区全量带入），`pendingInputPaths` watch（immediate）后按张数分流——单张直达 removeBg（只改本地不落盘默认），多张自动切拼接模式（选区序即拼接序，预览区序号可调）。

- **移除背景**：选输入图 → 棋盘格预览区显示原图（`@load` 解码完成后 600ms 淡入，框与图不两步突现）→ 处理后透明结果淡入覆盖（600ms 过渡）→ 操作行复制 / 保存
- **拼接**：实时预览（列表即预览，多图按布局合二为一）；添加多图（首次按文件名升序，后续追加末尾）；方向 / 统一尺寸（500 / 1000 / 2000）/ 间距参数；**点击选中图片** → 预览区底部中心浮现悬浮操作条（上移 / 下移 / 移除，`acrylic-bar` + `ui-popup` 浮层过渡，挂 wrapper 不随内容滚动；方向语义随拼接方向切换）；**惰性生成**——复制 / 保存时才拼接，文件或参数变更（指纹）后自动重新生成

## 核心

Vision 模型内置于系统，GPU 加速，通常 <1s；Bria 走网络，耗时取决于链路与图片尺寸。所有 ObjC 操作在 `autoreleasepool` 内执行，本地命令在 `spawn_blocking` 中调用（Vision `performRequests` 与 CoreGraphics 位图合成均同步阻塞）。

### 移除背景（remove_bg.rs / bria.rs）

- **Vision**：NSImage 加载 → CGImage → `VNImageRequestHandler` → `VNGenerateForegroundInstanceMaskRequest` → `VNInstanceMaskObservation` → `generateMaskedImageOfInstances`（背景已置透明黑 CVPixelBuffer）→ CIImage 渲染 → PNG 编码。alloc+init 对象在返回前逐一 release，错误路径交 autoreleasepool 回收
- **Bria**：路径校验 → NSImage 解码取尺寸 → 按 `maxEdge`（0 = 原始，2048 / 4096 / 1024）与 20MP 上限算缩放比 → JPEG/PNG/WEBP 且无需缩放/未超 2MB 直接透传，其余等比缩放到目标尺寸、白底绘制后 ImageIO 转 JPEG（q0.9；未缩放且重编码反而更大则回退原始字节）→ base64 → `POST /v2/image/edit/remove_background`（`api_token` 头，`sync: true`，`preserve_alpha: true`）→ 校验结果 URL（https + 公网，防 SSRF）→ 下载 PNG → 解析 IHDR 取尺寸 → 临时文件 + base64 预览。HTTP 错误按 401/402/403/406/413/415/429/5xx 映射为可读文案。`Channel<String>` 推送 `preparing / uploading / downloading`，前端映射为处理中副标题

### 拼接（stitch.rs）

逐张加载 →（可选）统一尺寸等比缩放 → 计算布局（纵向宽度取最大值水平居中，横向高度取最大值垂直居中）→ RGBA 位图上下文逐张绘制 → PNG。`gap` 正值=间距、负值=重叠。**逆序绘制**（painter's algorithm 后绘制者覆盖先绘制者——首张图最后绘制 = 最上层，重叠时后续图仅露出底部台词）。

### 共用（shared.rs）

`load_image`（NSImage → CGImage，path_guard 安全校验）/ `encode_png`（NSBitmapImageRep）/ `build_result`（PNG 字节 → 写临时文件 `voidnix_image_` 前缀 + base64 data URL 预览，启动期 `cleanup_all_voidnix_temps` 自动清理）/ `save_png_safely`。

全局 `BUSY` AtomicBool：同时仅允许一个图片处理任务，互斥移除背景与拼接。

## 命令

- `image_remove_bg`（inputPath / engine?: `vision` | `bria` / token? / maxEdge? / onProgress: Channel）
- `image_stitch`（inputPaths / direction / gap / resize）
- `image_read_preview`（返回 data URL，WKWebView 系统解码器预览全格式）
- `image_save_result`（tempPath → outputPath，复用临时文件不重复处理）
- `image_copy_to_clipboard`（PNG 含透明通道）

框架：`pick_files`（扩展名白名单）/ `pick_directory`。

## 配置

`extensions/image/config.json`（defineConfig）：

- `defaultTool`（用户主动切换工具时写回，投递直达只改会话内状态不落盘）
- `outputDir`（空 = 与源文件同目录）
- `removeBgEngine`（`vision` 默认 / `bria`）
- `briaToken`（Bria `api_token` 请求头；明文存本地配置）
- `briaMaxEdge`（云端输入最长边，默认 `2048`；`0` = 原始，`4096` / `1024` 可选）

设置入口：搜索栏右侧齿轮（`Actions.vue`）→ `config` 子视图。引擎切到 Bria 后展示最长边、Token 输入与 Bria 控制台入口；Token 为空时直接点移除背景会 toast 引导。

## 目录

```
extensions/image/
├── index.ts / config.ts / logic.ts / locales.ts
├── View.vue / Settings.vue / Actions.vue
└── native/
    ├── mod.rs       # 命令入口 + BUSY 锁
    ├── remove_bg.rs # Vision 前景分割
    ├── bria.rs      # Bria RMBG-2.0 云端请求
    ├── stitch.rs    # CoreGraphics 位图合成 + 布局计算
    └── shared.rs    # 加载 / PNG·JPEG 编码 / 临时文件 / 结果构建
```
