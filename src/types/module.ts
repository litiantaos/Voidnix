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
 * 视图内部的私有 UI（如截图标注调色板）不属于模块槽位契约，
 * 由模块自行组合，禁止占用 toolbar / header / footer 等会与槽位混淆的命名。
 */

export interface ModuleSearchItem {
  id: string
  title: string
  subtitle?: string
  icon?: string
  keywords: string[]
  group?: string
  [key: string]: unknown
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
   * 无自定义 view 时，ContentView 标准列表的行为配置。
   * 有 view 时此字段无效。
   */
  listOptions?: {
    multiSelect?: boolean
  }

  /**
   * 模块独立窗口的视图组件映射。
   * key 为窗口 label 或 label 前缀（如 'pin-'），value 为对应的根组件。
   * App.vue 挂载时会检查 Tauri 窗口的 label，匹配时优先渲染该组件。
   */
  windowViews?: Record<string, Component>

  onInit?(): void | Promise<void>
  onActivate?(): void | Promise<void>
  onDeactivate?(): void | Promise<void>

  /** 主视图组件。不提供时，ContentView 使用标准列表视图 */
  view?: Component

  /** 搜索栏右侧附属区域（模型选择器、状态标签、按钮组等，内容不限） */
  searchBarAccessory?: Component

  /**
   * 命名子视图。key 为子视图标识，激活时替换主视图占满内容区。
   * 适用于配置页、功能结果页等二级界面。
   */
  subviews?: Record<string, Component>

  onSearch?(query: string): Promise<SearchResult[]>

  onModuleSearch?(query: string): Promise<SearchResult[]>

  /**
   * 声明式搜索项。框架自动调用 scoreFields (pinyin-pro) 做模糊匹配（中文/拼音/英文 · 全词/单字/缩写），
   * 并通过 provide('filteredItems') 向 view 子组件提供过滤结果。
   * 适用于设置项、开关项等半静态内容。
   * 动态内容（剪贴板历史、计算器结果等）仍用 onModuleSearch。
   */
  searchItems?: () => ModuleSearchItem[]

  onExecute?(result: SearchResult, selectedResults?: SearchResult[]): Promise<void>

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
   * 外部通过 `open-module-subview` 事件触发打开子视图时的回调。
   * 由框架层统一分发，模块在此解析 payload 并更新内部状态。
   */
  onOpenSubview?(subviewId: string, payload: unknown): void | Promise<void>

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
   * 禁用主搜索框（输入框置 disabled，不响应键入，保留 placeholder）。
   * 适用于模块自行接管输入区的场景（如聊天、翻译使用独立 textarea）。
   */
  disableSearchInput?: boolean

  /**
   * 状态栏 ↵ 动作描述（如 '粘贴'、'复制'、'翻译'）。
   * 未设置时搜索模式默认 '打开'，模块模式不显示 ↵ 提示。
   */
  enterHint?: string

  /**
   * 是否在状态栏显示 ⇧/⌘ 多选提示。
   */
  multiSelectHint?: boolean
}
