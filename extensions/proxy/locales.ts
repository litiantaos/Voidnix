import { registerMessages } from '@/runtime/i18n'

registerMessages({
  // ─── 组标题 ─────────────────────────────────
  'proxy.group.proxy': { 'zh-CN': '代理', en: 'Proxy' },
  'proxy.group.subscription': { 'zh-CN': '订阅', en: 'Subscriptions' },
  'proxy.group.nodes': { 'zh-CN': '节点', en: 'Nodes' },

  // ─── View 列表项 ────────────────────────────
  'proxy.enableProxy': { 'zh-CN': '开启代理', en: 'Enable Proxy' },
  'proxy.downloadingCore': { 'zh-CN': '正在下载核心…', en: 'Downloading core…' },
  'proxy.coreRequired': {
    'zh-CN': '功能依赖 mihomo 核心，请先下载',
    en: 'Requires mihomo core. Please download it first.',
  },
  'proxy.coreVersion': { 'zh-CN': '核心版本：mihomo {version}', en: 'Core: mihomo {version}' },
  'proxy.newCoreAvailable': {
    'zh-CN': '有新核心 {version}',
    en: 'New core {version} available',
  },
  'proxy.disableToUpdate': { 'zh-CN': '，请关闭代理后更新', en: ' — disable proxy to update' },
  'proxy.clickToDownload': { 'zh-CN': '，点击下载更新', en: ' — click to download update' },
  'proxy.downloadUpdate': { 'zh-CN': '下载更新', en: 'Download Update' },
  'proxy.downloadCore': { 'zh-CN': '下载核心', en: 'Download Core' },
  'proxy.reconnect': { 'zh-CN': '重连', en: 'Reconnect' },

  // ─── 首次启用 TUN 确认 ─────────────────────
  'proxy.tunConfirmTitle': { 'zh-CN': '开启代理（TUN 全局模式）', en: 'Enable Proxy (TUN Mode)' },
  'proxy.tunConfirmMessage': {
    'zh-CN': `首次开启将安装系统级组件，需要输入管理员密码（仅此一次）：

- 安装 **LaunchDaemon**（\`/Library/LaunchDaemons/…mihomo.plist\`），mihomo 代理核心以 **root 常驻**运行：开机自启、崩溃自愈
- 开启后 TUN 虚拟网卡接管本机**全部 IP 流量**（含终端、Docker 等原生应用），关闭代理即恢复直通
- 关闭代理不卸载常驻组件，可在代理设置（搜索栏齿轮）**完全卸载**彻底移除`,
    en: `First-time setup installs system-level components and asks for your administrator password (once only):

- A **LaunchDaemon** (\`/Library/LaunchDaemons/…mihomo.plist\`) is installed; the mihomo core runs as a **persistent root process** (starts at login, auto-restarts on crash)
- Once enabled, a TUN virtual interface takes over **all IP traffic** (terminal, Docker and native apps included); disabling restores direct connection
- Disabling keeps the resident components; use **Full Uninstall** in proxy settings (gear icon) to remove them entirely`,
  },
  'proxy.tunConfirmOk': { 'zh-CN': '安装并开启', en: 'Install & Enable' },

  // ─── 设置子视图 ────────────────────────────
  'proxy.openSettings': { 'zh-CN': '代理设置', en: 'Proxy Settings' },
  'proxy.settingsGroup': { 'zh-CN': '系统', en: 'System' },
  'proxy.settingsGroupGeneral': { 'zh-CN': '通用', en: 'General' },
  'proxy.menubarToggle': { 'zh-CN': '在菜单栏图标菜单中显示', en: 'Show in Menu Bar Icon Menu' },
  'proxy.uninstall': { 'zh-CN': '完全卸载', en: 'Full Uninstall' },
  'proxy.uninstallHint': {
    'zh-CN': '移除 root 常驻进程、LaunchDaemon 与核心文件（需管理员密码）',
    en: 'Remove the root process, LaunchDaemon and core files (admin password required)',
  },
  'proxy.uninstallTitle': { 'zh-CN': '完全卸载代理核心', en: 'Full Uninstall' },
  'proxy.uninstallConfirmMessage': {
    'zh-CN': `将停止代理并以管理员身份移除：

- \`/Library/LaunchDaemons\` 下的 mihomo LaunchDaemon（root 常驻、开机自启）
- mihomo 核心与全部运行文件（数据目录内）

订阅与端口配置保留，重新下载核心即可恢复使用。`,
    en: `This stops the proxy and removes with administrator privileges:

- The mihomo LaunchDaemon in \`/Library/LaunchDaemons\` (persistent root process, starts at login)
- The mihomo core and all runtime files (in the data directory)

Subscriptions and port settings are kept; download the core again to reuse.`,
  },
  'proxy.uninstallConfirmOk': { 'zh-CN': '卸载', en: 'Uninstall' },
  'proxy.uninstalled': { 'zh-CN': '已完全卸载代理核心', en: 'Proxy core fully uninstalled' },
  'proxy.uninstallFailed': { 'zh-CN': '卸载失败', en: 'Uninstall failed' },
  'proxy.ruleMode': { 'zh-CN': '规则模式', en: 'Rule Mode' },
  'proxy.ruleModeHint': {
    'zh-CN': '规则按分流策略，全局代理所有流量',
    en: 'Rule splits traffic by policies; Global proxies all traffic',
  },
  'proxy.addSubscription': { 'zh-CN': '添加订阅', en: 'Add Subscription' },
  'proxy.editSubscription': { 'zh-CN': '编辑订阅', en: 'Edit Subscription' },
  'proxy.updateSubscription': { 'zh-CN': '更新订阅', en: 'Update Subscription' },
  'proxy.locateSelected': { 'zh-CN': '定位到选中节点', en: 'Locate Selected' },
  'proxy.testAll': { 'zh-CN': '全部测速', en: 'Test All' },
  'proxy.unnamedSubscription': { 'zh-CN': '未命名订阅', en: 'Unnamed Subscription' },
  'proxy.subscriptionInfo': {
    'zh-CN': '{count} 节点 · {time}',
    en: '{count} nodes · {time}',
  },
  'proxy.notConfigured': { 'zh-CN': '未配置', en: 'Not configured' },
  'proxy.nodeGroup': { 'zh-CN': '节点分组', en: 'Node Group' },
  'proxy.nodeGroupHint': {
    'zh-CN': '当前显示的 selector 分组',
    en: 'Currently displayed selector group',
  },

  // ─── 订阅编辑弹窗 ───────────────────────────
  'proxy.subscriptionName': { 'zh-CN': '订阅名称', en: 'Subscription Name' },
  'proxy.subscriptionNamePlaceholder': {
    'zh-CN': '默认为订阅链接域名',
    en: 'Defaults to subscription URL domain',
  },
  'proxy.subscriptionUrl': { 'zh-CN': '订阅链接', en: 'Subscription URL' },
  'proxy.subscriptionUrlPlaceholder': {
    'zh-CN': '订阅 URL 或 Clash YAML URL',
    en: 'Subscription URL or Clash YAML URL',
  },
  'proxy.deleteSubscription': { 'zh-CN': '删除订阅', en: 'Delete Subscription' },
  'proxy.deleteConfirm': { 'zh-CN': '确定删除「{name}」？', en: 'Delete "{name}"?' },

  // ─── 规则模式 ───────────────────────────────
  'proxy.mode.rule': { 'zh-CN': '规则', en: 'Rule' },
  'proxy.mode.global': { 'zh-CN': '全局', en: 'Global' },

  // ─── Actions ───────────────────────────────
  'proxy.connections': { 'zh-CN': '连接', en: 'Connections' },
  'proxy.rules': { 'zh-CN': '规则', en: 'Rules' },
  'proxy.logs': { 'zh-CN': '日志', en: 'Logs' },

  // ─── 子视图空态 ─────────────────────────────
  'proxy.noActiveConnections': { 'zh-CN': '无活动连接', en: 'No active connections' },
  'proxy.noLogs': { 'zh-CN': '无日志', en: 'No logs' },
  'proxy.noRules': { 'zh-CN': '无规则', en: 'No rules' },

  // ─── 下载进度 ───────────────────────────────
  'proxy.extracting': { 'zh-CN': '解压中', en: 'Extracting' },
  'proxy.downloading': { 'zh-CN': '下载中', en: 'Downloading' },
  'proxy.notUpdated': { 'zh-CN': '未更新', en: 'Never updated' },

  // ─── logic ─────────────────────────────────
  'proxy.timeout': { 'zh-CN': '超时', en: 'Timeout' },

  // ─── toast / 状态反馈 ───────────────────────
  'proxy.switchFailed': { 'zh-CN': '切换失败', en: 'Failed to toggle proxy' },
  'proxy.coreDownloadFailed': { 'zh-CN': '核心下载失败', en: 'Core download failed' },
  'proxy.coreUpdated': { 'zh-CN': '核心已更新', en: 'Core updated' },
  'proxy.coreUpdateFailed': { 'zh-CN': '核心更新失败', en: 'Core update failed' },
  'proxy.reconnected': { 'zh-CN': '代理已重连', en: 'Proxy reconnected' },
  'proxy.reconnectFailed': { 'zh-CN': '重连失败', en: 'Reconnect failed' },
  'proxy.loadNodesFailed': { 'zh-CN': '加载节点失败', en: 'Failed to load nodes' },
  'proxy.switchNodeFailed': { 'zh-CN': '切换节点失败', en: 'Failed to switch node' },
  'proxy.switchModeFailed': { 'zh-CN': '切换模式失败', en: 'Failed to switch mode' },
  'proxy.switchSubscriptionFailed': {
    'zh-CN': '切换订阅失败',
    en: 'Failed to switch subscription',
  },
  'proxy.nodesUpdated': { 'zh-CN': '已更新 {count} 个节点', en: 'Updated {count} nodes' },
  'proxy.updateFailed': { 'zh-CN': '更新失败', en: 'Update failed' },
  'proxy.cleanupSubscriptionFailed': {
    'zh-CN': '清理订阅失败',
    en: 'Failed to clean up subscription',
  },
})
