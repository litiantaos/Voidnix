import { registerMessages } from '@/runtime/i18n'

// 视频处理扩展文案
registerMessages({
  'video.downloadFFmpeg': { 'zh-CN': '下载 FFmpeg', en: 'Download FFmpeg' },
  'video.select': { 'zh-CN': '选择', en: 'Select' },
  'video.cancel': { 'zh-CN': '取消', en: 'Cancel' },
  'video.start': { 'zh-CN': '开始', en: 'Start' },
  'video.operations': { 'zh-CN': '操作', en: 'Actions' },
  'video.sameDir': { 'zh-CN': '同目录', en: 'Same Folder' },

  // 核心下载
  'video.downloading': { 'zh-CN': '下载中…', en: 'Downloading…' },
  // 依赖下载：按钮内百分比 + 本行副标题说明（非处理任务，不进任务行，见 docs/design.md）
  'video.downloadingCore': { 'zh-CN': '正在下载 FFmpeg…', en: 'Downloading FFmpeg…' },
  'video.coreVersionNone': { 'zh-CN': '核心版本：FFmpeg —', en: 'Core version: FFmpeg —' },
  'video.dependencyHint': { 'zh-CN': '使用前需先下载 FFmpeg', en: 'Download FFmpeg first' },
  'video.coreVersion': {
    'zh-CN': '核心版本：FFmpeg {version}',
    en: 'Core version: FFmpeg {version}',
  },
  'video.coreReady': { 'zh-CN': 'FFmpeg 已就绪', en: 'FFmpeg is ready' },
  'video.downloadFailed': { 'zh-CN': '下载失败', en: 'Download failed' },

  // 输入
  'video.inputVideo': { 'zh-CN': '输入视频', en: 'Input Video' },
  'video.fileCount': { 'zh-CN': '{n} 个视频', en: '{n} Videos' },
  'video.group.file': { 'zh-CN': '文件', en: 'File' },

  // 模式
  'video.group.common': { 'zh-CN': '通用', en: 'General' },
  'video.mode': { 'zh-CN': '模式', en: 'Mode' },
  'video.mode.compress': { 'zh-CN': '压缩', en: 'Compress' },
  'video.mode.convert': { 'zh-CN': '格式转换', en: 'Convert Format' },
  'video.mode.extractAudio': { 'zh-CN': '提取音频', en: 'Extract Audio' },

  // 参数
  'video.group.params': { 'zh-CN': '参数', en: 'Parameters' },
  'video.quality': { 'zh-CN': '质量', en: 'Quality' },
  'video.quality.high': { 'zh-CN': '高画质', en: 'High quality' },
  'video.quality.balanced': { 'zh-CN': '均衡', en: 'Balanced' },
  'video.quality.small': { 'zh-CN': '小体积', en: 'Small size' },
  'video.resolution': { 'zh-CN': '分辨率', en: 'Resolution' },
  'video.resolution.original': { 'zh-CN': '原始', en: 'Original' },
  'video.container': { 'zh-CN': '封装格式', en: 'Container' },
  'video.targetFormat': { 'zh-CN': '目标格式', en: 'Target Format' },
  'video.frameRateTier': { 'zh-CN': '帧率', en: 'Frame rate' },
  'video.audioFormat': { 'zh-CN': '音频格式', en: 'Audio Format' },
  'video.audioQuality': { 'zh-CN': '音质', en: 'Audio Quality' },
  'video.audioQuality.high': { 'zh-CN': '高（192k）', en: 'High (192k)' },
  'video.audioQuality.balanced': { 'zh-CN': '标准（128k）', en: 'Standard (128k)' },
  'video.audioQuality.small': { 'zh-CN': '小体积（96k）', en: 'Small (96k)' },

  // 输出
  'video.group.output': { 'zh-CN': '输出', en: 'Output' },
  'video.outputDir': { 'zh-CN': '输出目录', en: 'Output Folder' },
  'video.sameAsSource': { 'zh-CN': '与源文件相同', en: 'Same as source' },

  // 处理态任务行（标题 + 进度条，见 docs/design.md「处理态」）
  'video.taskCompress': { 'zh-CN': '正在压缩', en: 'Compressing' },
  'video.taskConvert': { 'zh-CN': '正在转换格式', en: 'Converting' },
  'video.taskExtractAudio': { 'zh-CN': '正在提取音频', en: 'Extracting audio' },
  'video.taskCounter': { 'zh-CN': '第 {i}/{n} 个', en: '{i}/{n}' },
  'video.taskPreparing': { 'zh-CN': '准备中…', en: 'Preparing…' },
  'video.taskDone': { 'zh-CN': '处理完成', en: 'Completed' },

  // 处理结果
  'video.cannotReadVideo': { 'zh-CN': '无法读取视频', en: 'Cannot read video' },
  'video.processComplete': { 'zh-CN': '处理完成', en: 'Processing complete' },
  'video.batchDone': { 'zh-CN': '已完成 {n} 个视频', en: 'Processed {n} videos' },
  'video.batchDonePartial': {
    'zh-CN': '完成 {done}/{n}，{failed} 个失败',
    en: '{done}/{n} done, {failed} failed',
  },
  'video.canceled': { 'zh-CN': '已取消', en: 'Canceled' },
  'video.failed': { 'zh-CN': '失败', en: 'Failed' },

  // 剪辑（独立剪辑窗口）
  'video.clip.export': { 'zh-CN': '导出', en: 'Export' },
  'video.clip.entry': { 'zh-CN': '剪辑视频', en: 'Edit Video' },
  'video.clip.entryDesc': {
    'zh-CN': '裁剪 · 拼接 · 一键去停顿',
    en: 'Trim, stitch, remove silence',
  },
  'video.clip.open': { 'zh-CN': '打开', en: 'Open' },
  'video.clip.title': { 'zh-CN': '剪辑', en: 'Editor' },
  'video.clip.probing': { 'zh-CN': '正在分析素材…', en: 'Analyzing media…' },
  'video.clip.timeline': { 'zh-CN': '正在生成时间轴…', en: 'Building timeline…' },
  'video.clip.openFailed': { 'zh-CN': '无法打开剪辑素材', en: 'Cannot open media' },
  'video.clip.notPreviewable': {
    'zh-CN': '该格式（{codec}）无法直接预览，需先重新封装或转码',
    en: 'Preview unsupported ({codec}); remux or transcode first',
  },
  'video.clip.previewFailed': { 'zh-CN': '预览加载失败', en: 'Preview failed to load' },
  'video.clip.segments': { 'zh-CN': '片段', en: 'Clips' },
  'video.clip.speed': { 'zh-CN': '倍速', en: 'Speed' },
  'video.clip.sources': { 'zh-CN': '素材', en: 'Sources' },
  'video.clip.sourcesCount': { 'zh-CN': '{n} 个素材', en: '{n} sources' },
  'video.clip.sourceSegs': { 'zh-CN': '{n} 段', en: '{n} clips' },
  'video.clip.notOnTimeline': { 'zh-CN': '未上轨', en: 'not on timeline' },
  'video.clip.noOut': { 'zh-CN': '未设', en: 'not set' },
  'video.clip.hintKeys': {
    'zh-CN': 'Space 播放 · I/O 打点 · ⌫ 删除 · S 切分 · ⌘↑↓ 排序',
    en: 'Space play · I/O mark · ⌫ delete · S split · ⌘↑↓ reorder',
  },
  'video.clip.needRange': { 'zh-CN': '先设入点和出点', en: 'Set in and out points first' },
  'video.clip.exit': { 'zh-CN': '关闭', en: 'Close' },
  'video.clip.exported': { 'zh-CN': '已导出', en: 'Exported' },
  'video.clip.reveal': { 'zh-CN': '在 Finder 显示', en: 'Show in Finder' },
  'video.clip.exportFailed': { 'zh-CN': '导出失败', en: 'Export failed' },
  'video.clip.exportCanceled': { 'zh-CN': '已取消导出', en: 'Export canceled' },
  'video.clip.exportStage': { 'zh-CN': '正在导出', en: 'Exporting' },
  'video.clip.removeSilence': { 'zh-CN': '去停顿', en: 'Cut pauses' },
  'video.clip.analyzing': { 'zh-CN': '正在分析停顿…', en: 'Analyzing pauses…' },
  'video.clip.noSilence': { 'zh-CN': '未检测到停顿', en: 'No pauses found' },
  'video.clip.silenceSummary': {
    'zh-CN': '将删除 {n} 段 · {duration}（保留气口 {gap}ms）',
    en: 'Remove {n} pauses · {duration} (keep {gap}ms)',
  },
  'video.clip.silenceApply': {
    'zh-CN': 'Enter 应用 · Esc 取消',
    en: 'Enter apply · Esc cancel',
  },
  'video.clip.silenceApplied': {
    'zh-CN': '已剪掉 {n} 处停顿',
    en: '{n} pauses cut',
  },
  'video.clip.silenceFailed': { 'zh-CN': '停顿分析失败', en: 'Pause analysis failed' },
  'video.clip.noAudio': { 'zh-CN': '该素材没有音轨', en: 'No audio track' },
  'video.clip.play': { 'zh-CN': '播放', en: 'Play' },
  'video.clip.pause': { 'zh-CN': '暂停', en: 'Pause' },
  'video.clip.prevFrame': { 'zh-CN': '上一帧', en: 'Previous frame' },
  'video.clip.nextFrame': { 'zh-CN': '下一帧', en: 'Next frame' },
  'video.clip.deleteSegment': { 'zh-CN': '删除片段', en: 'Delete clip' },
  'video.clip.dragSort': { 'zh-CN': '拖拽排序', en: 'Drag to reorder' },
})
