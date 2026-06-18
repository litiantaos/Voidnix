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

export interface ModuleSearchItem {
  id: string
  title: string
  subtitle?: string
  icon?: string
  keywords: string[]
  group?: string
  [key: string]: unknown
}

// ─── 组合接口（AppModule = Meta + UI + Search + Lifecycle + Hints）───────

/// 模块元数据
export interface ModuleMeta {
  id: string
  name: string
  description: string
  icon: string
  keywords: string[]
  shortcut?: string
  /** 扩展列表排序权重（越小越靠前） */
  order?: number
  /** 是否在扩展列表中隐藏 */
  hidden?: boolean
}

/// UI 槽位
export interface ModuleUI {
  /** 主视图组件（无则 ContentView 用标准列表） */
  view?: Component
  /** 搜索栏右侧附属区域 */
  searchBarAccessory?: Component
  /** 命名子视图（配置页、结果页等二级界面） */
  subviews?: Record<string, Component>
  /** 独立窗口视图映射（key 为窗口 label 或前缀） */
  windowViews?: Record<string, Component>
  /** 搜索框 placeholder */
  placeholder?: string
  /** 标准列表行为配置 */
  listOptions?: { multiSelect?: boolean }
}

/// 搜索能力
export interface ModuleSearch {
  /** 全局搜索聚合（并行调用所有模块） */
  onSearch?(query: string): Promise<SearchResult[]>
  /** 模块激活时的本地搜索 */
  onModuleSearch?(query: string): Promise<SearchResult[]>
  /** 半静态声明式搜索项（框架自动 scoreFields 模糊匹配） */
  searchItems?: () => ModuleSearchItem[]
  /** 结果项执行回调 */
  onExecute?(result: SearchResult, selectedResults?: SearchResult[]): Promise<void>
  /** 是否复用主搜索框作为输入 */
  useSearchInput?: boolean
  /** useSearchInput 时 Enter 回调 */
  onSearchInput?(query: string): Promise<void>
  /** Enter 提交后是否保留搜索框内容 */
  keepSearchInput?: boolean
  /** 禁用主搜索框（模块自管输入） */
  disableSearchInput?: boolean
}

/// 生命周期钩子
export interface ModuleLifecycle {
  onInit?(): void | Promise<void>
  onActivate?(): void | Promise<void>
  onDeactivate?(): void | Promise<void>
  onOpenSubview?(subviewId: string, payload: unknown): void | Promise<void>
  /** 全局快捷键注册 */
  globalShortcuts?: {
    id: string
    default?: string
    onExecute: (wasVisible: boolean) => void
  }[]
}

/// 状态栏提示
export interface ModuleHints {
  /** ↵ 动作描述 */
  enterHint?: string
  /** 是否显示 ⇧/⌘ 多选提示 */
  multiSelectHint?: boolean
  /** ⌘⌫ 删除动作描述 */
  deleteHint?: string
}

/// 扩展模块接口 = 元数据 + UI 槽位 + 搜索能力 + 生命周期 + 状态栏提示
export interface AppModule extends ModuleMeta, ModuleUI, ModuleSearch, ModuleLifecycle, ModuleHints {}
