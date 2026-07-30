// 设置项类型（BaseSettingsList + 各扩展 Settings.vue 消费）。

export interface SettingSelectOption {
  label: string
  value: string | number
}
export interface SettingSelectOptionGroup {
  label: string
  options: SettingSelectOption[]
}
export type SettingSelectOptions = (SettingSelectOption | SettingSelectOptionGroup)[]

interface SettingItemBase {
  id: string
  /** 标题；action/custom 类型若整体 slot 覆盖可省略 */
  title?: string
  subtitle?: string
  icon?: string
  group?: string
  /** 标题色调（accent 强调 / danger 危险），透传 BaseListItem.tone */
  tone?: 'accent' | 'danger'
}

export type SettingItem =
  | (SettingItemBase & { type: 'shortcut'; value: string; update: (v: string) => void })
  | (SettingItemBase & {
      type: 'select'
      value: string | number
      options: SettingSelectOptions
      update: (v: string | number) => void
    })
  | (SettingItemBase & {
      type: 'button'
      label?: string
      variant?: 'default' | 'primary' | 'danger'
      action: () => void
    })
  | (SettingItemBase & { type: 'toggle'; value: boolean; update: (v: boolean) => void })
  | (SettingItemBase & { type: 'action'; action: () => void })
  | (SettingItemBase & { type: 'custom' })
