// 语义常量单一真相源（仅前端；Rust 端无常量文件）。
// 不可配置；可调参数走 config 系统（stores/settings.ts + defineConfig）。

export const SEARCH = {
  // fuzzy 权重（utils/fuzzy.ts 消费）
  WEIGHTS: {
    prefix: 1000, // 子串前缀命中基分
    contains: 600, // 子串包含命中基分
    pinyinBase: 400, // 拼音匹配基分
    decay: 0.85, // 多字段权重衰减
    logBase: 2, // frequencyBoost log 底
    logMul: 50, // frequencyBoost log 乘子
    cap: 320, // frequencyBoost 上限
  },
  // 组间定序严格锁死；不设组级 GROUP_BOOST（GROUP_ORDER 已定组间序）。
  // 扩展用 per-item boost（SearchResult.boost）调整组内优先级。
  // 顺序按使用频率第一性推导：启动应用 / 扩展工具 / 查找文件 / 剪贴板辅助 / web 垫底。
  GROUP_ORDER: ['application', 'module', 'file', 'clipboard', 'web'] as const,
  GROUP_TITLES: {
    application: '应用',
    file: '文件', // file 与 folder 共用（同组，仅 kind 值区分）
    module: '扩展',
    clipboard: '剪贴板',
    web: '快捷操作',
  },
  // keywordSearchAll 产出的模块入口结果组内加权（原 module-helpers.ts:45 魔数）
  KEYWORD_MODULE_BOOST: 500,
} as const

export const LIMITS = {
  maxAppResults: 30,
  maxFileResults: 50, // file 组限流（含 folder；单组计数，无跨组共享）
  searchTimeoutMs: 3000,
} as const

// 主窗口尺寸（不可配置；与 tauri.conf.json 主窗口 width/height 一致）。
// 扩展通过 Extension.windowHeight 声明模块激活时的高度，框架 clamp 到 [MIN, MAX]。
// 搜索栏 chrome 拆成 top/height/gap 再求和，改 MainView top-*/h-* 时同步改这三项。
const SEARCH_BAR_TOP = 8 // top-2
const SEARCH_BAR_HEIGHT = 52 // h-13
const SEARCH_BAR_GAP = 8 // 栏底与内容间距
/** chrome 渐隐遮罩伸入内容的尾部长（视觉软边，不计入 paddingTop） */
const CHROME_FADE_EXTRA = 24
export const WINDOW = {
  WIDTH: 720,
  DEFAULT_HEIGHT: 480,
  SEARCH_BAR_TOP,
  SEARCH_BAR_HEIGHT,
  SEARCH_BAR_GAP,
  CHROME_FADE_EXTRA,
  /** 悬浮搜索栏占用高 = top + height + gap；scrollContainer paddingTop / scroll-padding-top / auto 高度共用 */
  CHROME_HEIGHT: SEARCH_BAR_TOP + SEARCH_BAR_HEIGHT + SEARCH_BAR_GAP,
  /** 渐隐遮罩总高 = chrome + 伸入内容的尾部 */
  CHROME_FADE_HEIGHT: SEARCH_BAR_TOP + SEARCH_BAR_HEIGHT + SEARCH_BAR_GAP + CHROME_FADE_EXTRA,
  MIN_HEIGHT: 360,
  MAX_HEIGHT: 820, // 留余量给菜单栏/Dock
} as const
