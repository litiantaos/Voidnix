// 语义常量单一真相源（仅前端；Rust 端无常量文件）。
// 不可配置；可调参数走 config 系统（stores/settings.ts + defineConfig）。

/**
 * accent 主色 hex。全链路仅两个源：theme.css --color-accent 与此常量（WKWebView 下
 * SVG 属性与 UnoCSS color-mix 需字面值，无法走 var() 串联）。改色两处同步。
 */
export const ACCENT_HEX = '#3d82f0'

export const SEARCH = {
  // fuzzy 权重（utils/fuzzy.ts 消费）
  WEIGHTS: {
    prefix: 1000, // 子串前缀命中基分
    contains: 600, // 子串包含命中基分
    pinyinBase: 400, // 拼音匹配基分
    decay: 0.85, // 多字段权重衰减
    logBase: 2, // frequencyBoost log 底
    logMul: 150, // frequencyBoost log 乘子（150：use_count=10→519，足以跨越 prefix/contains fuzzy 差距 ~400）
    cap: 1500, // frequencyBoost 上限（1500：使用频率占 finalScore ~55%，常访问文件可靠前置）
  },
  // 组间定序严格锁死；不设组级 GROUP_BOOST（GROUP_ORDER 已定组间序）。
  // 扩展用 per-item boost（SearchResult.boost）调整组内优先级。
  // 顺序按使用频率第一性推导：启动应用 / 扩展工具 / 查找文件 / 剪贴板辅助 / web 垫底。
  // 组标题由 i18n 提供（t('group.application') 等），MainView 的 groupTitle() 回调消费。
  GROUP_ORDER: ['application', 'extension', 'file', 'clipboard', 'web'] as const,
  // keywordSearchAll 产出的扩展入口结果组内加权（原 ext-helpers.ts:45 魔数）
  KEYWORD_EXTENSION_BOOST: 500,
} as const

/** 更新检查：唤起节流（窗口获焦时检查，距上次 ≥ 此间隔才真正执行；失败也计入冷却）。 */
export const UPDATE = {
  checkIntervalMs: 6 * 60 * 60 * 1000, // 6h
} as const

export const LIMITS = {
  /** 非 file 组组内限流（application / extension / clipboard / web 共用）。
   *  启动器场景用户极少翻到第 12 项之后；收紧以削减每次搜索的 DOM 节点峰值（遏制 WebKit RSS 高水位）。 */
  maxGroupResults: 12,
  maxFileResults: 20, // file 组限流（含 folder；单组计数，无跨组共享）
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
