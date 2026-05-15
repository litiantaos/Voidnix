import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { commands } from '@/bindings'

const mod: AppModule = {
  id: 'ip',
  name: 'IP 信息',
  description: '查询本机或指定 IP 信息',
  icon: 'i-ri-global-line',
  keywords: ['ip', 'network', '网络', '地址'],
  placeholder: '输入指定 IP 地址，留空则查询本机',
  order: 5,
  onSearch: async (query) => {
    if ('ip'.includes(query.toLowerCase()) || '网络'.includes(query)) {
      return [{
        id: 'ip-module',
        title: 'IP 查询',
        description: '查看本机或指定 IP 信息',
        module: 'ip',
        icon: 'i-ri-global-line',
        score: 100,
        data: { kind: 'module', moduleId: 'ip' }
      }]
    }
    return []
  },
  onModuleSearch: async (query) => {
    const trimmed = query.trim()
    try {
      const data = await commands.fetchIpInfo(trimmed || null)

      if (data.success && data.ip) {
        const location = [data.country, data.region, data.city].filter(Boolean).join(' ')
        return [
          { id: 'ip-addr', title: data.ip, description: 'IP 地址', module: 'ip', icon: 'i-ri-global-line', data: { isHighlight: true } },
          { id: 'ip-loc', title: location, description: '地理位置', module: 'ip', icon: 'i-ri-map-pin-line' },
          { id: 'ip-isp', title: data.isp || '', description: '运营商 (ISP)', module: 'ip', icon: 'i-ri-router-line' },
          { id: 'ip-org', title: data.org || '', description: '组织 (Org)', module: 'ip', icon: 'i-ri-building-line' }
        ].filter(i => i.title)
      } else {
        return [{ id: 'ip-err', title: '查询失败', description: data.message || '未知错误', module: 'ip', icon: 'i-ri-error-warning-line' }]
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      return [{ id: 'ip-err', title: '网络请求失败', description: msg, module: 'ip', icon: 'i-ri-error-warning-line' }]
    }
  },
  onExecute: async (result) => {
    if (result.id === 'ip-err') return
    try {
      await writeText(result.title)
      getCurrentWindow().hide()
    } catch (e) {
      console.error('Failed to copy IP info:', e)
    }
  }
}

registerModule(mod)
