# 视频处理（video）

压缩、格式转换、提取音频。FFmpeg 子进程，扩展视图操作。

## 能力

- 选输入视频（框架 `pick_files` 多选，扩展名白名单），多文件批量处理
- **跨扩展接收**：finder-ext 等经事件总线 `video-pending-input-path` 投递路径数组（访达多选区全量带入），`pendingInputPaths` watch 后自动加载并逐个 probe
- 探测元数据（时长 / 分辨率 / 编码 / 体积）；多文件副标题汇总总时长 · 总大小
- **压缩**：质量预设 high / balanced / small
- **格式转换**：mp4 / mov / mkv / webm / gif
- **提取音频**：m4a / mp3
- 可选缩放：original / 1080 / 720 / 480
- 输出默认与源同目录，命名 `{stem}.{compressed|converted|audio}.{ext}`；可改输出目录
- 批量队列：单文件失败跳过继续下一个，结束汇总 toast（`完成 x/n，y 个失败`）；取消终止整批；全部成功后清空选择回到初始态，失败/取消保留列表可重试（重试重跑全部文件，输出冲突自动加 `-1/-2` 后缀）
- 重新进入扩展即新会话：未在处理中的文件选择清空重置（处理中保留进度上下文）
- Rust 端保持单任务模型（BUSY 互斥）；队列在前端串行发起（复用 `video_run`，终态经 Channel 事件推进——invoke resolve 不区分成败）。批量状态（paths / metas / busy / 进度计数）放 View.vue 模块级 reactive，窗口隐藏 KeepAlive 卸载后队列继续跑、重开面板计数不丢；WebContent navigate 重载会丢队列（当前文件跑完即止，重开面板退化为孤儿观察模式）
- 可取消；窗口隐藏后任务继续

## UI

「输入视频」单行承载核心 + 选文件 + 开始（无独立「开始处理」行）：

- 未就绪：副标题提示 +「下载 FFmpeg」
- 下载中：`正在下载核心…` + 进度
- 就绪未选文件：副标题 `核心版本：FFmpeg x.x.x` +「选择」
- 已选单文件：标题文件名、副标题仅元数据 +「选择」+「开始」（不显示核心版本）
- 已选多文件：标题 `{n} 个视频`、副标题总时长 · 总大小（探测完成度渐进）
- 进行中：副标题 `进度 {i}/{n} · xx%`（单文件 `进度 xx%`）+「取消」（不显示文件信息）
- 完成后 toast（单文件原文案、多文件汇总），全部成功同时清空选择回到初始态，不常驻输出结果行

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
└── native/
    ├── mod.rs    # 命令入口
    ├── core.rs   # 解析 / 下载 ffmpeg
    ├── args.rs   # 预设 → argv
    └── job.rs    # 任务 / 进度 / 取消
```
