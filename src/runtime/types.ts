import type { Component } from 'vue'

// ─── 搜索结果（§2.3）─────────────────────────────────────────────────

/** 严格枚举：分组依据。file/folder 同属 'file' 组（getGroupKey 合并，§2.5）。 */
export type SearchResultKind = 'application' | 'folder' | 'file' | 'module' | 'clipboard' | 'web'

export interface SearchData {
  /** 必填：分组依据（经 getGroupKey 映射到组）。扩展须正确设置。 */
  kind: SearchResultKind
  /** 模块入口结果专用：kind==='module' 时必填，框架内置激活此模块（§2.2 执行分派）。 */
  moduleId?: string
  path?: string
  [key: string]: unknown
}

export interface SearchResult {
  /** 扩展内 localId（自管唯一）。框架去重用 `<module>:<id>` 组合键。 */
  id: string
  title: string
  /** 框架自动注入，扩展禁填（v1.6 N4）：dynamic 结果 = 产出扩展 meta.id；keyword 入口 = 目标模块 id。 */
  module: string
  description?: string
  icon?: string
  shortcut?: string
  data?: SearchData
  /** 扩展可选组内优先级提示（默认 0）；框架 finalScore = fuzzy(title,query) + boost。 */
  boost?: number
  /** 仅框架填，扩展禁止填。 */
  score?: number
}

/** 扩展 dynamic 返回的原始结果（不含 module，框架注入 producing 扩展 id，§2.3 v1.6 N4）。 */
export type ProviderResult = Omit<SearchResult, 'module'>

// ─── 搜索能力（§2.3 单通道）──────────────────────────────────────────

export interface SearchContext {
  /** 新查询覆盖旧查询时 abort；持有非自动释放资源的 provider 须 addEventListener('abort', cleanup)。 */
  signal: AbortSignal
  /** true = 模块独占（runModuleSearch 进入模块时调用）；false/缺省 = 全局聚合（searchEngine 默认列表）。
   *  扩展可据此区分：全局空 query 时跳过网络等重操作（避免拖慢默认列表），模块内空 query 正常执行。 */
  moduleMode?: boolean
}

export interface SearchProvider {
  /** 动态召回：每次查询并行调用。模块模式下只调激活模块的 dynamic；全局模式聚合所有扩展。
   *  半静态内容（如 base64 选项）由扩展内部模块级缓存自管，走 dynamic 返回。
   *  返回项禁止带 module（框架注入）与 score（框架重算）。 */
  dynamic(query: string, ctx: SearchContext): ProviderResult[] | Promise<ProviderResult[]>
}

// ─── 扩展接口（§2.2 能力槽）─────────────────────────────────────────

export interface ExtensionMeta {
  id: string
  name: string
  icon: string
  /** 扩展列表排序权重（越小越靠前）。 */
  order: number
  /** 搜索关键词（keywordSearchAll 匹配用）。 */
  keywords?: string[]
  /** 描述（可选；进 keyword 搜索评分字段）。 */
  description?: string
  /** 是否在扩展列表中隐藏。 */
  hidden?: boolean
}

export interface ModuleHints {
  /** ↵ 动作描述（如「粘贴」「复制」）。 */
  enter?: string
  /** 多选提示（如「⇧/⌘ 多选」）。 */
  multiSelect?: string
  /** 删除提示（如「删除」）。 */
  delete?: string
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
  teardown?(): void

  // ── 能力槽（均有真实消费者，§2.2）──
  search?: SearchProvider
  /** 搜索结果回车动作（扩展私有；模块入口结果走框架内置激活，见下方「执行分派」）。 */
  onExecute?(result: SearchResult, selectedResults?: SearchResult[]): void | Promise<void>
  /** 主视图（9 扩展）。 */
  mainView?: () => Component
  /** 搜索栏右侧附属区域（2 扩展：clipboard/agent；translate ⚙️ 随 settingsView 集中化移除，v1.7）。 */
  searchBarAccessory?: () => Component
  /** 扩展私有命名子视图（screenshot{ocr}）。 */
  subviews?: Record<string, () => Component>
  /** 设置片段（3 扩展；跨扩展契约：settings 扩展 mainView 扫描消费）。 */
  settingsView?: () => Component
  /** 独立窗口视图（screenshot{screenshot,pin-*}/window-manager{snap-panel}）。 */
  windowViews?: Record<string, () => Component>
  globalShortcuts?: ShortcutBinding[]
  hints?: ModuleHints
  /** 搜索框占位提示（激活模块时显示）；7 扩展。 */
  placeholder?: string

  // ── 以下为现网活跃消费者承载字段（蓝本精简模型省略，过渡期保留）──
  /** 模块自管输入、禁用主搜索框（translate/agent）。 */
  disableSearchInput?: boolean
  /** 标准列表行为（clipboard 多选）。 */
  listOptions?: { multiSelect?: boolean }
  /** 子视图打开回调（screenshot OCR payload 转交）。 */
  onOpenSubview?(subviewId: string, payload: unknown): void | Promise<void>
}
