/** 访达工具启动快捷键（可被 settings.shortcutOverrides 覆盖）。 */
export const FINDER_SHORTCUT = {
  id: 'finder-ext',
  default: 'Alt+F',
} as const

export type FinderAction = 'copy_path' | 'open_terminal' | 'new_file' | 'toggle_hidden'

export const FINDER_ACTIONS: {
  id: FinderAction
  titleKey: string
  icon: string
}[] = [
  {
    id: 'copy_path',
    titleKey: 'finderExt.action.copyPath',
    icon: 'i-ri-file-copy-line',
  },
  {
    id: 'open_terminal',
    titleKey: 'finderExt.action.openTerminal',
    icon: 'i-ri-terminal-box-line',
  },
  {
    id: 'new_file',
    titleKey: 'finderExt.action.newFile',
    icon: 'i-ri-file-add-line',
  },
  {
    id: 'toggle_hidden',
    titleKey: 'finderExt.action.toggleHidden',
    icon: 'i-ri-eye-off-line',
  },
]
