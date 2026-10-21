import { registerMessages } from '@/runtime/i18n'

// 图片处理扩展文案
registerMessages({
  'image.removeBg': { 'zh-CN': '移除背景', en: 'Remove Background' },
  'image.stitch': { 'zh-CN': '拼接长图', en: 'Stitch Images' },

  // 预览区
  'image.original': { 'zh-CN': '原图', en: 'Original' },
  'image.result': { 'zh-CN': '处理结果', en: 'Result' },

  // 按钮
  'image.select': { 'zh-CN': '选择', en: 'Select' },
  'image.processing': { 'zh-CN': '处理中…', en: 'Processing…' },
  'image.add': { 'zh-CN': '添加', en: 'Add' },
  'image.sameDir': { 'zh-CN': '同目录', en: 'Same Folder' },

  // 拼接参数
  'image.width': { 'zh-CN': '宽度', en: 'Width' },
  'image.height': { 'zh-CN': '高度', en: 'Height' },
  'image.segmentingForeground': { 'zh-CN': '正在抠图…', en: 'Removing background…' },
  'image.formatsHint': {
    'zh-CN': '支持 PNG / JPEG / HEIC / WebP 等格式',
    en: 'Supports PNG / JPEG / HEIC / WebP, etc.',
  },
  'image.formatsHintMulti': {
    'zh-CN': '支持 PNG / JPEG / HEIC / WebP 等格式，选择多张',
    en: 'Supports PNG / JPEG / HEIC / WebP, etc. Select multiple images.',
  },
  'image.moveUp': { 'zh-CN': '上移', en: 'Move Up' },
  'image.moveLeft': { 'zh-CN': '左移', en: 'Move Left' },
  'image.moveDown': { 'zh-CN': '下移', en: 'Move Down' },
  'image.moveRight': { 'zh-CN': '右移', en: 'Move Right' },
  'image.remove': { 'zh-CN': '移除', en: 'Remove' },

  // 列表项
  'image.inputImage': { 'zh-CN': '输入图片', en: 'Input Image' },
  'image.mode': { 'zh-CN': '模式', en: 'Mode' },
  'image.totalSize': { 'zh-CN': '共 {size}', en: '{size} total' },
  'image.group.common': { 'zh-CN': '通用', en: 'General' },
  'image.group.file': { 'zh-CN': '文件', en: 'File' },
  'image.imageCount': { 'zh-CN': '{count} 张图片', en: '{count} images' },
  'image.direction': { 'zh-CN': '方向', en: 'Direction' },
  'image.direction.vertical': { 'zh-CN': '纵向', en: 'Vertical' },
  'image.direction.horizontal': { 'zh-CN': '横向', en: 'Horizontal' },
  'image.group.params': { 'zh-CN': '参数', en: 'Parameters' },
  'image.gap': { 'zh-CN': '间距', en: 'Gap' },

  // 输出
  'image.outputDir': { 'zh-CN': '输出目录', en: 'Output Folder' },
  'image.sameAsSource': { 'zh-CN': '与源文件相同', en: 'Same as source' },
  'image.group.output': { 'zh-CN': '输出', en: 'Output' },
  'image.copy': { 'zh-CN': '复制', en: 'Copy' },
  'image.save': { 'zh-CN': '保存', en: 'Save' },
  'image.operations': { 'zh-CN': '操作', en: 'Actions' },

  // 状态
  'image.processFailed': { 'zh-CN': '处理失败', en: 'Processing failed' },
  'image.stitchFailed': { 'zh-CN': '拼接失败', en: 'Stitching failed' },
  'image.copyFailed': { 'zh-CN': '复制失败', en: 'Copy failed' },
  'image.saveFailed': { 'zh-CN': '保存失败', en: 'Save failed' },
  'image.copiedToClipboard': { 'zh-CN': '已复制到剪贴板', en: 'Copied to clipboard' },
  'image.saved': { 'zh-CN': '已保存', en: 'Saved' },

  // 设置：移除背景引擎
  'image.settings': { 'zh-CN': '设置', en: 'Settings' },
  'image.closeSettings': { 'zh-CN': '关闭设置', en: 'Close Settings' },
  'image.group.removeBg': { 'zh-CN': '移除背景', en: 'Background Removal' },
  'image.engine': { 'zh-CN': '引擎', en: 'Engine' },
  'image.engineHint': {
    'zh-CN': '系统模型本地免费；云端模型毛发与复杂边缘更准（图片会上传 Bria）',
    en: 'Local model is free; cloud model handles hair and complex edges better (uploads to Bria)',
  },
  'image.engine.vision': { 'zh-CN': '系统 Vision（本地）', en: 'System Vision (local)' },
  'image.engine.bria': { 'zh-CN': 'Bria RMBG-2.0（云端）', en: 'Bria RMBG-2.0 (cloud)' },
  'image.cloudMaxEdge': { 'zh-CN': '云端最长边', en: 'Cloud Max Edge' },
  'image.cloudMaxEdgeHint': {
    'zh-CN': '越小上传/下载越快；2048 对一般抠图足够，要原图选「原始」',
    en: 'Smaller is faster; 2048 is enough for most cutouts, choose Original for full resolution',
  },
  'image.cloudMaxEdge.original': { 'zh-CN': '原始', en: 'Original' },
  'image.preparing': { 'zh-CN': '准备图片…', en: 'Preparing image…' },
  'image.uploading': { 'zh-CN': '上传并处理中…', en: 'Uploading and processing…' },
  'image.downloading': { 'zh-CN': '下载结果…', en: 'Downloading result…' },
  'image.briaToken': { 'zh-CN': 'Bria API Token', en: 'Bria API Token' },
  'image.briaTokenHint': {
    'zh-CN': '明文保存在本地扩展配置',
    en: 'Stored in plain text in local extension config',
  },
  'image.briaTokenPlaceholder': { 'zh-CN': '粘贴 Token', en: 'Paste token' },
  'image.briaTokenRequired': {
    'zh-CN': '未配置 Bria API Token，请先在设置中填写',
    en: 'Bria API Token is not configured. Add it in Settings.',
  },
  'image.briaConsole': { 'zh-CN': 'Bria 控制台', en: 'Bria Console' },
  'image.briaFreeHint': {
    'zh-CN': '新账户 100 次免费，之后约 $0.018/张',
    en: '100 free calls for new accounts, then about $0.018/image',
  },
  'image.open': { 'zh-CN': '打开', en: 'Open' },
})
