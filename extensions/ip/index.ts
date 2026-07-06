import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isValidIpLike } from './logic'

interface IpInfo {
  ip?: string
  success?: boolean
  message?: string
  country?: string
  region?: string
  city?: string
  isp?: string
  org?: string
  asn?: string
}

// 短期 memoize：IP 信息秒级稳定，输入过程（如 1.1.1.1→8.8.8.8）每键不重复请求；
// 失败结果不缓存，保留重试友好性
const IP_CACHE_TTL = 60_000
let ipCache: { query: string | null; data: IpInfo; ts: number } | null = null

async function fetchIpInfo(ip: string | null): Promise<IpInfo> {
  if (ipCache && ipCache.query === ip && Date.now() - ipCache.ts < IP_CACHE_TTL) {
    return ipCache.data
  }
  // 走框架 Rust http_get：绕过 webview 的 UA/Referer 反爬（ipwhois.app 对 WebKit+localhost 返回 403）与 CORS
  const url = ip
    ? `https://ipwhois.app/json/${ip}?lang=zh-CN`
    : 'https://ipwhois.app/json/?lang=zh-CN'
  const text = await invoke<string>(CMD.httpGet, { url })
  const data = JSON.parse(text) as IpInfo
  if (data.success !== false) {
    ipCache = { query: ip, data, ts: Date.now() }
  }
  return data
}

export default defineExtension({
  meta: {
    id: 'ip',
    name: 'IP 信息',
    description: '查询 IP 地址信息',
    icon: 'i-ri-global-line',
    keywords: ['ip', 'network', '网络', '地址'],
    order: 60,
  },

  placeholder: '输入 IP 地址，留空查询本机',

  search: {
    dynamic: async (query, ctx): Promise<ProviderResult[]> => {
      const trimmed = query.trim()
      // 全局默认列表（moduleMode=false）空 query 不触发网络请求（避免拖慢主列表）；
      // 模块内空 query 正常查询本机 IP
      if (!trimmed && !ctx?.moduleMode) return []
      if (trimmed && !isValidIpLike(trimmed)) return []
      try {
        const data = await fetchIpInfo(trimmed || null)

        if (data.success && data.ip) {
          const location = [data.country, data.region, data.city].filter(Boolean).join(' ')
          const list: ProviderResult[] = [
            {
              id: 'ip-addr',
              title: data.ip,
              description: 'IP 地址',
              icon: 'i-ri-global-line',
              data: { kind: 'module', isHighlight: true },
            },
            {
              id: 'ip-loc',
              title: location,
              description: '地理位置',
              icon: 'i-ri-map-pin-line',
              data: { kind: 'module' },
            },
            {
              id: 'ip-isp',
              title: data.isp || '',
              description: '运营商 (ISP)',
              icon: 'i-ri-router-line',
              data: { kind: 'module' },
            },
            {
              id: 'ip-org',
              title: data.org || '',
              description: '组织 (Org)',
              icon: 'i-ri-building-line',
              data: { kind: 'module' },
            },
          ]
          return list.filter((i) => i.title)
        }
        return [
          {
            id: 'ip-err',
            title: '查询失败',
            description: data.message || '未知错误',
            icon: 'i-ri-error-warning-line',
            data: { kind: 'module' },
          },
        ]
      } catch (e: unknown) {
        return [
          {
            id: 'ip-err',
            title: '网络请求失败',
            description: e instanceof Error ? e.message : String(e),
            icon: 'i-ri-error-warning-line',
            data: { kind: 'module' },
          },
        ]
      }
    },
  },

  onExecute: async (result) => {
    if (result.id === 'ip-err') return
    try {
      await copyAndHide(result.title)
    } catch (e) {
      console.error('Failed to copy IP info:', e)
    }
  },
})
