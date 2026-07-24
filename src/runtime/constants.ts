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
  GROUP_ORDER: ['application', 'extension', 'file', 'clipboard', 'web'] as const,
  GROUP_TITLES: {
    application: '应用',
    file: '文件', // file 与 folder 共用（同组，仅 kind 值区分）
    extension: '扩展',
    clipboard: '剪贴板',
    web: '快捷操作',
  },
  // keywordSearchAll 产出的扩展入口结果组内加权（原 ext-helpers.ts:45 魔数）
  KEYWORD_EXTENSION_BOOST: 500,
} as const

export const LIMITS = {
  /** 非 file 组组内限流（application / extension / clipboard / web 共用）。 */
  maxGroupResults: 30,
  maxFileResults: 50, // file 组限流（含 folder；单组计数，无跨组共享）
  searchTimeoutMs: 3000,
} as const

// 主窗口尺寸（不可配置；与 tauri.conf.json 主窗口 width/height 一致）。
// 扩展通过 Extension.windowHeight 声明扩展激活时的高度，框架 clamp 到 [MIN, MAX]。
// 搜索栏 chrome 拆成 top/height/gap 再求和，改 MainView top-*/h-* 时同步改这三项。
const SEARCH_BAR_TOP = 12 // top-3
const SEARCH_BAR_HEIGHT = 52 // h-13
const SEARCH_BAR_GAP = 12 // 栏底与内容间距（与全局 p-3 一致）
/** 悬浮搜索栏占用高 = top + height + gap；scrollContainer / chrome-fade 共用 */
const CHROME_HEIGHT = SEARCH_BAR_TOP + SEARCH_BAR_HEIGHT + SEARCH_BAR_GAP
/** 内容区内边距（与全局 p-3 / 列表 px-3 pb-3 一致；scroll-padding-bottom / 选中项贴边对齐共用） */
const CONTENT_INSET = 12
export const WINDOW = {
  WIDTH: 720,
  DEFAULT_HEIGHT: 480,
  CONTENT_INSET,
  /** 悬浮搜索栏占用高；scrollContainer paddingTop / scroll-padding-top / auto 高度共用 */
  CHROME_HEIGHT,
  /**
   * 渐隐遮罩总高 = chrome（与内容顶对齐；与 CHROME_HEIGHT 同值）。
   * 勿再伸入内容区：静止时会糊住翻译输入框等顶边控件；列表上滚进 gap/栏底时自然进入 mask 软边。
   */
  CHROME_FADE_HEIGHT: CHROME_HEIGHT,
  MIN_HEIGHT: 360,
  MAX_HEIGHT: 820, // 留余量给菜单栏/Dock
} as const
