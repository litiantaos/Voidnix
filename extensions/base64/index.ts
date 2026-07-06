import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { encodeBase64, decodeBase64 } from './logic'

export default defineExtension({
  meta: {
    id: 'base64',
    name: 'Base64',
    description: 'Base64 编解码工具',
    icon: 'i-ri-code-s-slash-line',
    order: 80,
    keywords: ['编码', '解码', 'encode', 'decode', 'base64'],
  },

  placeholder: '输入文本编解码 Base64',

  search: {
    dynamic: (query, ctx): ProviderResult[] => {
      const trimmed = query.trim()
      if (!trimmed) return []

      const results: ProviderResult[] = []
      const moduleMode = !!ctx?.moduleMode

      // 编码：任意文本均可，全局模式过宽（"100 usd" 也会编码）→ 仅模块内提供
      if (moduleMode) {
        const encoded = encodeBase64(trimmed)
        if (encoded) {
          results.push({
            id: 'base64-encode',
            title: encoded,
            description: 'Base64 编码',
            icon: 'i-ri-code-s-slash-line',
            boost: 1000,
            data: { kind: 'module', value: encoded },
          })
        }
      }

      // 解码：合法 base64 格式才返回（格式特征明确，全局也安全）
      if (/^[A-Za-z0-9+/=]+$/.test(trimmed) && trimmed.length % 4 === 0) {
        const decoded = decodeBase64(trimmed)
        if (decoded) {
          results.push({
            id: 'base64-decode',
            title: decoded,
            description: 'Base64 解码',
            icon: 'i-ri-text',
            boost: 999,
            data: { kind: 'module', value: decoded },
          })
        }
      }

      return results
    },
  },

  onExecute: async (result) => {
    if (result.data?.value) {
      copyAndHide(result.data.value as string)
    }
  },
})
