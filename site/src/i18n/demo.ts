// Demo 动画双语文案。被 demo-utils / demo-scenes / demo-player / DemoStage 共用。
// 与页面文案分离——demo 在浏览器端运行，按 data-lang 属性取值。

import type { Lang } from './translations'

type SegId = 'search' | 'clipboard' | 'agent' | 'shot' | 'snap' | 'finder'

export interface DemoText {
  // demo-utils 段字幕
  caps: Record<SegId, string>
  // demo-scenes 动态文本
  searchPlaceholder: string
  clipboardTag: string
  clipboardSearch: string
  clipboardPasted: string
  agentTag: string
  agentSearch: string
  agentUser: string
  agentToolOut: string
  agentResult: string
  ocrTag: string
  ocrSearch: string
  ocrText: string
  ocrAction: string
  shotCapA: string
  shotCapB: string
  shotCapC: string
  grpApp: string
  grpFile: string
  grpOps: string
  // DemoStage 静态文本
  finderTag: string
  finderSearch: string
  finderActions: string[]
  finderSidebar: string[]
  agentInputPlaceholder: string
  toastPasted: string
  shotTools: {
    rect: string
    line: string
    arrow: string
    text: string
    blur: string
    ocr: string
    scroll: string
    pin: string
    save: string
    close: string
    copy: string
  }
  // demo-player 控件
  segBtns: string[]
  btnPlayAll: string
  btnPlaySeg: string
  playAria: string
}

const zh: DemoText = {
  caps: {
    search: '全局搜索：应用、文件、扩展',
    clipboard: '扩展模式：剪贴板历史',
    agent: 'Agent：自然语言驱动工具',
    shot: '截屏：标注 + 滚动截屏 + OCR',
    snap: '窗口管理：鼠标顶部触发分屏',
    finder: '访达工具：快捷键操作 Finder',
  },
  searchPlaceholder: '搜索应用、文件、扩展等',
  clipboardTag: '剪贴板',
  clipboardSearch: '在 剪贴板 中搜索',
  clipboardPasted: 'const FPS = 30',
  agentTag: 'Agent',
  agentSearch: '在 Agent 中搜索',
  agentUser: '列出 extensions 目录下的 ts 文件',
  agentToolOut: '23 个 index.ts，含 native 的 16 个',
  agentResult:
    '找到 23 个 index.ts，含 native 的 16 个。\n\n这些文件分布在 extensions/ 下的每个扩展目录中，纯 TS 扩展（calculator、ip 等）同样包含 index.ts。还需要其他帮助吗？',
  ocrTag: 'OCR',
  ocrSearch: '识别结果',
  ocrText: 'Voidnix — macOS 效率启动器\n模块化扩展架构\nRust + Vue 3 + Tauri 2',
  ocrAction: '复制',
  shotCapA: '截屏标注：拉出选区 + 标注工具',
  shotCapB: '滚动截屏：长内容连续捕获',
  shotCapC: '截图 OCR：文字识别与复制',
  grpApp: '应用',
  grpFile: '文件',
  grpOps: '操作',
  finderTag: '访达工具',
  finderSearch: '在 访达工具 中搜索',
  finderActions: ['拷贝路径', '在终端中打开', '新建文件', '切换隐藏文件'],
  finderSidebar: ['下载', '文稿'],
  agentInputPlaceholder: '聊点什么...',
  toastPasted: '已粘贴',
  shotTools: {
    rect: '矩形',
    line: '直线',
    arrow: '箭头',
    text: '文字',
    blur: '模糊',
    ocr: '识别',
    scroll: '滚动截屏',
    pin: '钉图',
    save: '保存',
    close: '关闭',
    copy: '复制并关闭',
  },
  segBtns: ['搜索', '剪贴板', 'Agent', '截屏', '窗口管理', '访达工具'],
  btnPlayAll: '连续播放',
  btnPlaySeg: '分段播放',
  playAria: '暂停/播放',
}

const en: DemoText = {
  caps: {
    search: 'Global Search: Apps, files, extensions',
    clipboard: 'Extension Mode: Clipboard history',
    agent: 'Agent: Natural language tool calling',
    shot: 'Screenshot: Annotate + scroll capture + OCR',
    snap: 'Window Management: Cursor-top snap trigger',
    finder: 'Finder Tools: Shortcut-driven Finder actions',
  },
  searchPlaceholder: 'Search apps, files, extensions...',
  clipboardTag: 'Clipboard',
  clipboardSearch: 'Search in Clipboard',
  clipboardPasted: 'const FPS = 30',
  agentTag: 'Agent',
  agentSearch: 'Search in Agent',
  agentUser: 'List the .ts files in the extensions directory',
  agentToolOut: '23 index.ts files, 16 with native code',
  agentResult:
    'Found 23 index.ts files, 16 with native code.\n\nThese files are in each extension directory under extensions/. Pure-TS extensions (calculator, ip, etc.) also contain index.ts. Need anything else?',
  ocrTag: 'OCR',
  ocrSearch: 'Recognition result',
  ocrText:
    'Voidnix — macOS Productivity Launcher\nModular extension architecture\nRust + Vue 3 + Tauri 2',
  ocrAction: 'Copy',
  shotCapA: 'Screenshot annotation: drag selection + tools',
  shotCapB: 'Scrolling capture: continuous long content',
  shotCapC: 'Screenshot OCR: text recognition & copy',
  grpApp: 'Apps',
  grpFile: 'Files',
  grpOps: 'Actions',
  finderTag: 'Finder Tools',
  finderSearch: 'Search in Finder Tools',
  finderActions: ['Copy Path', 'Open in Terminal', 'New File', 'Toggle Hidden Files'],
  finderSidebar: ['Downloads', 'Documents'],
  agentInputPlaceholder: 'Ask anything...',
  toastPasted: 'Pasted',
  shotTools: {
    rect: 'Rectangle',
    line: 'Line',
    arrow: 'Arrow',
    text: 'Text',
    blur: 'Blur',
    ocr: 'OCR',
    scroll: 'Scroll capture',
    pin: 'Pin',
    save: 'Save',
    close: 'Close',
    copy: 'Copy & close',
  },
  segBtns: ['Search', 'Clipboard', 'Agent', 'Screenshot', 'Windows', 'Finder'],
  btnPlayAll: 'Play all',
  btnPlaySeg: 'Per segment',
  playAria: 'Pause/Play',
}

const demoDicts: Record<Lang, DemoText> = { zh, en }

export function getDemoText(lang: Lang): DemoText {
  return demoDicts[lang]
}
