import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'systemStatus.permissionHint': {
    'zh-CN': '请检查系统权限或重启应用',
    en: 'Please check system permissions or restart the app',
  },
  'systemStatus.device': { 'zh-CN': '设备', en: 'Device' },
  'systemStatus.copiedModel': { 'zh-CN': '已复制机型', en: 'Model copied' },
  'systemStatus.copiedHostname': { 'zh-CN': '已复制主机名', en: 'Hostname copied' },
  'systemStatus.copiedLanIp': { 'zh-CN': '已复制内网 IP', en: 'LAN IP copied' },
  'systemStatus.uptime': { 'zh-CN': '运行 {time}', en: 'Uptime {time}' },
  'systemStatus.load': { 'zh-CN': '负载 15m {pct}%', en: 'Load 15m {pct}%' },
  'systemStatus.lowPowerMode': { 'zh-CN': '低电量模式', en: 'Low Power Mode' },

  // ─── 硬件 ──────────────────────────────
  'systemStatus.cpu': { 'zh-CN': '处理器', en: 'CPU' },
  'systemStatus.cpuCores': { 'zh-CN': '{cores} 核 CPU', en: '{cores}-core CPU' },
  'systemStatus.gpuCores': { 'zh-CN': '{n} 核 GPU', en: '{n}-core GPU' },
  'systemStatus.memory': { 'zh-CN': '内存', en: 'Memory' },
  'systemStatus.available': { 'zh-CN': '可用 {size}', en: 'Available {size}' },
  'systemStatus.swap': { 'zh-CN': '交换', en: 'Swap' },
  'systemStatus.disk': { 'zh-CN': '磁盘', en: 'Disk' },
  'systemStatus.external': { 'zh-CN': '外置', en: 'External' },

  // ─── 电池 ──────────────────────────────
  'systemStatus.battery': { 'zh-CN': '电池', en: 'Battery' },
  'systemStatus.health': { 'zh-CN': '健康 {pct}%', en: 'Health {pct}%' },
  'systemStatus.remaining': { 'zh-CN': '剩余 {time}', en: 'Remaining {time}' },
  'systemStatus.fullCharge': { 'zh-CN': '充满 {time}', en: 'Full {time}' },
  'systemStatus.cycles': { 'zh-CN': '{n} 循环', en: '{n} cycles' },
  'systemStatus.noBattery': { 'zh-CN': '无电池（台式机）', en: 'No battery (desktop)' },
  'systemStatus.batteryState.charging': { 'zh-CN': '充电中', en: 'Charging' },
  'systemStatus.batteryState.discharging': { 'zh-CN': '使用中', en: 'Discharging' },
  'systemStatus.batteryState.full': { 'zh-CN': '已充满', en: 'Full' },

  // ─── 进程 / 网络 ───────────────────────
  'systemStatus.processes': { 'zh-CN': '进程', en: 'Processes' },
  'systemStatus.network': { 'zh-CN': '网络', en: 'Network' },
  'systemStatus.lan': { 'zh-CN': '内网', en: 'LAN' },
  'systemStatus.clickToCopy': { 'zh-CN': '点击复制', en: 'Click to copy' },

  // ─── 热状态 ────────────────────────────
  'systemStatus.thermal.fair': { 'zh-CN': '轻微发热', en: 'Slightly warm' },
  'systemStatus.thermal.serious': { 'zh-CN': '热节流', en: 'Thermal throttling' },
  'systemStatus.thermal.critical': { 'zh-CN': '严重过热', en: 'Critically hot' },
  'systemStatus.thermalStateLabel': { 'zh-CN': '系统热状态', en: 'Thermal state' },

  // ─── 错误 ──────────────────────────────
  'systemStatus.readFailed': {
    'zh-CN': '读取系统信息失败：{error}',
    en: 'Failed to read system info: {error}',
  },
})
