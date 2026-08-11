import { registerMessages } from '@/runtime/i18n'

registerMessages({
  // ─── 状态行 ──────────────────────────────
  'homebrew.updates': { 'zh-CN': '{count} 更新', en: '{count} updates' },
  'homebrew.processing': { 'zh-CN': '处理中', en: 'Processing' },
  'homebrew.update': { 'zh-CN': '更新', en: 'Update' },

  // ─── 服务 ──────────────────────────────
  'homebrew.services': { 'zh-CN': '服务', en: 'Services' },
  'homebrew.start': { 'zh-CN': '启动', en: 'Start' },
  'homebrew.stop': { 'zh-CN': '停止', en: 'Stop' },
  'homebrew.restart': { 'zh-CN': '重启', en: 'Restart' },
  'homebrew.running': { 'zh-CN': '运行中', en: 'Running' },
  'homebrew.stopped': { 'zh-CN': '已停止', en: 'Stopped' },
  'homebrew.error': { 'zh-CN': '错误', en: 'Error' },

  // ─── 空态 ──────────────────────────────
  'homebrew.noMatch': { 'zh-CN': '无匹配包', en: 'No matching packages' },
  'homebrew.noInstalled': { 'zh-CN': '无已安装包', en: 'No installed packages' },
  'homebrew.noMatchDetail': { 'zh-CN': '无匹配', en: 'No match' },

  // ─── 详情 ──────────────────────────────
  'homebrew.dependencies': { 'zh-CN': '依赖', en: 'Dependencies' },
  'homebrew.dependents': { 'zh-CN': '被依赖', en: 'Dependents' },
  'homebrew.missingPackageInfo': { 'zh-CN': '缺少包信息', en: 'Missing package info' },
  'homebrew.uninstall': { 'zh-CN': '卸载', en: 'Uninstall' },
  'homebrew.uninstalled': { 'zh-CN': '已卸载 {name}', en: 'Uninstalled {name}' },
  'homebrew.uninstallTitle': { 'zh-CN': '卸载 {name}？', en: 'Uninstall {name}?' },
  'homebrew.uninstallDepMsg': {
    'zh-CN': '此包被 {count} 个包依赖（{names}）',
    en: 'Depended on by {count} packages ({names})',
  },
  'homebrew.uninstallOrphanMsg': {
    'zh-CN': '孤立的依赖将自动清理',
    en: 'Orphaned dependencies will be cleaned up',
  },
  'homebrew.uninstallConfirm': {
    'zh-CN': '确定要卸载此包吗？',
    en: 'Are you sure you want to uninstall this package?',
  },

  // ─── 操作步骤 ──────────────────────────
  'homebrew.step.update': { 'zh-CN': '拉取更新', en: 'Fetching updates' },
  'homebrew.step.upgrade': { 'zh-CN': '升级中', en: 'Upgrading' },
  'homebrew.step.cleanup': { 'zh-CN': '清理中', en: 'Cleaning' },
  'homebrew.step.autoremove': { 'zh-CN': '清理依赖', en: 'Removing dependencies' },
  'homebrew.step.uninstall': { 'zh-CN': '卸载中', en: 'Uninstalling' },
  'homebrew.step.servicesStart': { 'zh-CN': '启动中', en: 'Starting' },
  'homebrew.step.servicesStop': { 'zh-CN': '停止中', en: 'Stopping' },
  'homebrew.step.servicesRestart': { 'zh-CN': '重启中', en: 'Restarting' },

  // ─── 完成 ──────────────────────────────
  'homebrew.updateDone': { 'zh-CN': '更新完成', en: 'Update complete' },
})
