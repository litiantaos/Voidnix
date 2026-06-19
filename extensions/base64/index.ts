import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/utils/clipboard'
import { encodeBase64, decodeBase64 } from './logic'

export default defineExtension({
  meta: {
    id: 'base64',
    name: 'Base64',
    description: 'Base64 编解码工具',
    icon: 'i-ri-code-s-slash-line',
    order: 100,
    keywords: ['编码', '解码', 'encode', 'decode', 'base64'],
  },

  placeholder: '输入文本进行 Base64 编解码',

  search: {
    dynamic: (query): ProviderResult[] => {
      const trimmed = query.trim()
      if (!trimmed) return []

      const results: ProviderResult[] = []

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
