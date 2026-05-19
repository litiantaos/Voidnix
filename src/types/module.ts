import type { Component } from 'vue'

export interface SearchResult {
  id: string
  title: string
  description?: string
  icon?: string
  shortcut?: string
  module: string
  score?: number
  data?: {
    path?: string
    kind?: string
    icon?: string | null
    [key: string]: unknown
  }
}

/**
 * 模块布局声明。集中所有"模块向 App Shell 贡献的 UI 槽位"。
 * 槽位名描述位置而非外形：
 *   - view: 主视图区
 *   - header / footer: 视图上下方的固定 chrome
 *   - searchBarAccessory: 全局搜索栏右侧的附属区域
 *
 * 视图内部的私有 UI（如截图标注调色板）不属于此契约，
 * 由模块自行组合，禁止占用 toolbar / header / footer 等会与槽位混淆的命名。
 */
export interface ModuleLayout {
  /** 主视图组件 */
  view: Component
  /** 视图上方固定区域（如标签栏） */
  header?: Component
  /** 视图下方固定区域（如操作栏） */
  footer?: Component
  /** 全局搜索栏右侧的附属区域（模型选择器、状态标签、按钮组等，内容不限） */
  searchBarAccessory?: Component
}

export interface AppModule {
  id: string
  name: string
  description: string
  icon: string
  keywords: string[]
  shortcut?: string
  placeholder?: string

  /**
   * 扩展列表中的显示排序权重（越小越靠前）。
   * 未设置的模块将默认排在最后。
   */
  order?: number

  /**
   * 是否在扩展列表（/唤出的列表）中隐藏。
   * 适用于提供底层全局搜索能力（如文件搜索、应用搜索），但不需要独立视图入口的模块。
   */
  hidden?: boolean

  /**
   * 模块独立窗口的视图组件映射。
   * key 为窗口 label 或 label 前缀（如 'pin-'），value 为对应的根组件。
   * App.vue 挂载时会检查 Tauri 窗口的 label，匹配时优先渲染该组件。
   */
  windowViews?: Record<string, Component>

  onInit?(): Promise<void>
  onActivate?(): Promise<void>
  onDeactivate?(): Promise<void>

  /**
   * 模块布局。声明视图及其 chrome（header/footer）以及对外壳的槽位贡献。
   * 不提供时，ContentView 使用标准列表视图。
   */
  layout?: ModuleLayout

  // 在全局搜索中使用的搜索回调
  onSearch?(query: string): Promise<SearchResult[]>

  // 在模块内部（激活状态下）使用的搜索回调（供标准列表视图使用）
  onModuleSearch?(query: string): Promise<SearchResult[]>

  // 执行某项结果时的回调
  onExecute?(result: SearchResult): Promise<void>

  /**
   * 是否使用主搜索框作为输入源。
   * 启用后，模块视图内的独立输入框应移除，用户直接在搜索框输入并按 Enter 提交。
   */
  useSearchInput?: boolean

  /**
   * 当 useSearchInput 为 true 时，用户在搜索框按 Enter 触发的回调。
   */
  onSearchInput?(query: string): Promise<void>

  /**
   * 按 Enter 提交后是否保留搜索框内容（默认清空）。
   */
  keepSearchInput?: boolean

  /**
   * 模块二级面板组件（如配置页、功能结果页等）。
   * 激活后占满内容区，隐藏模块主视图的 header/footer chrome。
   */
  panel?: Component

  /**
   * 外部通过 `open-module-panel` 事件触发打开面板时的回调。
   * 由框架层统一分发，模块在此解析 payload 并更新内部状态。
   */
  onOpenPanel?(payload: unknown): void | Promise<void>

  /**
   * 模块在全局范围注册的快捷键及其处理逻辑
   */
  globalShortcuts?: {
    /** 快捷键的唯一标识，用于在配置文件中存放用户自定义按键 */
    id: string
    /** 快捷键的默认值 */
    default?: string
    /** 快捷键触发后的执行回调，传递被触发时应用是否处于可见状态 */
    onExecute: (wasVisible: boolean) => void
  }[]

  

  /**
   * 禁用主搜索框（输入框置 disabled，不响应键入）。
   * 适用于无搜索语义的模块（如 OCR 结果展示）。
   */
  disableSearchInput?: boolean

  /**
   * 搜索框是否允多行输入（例如翻译、对话模块）
   */
  multiline?: boolean
}