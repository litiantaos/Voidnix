// 声明式 UI 原语：扩展只返回 View 描述，宿主独占渲染

export type View = ListView | MarkdownView | FormView | DetailView | StreamView

// ── List ──────────────────────────────────────────────────

export interface ListView {
  type: 'list'
  items: ListItem[]
  emptyText?: string
  searchPlaceholder?: string
  isLoading?: boolean
}

export interface ListItem {
  id: string
  title: string
  subtitle?: string
  icon?: string
  accessories?: Accessory[]
  actions?: Action[]
}

export type Accessory =
  | { type: 'text'; text: string; color?: TextColor }
  | { type: 'tag'; text: string; color?: TagColor }
  | { type: 'date'; timestamp: number }

export interface Action {
  id: string
  title: string
  icon?: string
  shortcut?: string
  primary?: boolean
  destructive?: boolean
}

// ── Markdown ──────────────────────────────────────────────

export interface MarkdownView {
  type: 'markdown'
  content: string
  actions?: Action[]
}

// ── Form ──────────────────────────────────────────────────

export interface FormView {
  type: 'form'
  fields: FormField[]
  submit: Action
  cancel?: Action
}

export type FormField =
  | {
      id: string
      type: 'text' | 'password' | 'number'
      label: string
      placeholder?: string
      default?: string
      required?: boolean
    }
  | { id: string; type: 'textarea'; label: string; rows?: number; default?: string }
  | {
      id: string
      type: 'select'
      label: string
      options: { value: string; label: string }[]
      default?: string
    }
  | { id: string; type: 'switch'; label: string; default?: boolean }
  | { id: string; type: 'shortcut'; label: string; default?: string }

// ── Detail ────────────────────────────────────────────────

export interface DetailView {
  type: 'detail'
  markdown: string
  metadata?: MetadataEntry[]
  actions?: Action[]
}

export type MetadataEntry =
  | { type: 'label'; title: string; text: string }
  | { type: 'link'; title: string; url: string; text?: string }
  | { type: 'tag'; title: string; tags: { text: string; color?: TagColor }[] }
  | { type: 'separator' }

// ── Stream ────────────────────────────────────────────────

export interface StreamView {
  type: 'stream'
  blocks: StreamBlock[]
  input?: FormField
  onSubmit?: string
  actions?: Action[]
}

export interface StreamBlock {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  done: boolean
  metadata?: { timestamp?: number; model?: string }
}

// ── Colors ────────────────────────────────────────────────

type TextColor = 'primary' | 'secondary' | 'subtle' | 'accent' | 'success' | 'warning' | 'danger'
type TagColor = 'gray' | 'blue' | 'green' | 'yellow' | 'red' | 'purple'
