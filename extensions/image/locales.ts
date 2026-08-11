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
  'image.segmentingForeground': { 'zh-CN': '正在分割前景…', en: 'Segmenting foreground…' },
  'image.formatsHint': {
    'zh-CN': '支持 PNG / JPEG / HEIC / WebP 等格式',
    en: 'Supports PNG / JPEG / HEIC / WebP, etc.',
  },
  'image.moveUp': { 'zh-CN': '上移', en: 'Move Up' },
  'image.moveLeft': { 'zh-CN': '左移', en: 'Move Left' },
  'image.moveDown': { 'zh-CN': '下移', en: 'Move Down' },
  'image.moveRight': { 'zh-CN': '右移', en: 'Move Right' },
  'image.remove': { 'zh-CN': '移除', en: 'Remove' },

  // 列表项
  'image.inputImage': { 'zh-CN': '输入图片', en: 'Input Image' },
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
  'image.revealInFinder': { 'zh-CN': '在访达中显示', en: 'Reveal in Finder' },
  'image.group.actions': { 'zh-CN': '操作', en: 'Actions' },

  // 状态
  'image.processFailed': { 'zh-CN': '处理失败', en: 'Processing failed' },
  'image.stitchFailed': { 'zh-CN': '拼接失败', en: 'Stitching failed' },
  'image.copyFailed': { 'zh-CN': '复制失败', en: 'Copy failed' },
  'image.saveFailed': { 'zh-CN': '保存失败', en: 'Save failed' },
  'image.copiedToClipboard': { 'zh-CN': '已复制到剪贴板', en: 'Copied to clipboard' },
  'image.saved': { 'zh-CN': '已保存', en: 'Saved' },
})
