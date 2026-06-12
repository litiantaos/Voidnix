// Manifest.toml 结构（与 Rust 端 ext_manifest.rs 对应）

export interface Manifest {
  extension: ExtensionMeta
  entry: Entry
  capabilities?: Capabilities
  ui?: UiConfig
  settings?: SettingField[]
  shortcuts?: ShortcutDef[]
  signature?: Signature
}

export interface ExtensionMeta {
  id: string
  name: string
  version: string
  description?: string
  author?: string
  icon?: string
  license?: string
  homepage?: string
  keywords?: string[]
  voidnix_api?: string
}

export interface Entry {
  main?: string
}

export interface Capabilities {
  required?: string[]
  optional?: string[]
}

export interface UiConfig {
  preferred_view?: string
  search_placeholder?: string
  disable_search_input?: boolean
}

export type SettingField =
  | {
      id: string
      type: 'text'
      label: string
      placeholder?: string
      default?: string
      required?: boolean
    }
  | { id: string; type: 'number'; label: string; default?: number }
  | { id: string; type: 'switch'; label: string; default?: boolean }
  | {
      id: string
      type: 'select'
      label: string
      options: { value: string; label: string }[]
      default?: string
    }

export interface ShortcutDef {
  id: string
  default?: string
  description?: string
}

export interface Signature {
  algorithm?: string
  public_key: string
  signature: string
}
