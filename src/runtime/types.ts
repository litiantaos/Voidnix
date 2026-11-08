import type { Component } from 'vue'
import type { LocalizedText } from './i18n'

// ─── 搜索结果─────────────────────────────────────────────────

/** 严格枚举：分组依据。file/folder 同属 'file' 组（getGroupKey 合并）。 */
export type SearchResultKind = 'application' | 'folder' | 'file' | 'extension' | 'clipboard' | 'web'

export interface SearchData {
  /** 必填：分组依据（经 getGroupKey 映射到组）。扩展须正确设置。 */
  kind: SearchResultKind
  /** 扩展入口结果专用：kind==='extension' 时必填，框架内置激活此扩展。 */
  extId?: string
  path?: string
  // ── 跨扩展高频共享字段（显式声明以获得类型安全；扩展私有字段走下方索引签名）──
  /** 复制值（currency/ip 等 onExecute 复制的内容）。 */
  value?: string
  /** 结果图标覆盖（ResultIcon 消费）。 */
  icon?: string
  /** 高亮显示（ip 等首项强调）。 */
  isHighlight?: boolean
  /** 图标样式类（web 搜索等）。 */
  iconStyle?: string
  /** 扩展私有字段兜底。 */
  [key: string]: unknown
}

export interface SearchResult {
  /** 扩展内 localId（自管唯一）。框架去重用 `<extId>:<id>` 组合键。 */
  id: string
  title: string
  /** 框架自动注入，扩展禁填：dynamic 结果 = 产出扩展 meta.id；keyword 入口 = 目标扩展 id。 */
  extId: string
  description?: string
  icon?: string
  shortcut?: string
  data?: SearchData
  /** 扩展可选组内优先级提示（默认 0）；框架 finalScore = fuzzy(title,query) + boost。 */
  boost?: number
  /** 仅框架填，扩展禁止填。 */
  score?: number
  /** 框架注入：全局模式 dynamic 工具型结果（kind=extension）的来源扩展显示名，UI 右侧标注。扩展禁填。 */
  source?: string
}

/** 扩展 dynamic 返回的原始结果（不含 extId，框架注入 producing 扩展 id）。 */
export type ProviderResult = Omit<SearchResult, 'extId' | 'source'>

// ─── 搜索能力──────────────────────────────────────────

export interface SearchContext {
  /** 新查询覆盖旧查询时 abort；持有非自动释放资源的 provider 须 addEventListener('abort', cleanup)。 */
  signal: AbortSignal
  /** true = 扩展独占（searchEngine 扩展模式，只调激活扩展）；false/缺省 = 全局聚合。
   *  扩展可据此区分：全局空 query 时跳过网络等重操作（避免拖慢默认列表），扩展内空 query 正常执行。 */
  extensionMode?: boolean
  /** 流式部分结果（可选）：扩展可多次调用先产出快结果，最后 return 补充/最终结果。
   *  不调用的扩展行为不变（一次性 return）。框架对 emit 的结果立即增量重排并回调上层，
   *  消除「快结果等慢结果」的 barrier——如应用缓存命中秒出，mdfind 文件/网络结果后补。
   *  emit 与最终 return 的结果会去重，扩展无需担心重复。 */
  emit?: (results: ProviderResult[]) => void
}

export interface SearchProvider {
  /** 动态召回：每次查询并行调用。扩展模式下只调激活扩展的 dynamic；全局模式聚合所有扩展。
   *  半静态内容（如 base64 选项）由扩展内部缓存自管，走 dynamic 返回。
   *  返回项禁止带 extId（框架注入）与 score（框架重算）。 */
  dynamic(query: string, ctx: SearchContext): ProviderResult[] | Promise<ProviderResult[]>
}

// ─── 扩展接口─────────────────────────────────────────

export interface ExtensionMeta {
  id: string
  name: LocalizedText
  icon: string
  /** 扩展列表排序权重（越小越靠前）。 */
  order: number
  /** 搜索关键词（keywordSearchAll 匹配用）。 */
  keywords?: string[]
  /** 描述（可选；进 keyword 搜索评分字段）。 */
  description?: LocalizedText
  /** 是否在扩展列表中隐藏。 */
  hidden?: boolean
}

export interface ShortcutBinding {
  /** 快捷键业务 id（如 'screenshot'、'translate'）。 */
  id: string
  /** 默认组合（如 'cmd+shift+s'），可被用户覆盖。 */
  default?: string
  onExecute: (wasVisible: boolean) => void
}

export interface Extension {
  meta: ExtensionMeta

  // ── 生命周期 ──
  /** 启动钩子（前端运行时初始化时调用）。 */
  setup?(): void | Promise<void>

  // ── 能力槽（均有真实消费者）──
  search?: SearchProvider
  /** 搜索结果回车动作（扩展私有；扩展入口结果走框架内置激活，见下方「执行分派」）。 */
  onExecute?(result: SearchResult, selectedResults?: SearchResult[]): void | Promise<void>
  /** 主视图。 */
  mainView?: () => Component
  /** 搜索栏右侧附属区域。 */
  searchBarAccessory?: () => Component
  /** 扩展私有命名子视图（如 screenshot 的 ocr、各扩展的 config）。 */
  subviews?: Record<string, () => Component>
  /** 子视图显示名（id → 名称），激活子视图时搜索栏 placeholder 用「搜索{name}」。 */
  subviewTitle?: Record<string, LocalizedText>
  globalShortcuts?: ShortcutBinding[]
  /** 搜索框占位提示（激活扩展时显示）。 */
  placeholder?: LocalizedText
  /** 扩展激活时主窗口高度（逻辑像素）：number = 固定值（clamp 到 [MIN,MAX]）；'auto' = 随内容自适应；未声明 = 默认高度。
   *  mainView 与所有 subviews 共用此值（subview 可经 subviewHeights 覆盖）。 */
  windowHeight?: number | 'auto'
  /** subview 级高度覆盖：key 对应 subviews 字典键，值语义同 windowHeight。 */
  subviewHeights?: Record<string, number | 'auto'>

  // ── 行为槽（与能力槽同等地位的扩展行为契约）──
  /** 扩展自管输入、禁用主搜索框。 */
  disableSearchInput?: boolean
  /** 标准列表行为（如多选）。 */
  listOptions?: { multiSelect?: boolean }
  /** 子视图打开回调（如 OCR payload 转交）。 */
  onOpenSubview?(subviewId: string, payload: unknown): void | Promise<void>
}
