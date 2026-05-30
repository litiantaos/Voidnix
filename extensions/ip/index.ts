import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { moduleSelfResult } from '@/core/module-helpers'
import { copyAndHide } from '@/utils/clipboard'
import { toErrorMessage } from '@/utils/error'
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
    if (!query.trim()) return []
    if ('ip'.includes(query.toLowerCase()) || '网络'.includes(query)) {
      return [moduleSelfResult(mod)]
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
      const msg = toErrorMessage(e)
      return [{ id: 'ip-err', title: '网络请求失败', description: msg, module: 'ip', icon: 'i-ri-error-warning-line' }]
    }
  },
  onExecute: async (result) => {
    if (result.id === 'ip-err') return
    try {
      await copyAndHide(result.title)
    } catch (e) {
      console.error('Failed to copy IP info:', e)
    }
  }
}

registerModule(mod)