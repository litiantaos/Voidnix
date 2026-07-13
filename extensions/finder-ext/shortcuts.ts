/** 访达工具启动快捷键（可被 settings.shortcutOverrides 覆盖）。 */
export const FINDER_SHORTCUT = {
  id: 'finder-ext',
  default: 'Alt+F',
} as const

export type FinderAction = 'copy_path' | 'open_terminal' | 'new_file' | 'toggle_hidden'

export const FINDER_ACTIONS: {
  id: FinderAction
  title: string
  icon: string
}[] = [
  {
    id: 'copy_path',
    title: '拷贝路径',
    icon: 'i-ri-file-copy-line',
  },
  {
    id: 'open_terminal',
    title: '在终端中打开',
    icon: 'i-ri-terminal-box-line',
  },
  {
    id: 'new_file',
    title: '新建文件',
    icon: 'i-ri-file-add-line',
  },
  {
    id: 'toggle_hidden',
    title: '切换隐藏文件',
    icon: 'i-ri-eye-off-line',
  },
]
