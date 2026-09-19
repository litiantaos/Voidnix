/** 访达工具启动快捷键（可被 settings.shortcutOverrides 覆盖）。 */
export const FINDER_SHORTCUT = {
  id: 'finder-ext',
  default: 'Alt+F',
} as const

export type FinderAction =
  'copy_path' | 'open_terminal' | 'new_file' | 'toggle_hidden' | 'open_with'

/** 面板目录行 id：finder_run_action 动作 + 选区媒体处理入口（跨扩展跳转，非命令）。 */
export type FinderEntryId = FinderAction | 'video_process' | 'image_process'

/** 面板目录单一数据源，顺序即正式面板序（用 App 打开 → 媒体入口 → 其余动作）。
 * 上下文模式（访达快捷键）：open_with 由「用 App 打开」候选组承载（目录行省略）、
 * 媒体入口仅有选区时出现；浏览模式（应用界面进入）：全量显示。 */
export const FINDER_CATALOG: {
  id: FinderEntryId
  titleKey: string
  icon: string
}[] = [
  {
    id: 'open_with',
    titleKey: 'finderExt.action.openWith',
    icon: 'i-ri-apps-2-line',
  },
  {
    id: 'video_process',
    titleKey: 'finderExt.videoProcess',
    icon: 'i-ri-video-line',
  },
  {
    id: 'image_process',
    titleKey: 'finderExt.imageProcess',
    icon: 'i-ri-image-edit-line',
  },
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
