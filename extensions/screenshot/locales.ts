import { registerMessages } from '@/runtime/i18n'

registerMessages({
  // ─── OcrView（主窗口） ──────────────────────
  'screenshot.previewAlt': { 'zh-CN': '截图预览', en: 'Screenshot preview' },
  'screenshot.ocrResult': { 'zh-CN': '识别结果', en: 'Recognition result' },
  'screenshot.actions': { 'zh-CN': '操作', en: 'Actions' },
  'screenshot.noContent': { 'zh-CN': '未识别到内容', en: 'No content recognized' },
  'screenshot.copy': { 'zh-CN': '复制', en: 'Copy' },
  'screenshot.translate': { 'zh-CN': '翻译', en: 'Translate' },
  'screenshot.trimSpaces': { 'zh-CN': '去空格', en: 'Trim Spaces' },
  'screenshot.trimNewlines': { 'zh-CN': '去换行', en: 'Trim Newlines' },
  'screenshot.trimEmptyLines': { 'zh-CN': '去空行', en: 'Trim Empty Lines' },
  'screenshot.savePath': { 'zh-CN': '截图保存位置', en: 'Screenshot Save Location' },

  // ─── 截图独立窗口（AnnotationPalette / PinWindow / Operation） ──
  'screenshot.save': { 'zh-CN': '保存', en: 'Save' },
  'screenshot.color': { 'zh-CN': '颜色', en: 'Color' },
  'screenshot.fontSize': { 'zh-CN': '字号', en: 'Font size' },
  'screenshot.lineWidth': { 'zh-CN': '线宽', en: 'Line width' },
  'screenshot.blurSelection': { 'zh-CN': '模糊整个选区', en: 'Blur entire selection' },
  'screenshot.blurText': { 'zh-CN': '模糊选区内文本', en: 'Blur text in selection' },
  'screenshot.blurAmount': { 'zh-CN': '模糊度', en: 'Blur amount' },
  'screenshot.ocr': { 'zh-CN': '识别', en: 'OCR' },
  'screenshot.scrollCapture': { 'zh-CN': '滚动截屏', en: 'Scroll capture' },
  'screenshot.pin': { 'zh-CN': '钉图', en: 'Pin' },
  'screenshot.closeEsc': { 'zh-CN': '关闭 (Esc)', en: 'Close (Esc)' },
  'screenshot.copyAndClose': { 'zh-CN': '复制并关闭 (Enter)', en: 'Copy & Close (Enter)' },
  'screenshot.tool.rect': { 'zh-CN': '矩形', en: 'Rectangle' },
  'screenshot.tool.line': { 'zh-CN': '直线', en: 'Line' },
  'screenshot.tool.arrow': { 'zh-CN': '箭头', en: 'Arrow' },
  'screenshot.tool.text': { 'zh-CN': '文字', en: 'Text' },
  'screenshot.tool.blur': { 'zh-CN': '模糊', en: 'Blur' },
  'screenshot.opacity': { 'zh-CN': '透明度', en: 'Opacity' },
  'screenshot.reachedBottom': {
    'zh-CN': '已到底部，按 Enter 完成',
    en: 'Reached bottom, press Enter to finish',
  },
  'screenshot.cancel': { 'zh-CN': '取消', en: 'Cancel' },
  'screenshot.fullscreen': { 'zh-CN': '全屏', en: 'Fullscreen' },
  'screenshot.copyColor': { 'zh-CN': '复制色值', en: 'Copy color value' },
  'screenshot.restoreSelection': { 'zh-CN': '恢复选区', en: 'Restore selection' },
})
