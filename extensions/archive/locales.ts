import { registerMessages } from '@/runtime/i18n'

// 归档扩展文案
registerMessages({
  'archive.mode': { 'zh-CN': '操作', en: 'Action' },
  'archive.file': { 'zh-CN': '文件', en: 'File' },
  'archive.extract': { 'zh-CN': '解压', en: 'Extract' },
  'archive.selectArchive': { 'zh-CN': '选择压缩包…', en: 'Select Archive…' },
  'archive.selectFiles': { 'zh-CN': '选择文件', en: 'Select Files' },
  'archive.truncated': {
    'zh-CN': '条目过多，仅显示前 {n} 项',
    en: 'Too many entries, showing first {n}',
  },
  'archive.entries': { 'zh-CN': '个条目', en: 'entries' },
  'archive.extractTo': { 'zh-CN': '解压到', en: 'Extract to' },
  'archive.sameDir': { 'zh-CN': '压缩包同目录', en: 'Same as Archive' },
  'archive.browseFiles': { 'zh-CN': '查看压缩包', en: 'Browse Archive' },
  'archive.truncatedShort': { 'zh-CN': '条目过多', en: 'Truncated' },
  'archive.chooseDir': { 'zh-CN': '选择目录', en: 'Choose Folder' },
  'archive.extracting': { 'zh-CN': '解压中…', en: 'Extracting…' },
  'archive.compress': { 'zh-CN': '压缩', en: 'Compress' },
  'archive.compressing': { 'zh-CN': '压缩中…', en: 'Compressing…' },
  'archive.cancel': { 'zh-CN': '取消', en: 'Cancel' },
  'archive.cancelled': { 'zh-CN': '已取消', en: 'Cancelled' },
  'archive.extractDone': { 'zh-CN': '已解压 {n} 个文件', en: 'Extracted {n} files' },
  'archive.compressDone': {
    'zh-CN': '已压缩 {n} 个文件（{size}）',
    en: 'Compressed {n} files ({size})',
  },
  'archive.junkSkipped': {
    'zh-CN': '已过滤 {n} 个条目',
    en: '{n} entries filtered',
  },
  'archive.skippedUnsafe': {
    'zh-CN': '已拦截 {n} 个危险路径条目',
    en: '{n} unsafe path entries blocked',
  },
  'archive.reveal': { 'zh-CN': '在 Finder 中显示', en: 'Reveal in Finder' },
  'archive.format': { 'zh-CN': '格式', en: 'Format' },
  'archive.level': { 'zh-CN': '压缩级别', en: 'Compression Level' },
  'archive.level.fastest': { 'zh-CN': '最快', en: 'Fastest' },
  'archive.level.standard': { 'zh-CN': '标准', en: 'Standard' },
  'archive.level.smallest': { 'zh-CN': '最小', en: 'Smallest' },
  'archive.password': { 'zh-CN': '密码', en: 'Password' },
  'archive.passwordHint': { 'zh-CN': 'AES-256 加密，留空不加密', en: 'AES-256, empty = none' },
  'archive.passwordSet': { 'zh-CN': '已设置', en: 'Set' },
  'archive.passwordUnset': { 'zh-CN': '未加密', en: 'Not Set' },
  'archive.passwordConfirm': { 'zh-CN': '确认密码', en: 'Confirm Password' },
  'archive.passwordMismatch': { 'zh-CN': '两次输入不一致', en: 'Passwords do not match' },
  'archive.passwordDialogHint': {
    'zh-CN': '两次输入一致后确定；均留空确定则清除密码',
    en: 'Enter twice to confirm; leave both empty to clear',
  },
  'archive.outputName': { 'zh-CN': '归档名称', en: 'Archive Name' },
  'archive.compressN': { 'zh-CN': '压缩 {n} 项', en: 'Compress {n} Items' },
  'archive.andMore': { 'zh-CN': '等 {n} 项', en: '{n} items' },
  'archive.junkFilter': {
    'zh-CN': '过滤系统垃圾文件',
    en: 'Filter System Junk',
  },
  'archive.junkFilterHint': {
    'zh-CN': '.DS_Store、__MACOSX、._ 资源分叉等',
    en: '.DS_Store, __MACOSX, AppleDouble, etc.',
  },
  'archive.hiddenFilter': {
    'zh-CN': '过滤隐藏文件',
    en: 'Filter Hidden Files',
  },
  'archive.hiddenFilterHint': {
    'zh-CN': '点开头的文件/文件夹（含 .git 等）',
    en: 'Dot-prefixed files/folders (incl. .git)',
  },

  // 设置
  'archive.settings.group': { 'zh-CN': '压缩与解压', en: 'Archive' },
  'archive.settings.compressDir': { 'zh-CN': '压缩输出目录', en: 'Compress Output Folder' },
  'archive.settings.compressDirHint': {
    'zh-CN': '留空则每次压缩时选择',
    en: 'Empty = choose each time',
  },
  'archive.clear': { 'zh-CN': '清除固定目录', en: 'Clear Fixed Folders' },
})
