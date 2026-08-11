import { registerMessages } from '@/runtime/i18n'

registerMessages({
  // ─── View ──────────────────────────────────
  'clipboard.empty': { 'zh-CN': '暂无剪贴板记录', en: 'No clipboard records' },
  'clipboard.imageAlt': { 'zh-CN': '剪贴板图片', en: 'Clipboard image' },
  'clipboard.previewImageAlt': { 'zh-CN': '预览图片', en: 'Preview image' },
  'clipboard.editTitle': { 'zh-CN': '编辑文本', en: 'Edit Text' },
  'clipboard.editPlaceholder': { 'zh-CN': '编辑剪贴板文本', en: 'Edit clipboard text' },
  'clipboard.pasteFailed': { 'zh-CN': '粘贴失败', en: 'Paste failed' },

  // ─── action menu ───────────────────────────
  'clipboard.deleteBatch': { 'zh-CN': '删除 {count} 条', en: 'Delete {count} item(s)' },
  'clipboard.preview': { 'zh-CN': '预览', en: 'Preview' },
  'clipboard.unfavorite': { 'zh-CN': '取消收藏', en: 'Unfavorite' },
  'clipboard.favorite': { 'zh-CN': '收藏', en: 'Favorite' },
  'clipboard.edit': { 'zh-CN': '编辑', en: 'Edit' },

  // ─── preview / edit 加载态 ──────────────────
  'clipboard.loadingText': { 'zh-CN': '加载中…', en: 'Loading…' },
  'clipboard.loadFailed': { 'zh-CN': '加载失败', en: 'Failed to load' },

  // ─── 删除确认 ───────────────────────────────
  'clipboard.deleteTitle': { 'zh-CN': '删除剪贴板记录', en: 'Delete Clipboard Record' },
  'clipboard.deleteConfirmMessageMulti': {
    'zh-CN': '确定要删除 {count} 条记录吗？',
    en: 'Are you sure you want to delete {count} record(s)?',
  },
  'clipboard.deleteConfirmMessageSingle': {
    'zh-CN': '确定要删除这条记录吗？',
    en: 'Are you sure you want to delete this record?',
  },

  // ─── Actions 筛选 ───────────────────────────
  'clipboard.filter.all': { 'zh-CN': '全部', en: 'All' },
  'clipboard.filter.text': { 'zh-CN': '文本', en: 'Text' },
  'clipboard.filter.image': { 'zh-CN': '图片', en: 'Image' },
  'clipboard.filter.file': { 'zh-CN': '文件', en: 'File' },

  // ─── Settings ───────────────────────────────
  'clipboard.clearHistoryTitle': { 'zh-CN': '清空剪贴板记录', en: 'Clear Clipboard History' },
  'clipboard.clearHistoryMessage': {
    'zh-CN': '确定要清空所有未收藏的剪贴板记录吗？',
    en: 'Clear all non-favorited clipboard records?',
  },
  'clipboard.settings.shortcut': { 'zh-CN': '启动快捷键', en: 'Shortcut' },
  'clipboard.settings.groupGeneral': { 'zh-CN': '通用', en: 'General' },
  'clipboard.settings.retention': { 'zh-CN': '记录保留时长', en: 'Retention Period' },
  'clipboard.settings.days15': { 'zh-CN': '15 天', en: '15 days' },
  'clipboard.settings.days30': { 'zh-CN': '30 天', en: '30 days' },
  'clipboard.settings.days90': { 'zh-CN': '90 天', en: '90 days' },
  'clipboard.settings.forever': { 'zh-CN': '永久', en: 'Forever' },
  'clipboard.settings.clearUnfavorited': {
    'zh-CN': '清空未收藏记录',
    en: 'Clear Unfavorited',
  },
  'clipboard.settings.groupData': { 'zh-CN': '数据', en: 'Data' },
  'clipboard.settings.clear': { 'zh-CN': '清空', en: 'Clear' },

  // ─── logic 语义占位 ─────────────────────────
  'clipboard.kind.image': { 'zh-CN': '图片 image', en: 'Image' },
  'clipboard.kind.file': { 'zh-CN': '文件 file', en: 'File' },
  'clipboard.titleImage': { 'zh-CN': '[图片]', en: '[Image]' },
  'clipboard.titleFile': { 'zh-CN': '[文件]', en: '[File]' },
})
