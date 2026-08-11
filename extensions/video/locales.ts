import { registerMessages } from '@/runtime/i18n'

// 视频处理扩展文案
registerMessages({
  'video.downloadFFmpeg': { 'zh-CN': '下载 FFmpeg', en: 'Download FFmpeg' },
  'video.select': { 'zh-CN': '选择', en: 'Select' },
  'video.cancel': { 'zh-CN': '取消', en: 'Cancel' },
  'video.start': { 'zh-CN': '开始', en: 'Start' },
  'video.sameDir': { 'zh-CN': '同目录', en: 'Same Folder' },

  // 核心下载
  'video.downloading': { 'zh-CN': '下载中…', en: 'Downloading…' },
  'video.downloadingCore': { 'zh-CN': '正在下载核心…', en: 'Downloading core…' },
  'video.coreVersionNone': { 'zh-CN': '核心版本：FFmpeg —', en: 'Core version: FFmpeg —' },
  'video.dependencyHint': {
    'zh-CN': '功能依赖 FFmpeg 核心，请先下载',
    en: 'This feature requires the FFmpeg core. Please download it first.',
  },
  'video.coreVersion': {
    'zh-CN': '核心版本：FFmpeg {version}',
    en: 'Core version: FFmpeg {version}',
  },
  'video.coreReady': { 'zh-CN': 'FFmpeg 已就绪', en: 'FFmpeg is ready' },
  'video.downloadFailed': { 'zh-CN': '下载失败', en: 'Download failed' },

  // 输入
  'video.inputVideo': { 'zh-CN': '输入视频', en: 'Input Video' },
  'video.group.file': { 'zh-CN': '文件', en: 'File' },

  // 模式
  'video.mode': { 'zh-CN': '模式', en: 'Mode' },
  'video.mode.compress': { 'zh-CN': '压缩', en: 'Compress' },
  'video.mode.convert': { 'zh-CN': '格式转换', en: 'Convert Format' },
  'video.mode.extractAudio': { 'zh-CN': '提取音频', en: 'Extract Audio' },

  // 参数
  'video.group.params': { 'zh-CN': '参数', en: 'Parameters' },
  'video.quality': { 'zh-CN': '质量', en: 'Quality' },
  'video.quality.high': { 'zh-CN': '高质量', en: 'High Quality' },
  'video.quality.balanced': { 'zh-CN': '均衡', en: 'Balanced' },
  'video.quality.small': { 'zh-CN': '体积优先', en: 'Smallest Size' },
  'video.resolution': { 'zh-CN': '分辨率', en: 'Resolution' },
  'video.resolution.original': { 'zh-CN': '原始', en: 'Original' },
  'video.container': { 'zh-CN': '容器', en: 'Container' },
  'video.targetFormat': { 'zh-CN': '目标格式', en: 'Target Format' },
  'video.frameRateTier': { 'zh-CN': '帧率档', en: 'Frame Rate Tier' },
  'video.audioFormat': { 'zh-CN': '音频格式', en: 'Audio Format' },
  'video.audioQuality': { 'zh-CN': '音质', en: 'Audio Quality' },
  'video.audioQuality.high': { 'zh-CN': '高（192k）', en: 'High (192k)' },
  'video.audioQuality.balanced': { 'zh-CN': '标准（128k）', en: 'Standard (128k)' },
  'video.audioQuality.small': { 'zh-CN': '省流（96k）', en: 'Compact (96k)' },

  // 输出
  'video.group.output': { 'zh-CN': '输出', en: 'Output' },
  'video.outputDir': { 'zh-CN': '输出目录', en: 'Output Folder' },
  'video.sameAsSource': { 'zh-CN': '与源文件相同', en: 'Same as source' },

  // 进度/状态
  'video.progress': { 'zh-CN': '进度 {value}', en: 'Progress {value}' },

  // 处理结果
  'video.cannotReadVideo': { 'zh-CN': '无法读取视频', en: 'Cannot read video' },
  'video.processComplete': { 'zh-CN': '处理完成', en: 'Processing complete' },
  'video.canceled': { 'zh-CN': '已取消', en: 'Canceled' },
  'video.failed': { 'zh-CN': '失败', en: 'Failed' },
  'video.startFailed': { 'zh-CN': '启动失败', en: 'Failed to start' },
})
