import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { encodeBase64, tryDecodeBase64 } from './logic'

/** 全局即时答案 boost（>KEYWORD_EXTENSION_BOOST=500，穿透 groupAndSort 零分过滤） */
const DECODE_BOOST = 1000

/** 全局模式 base64 识别最低长度（避免普通短词误触） */
const GLOBAL_MIN_LENGTH = 8

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

      // 解码优先（扩展内 + 全局均提供）
      const minLen = ctx?.extensionMode ? 0 : GLOBAL_MIN_LENGTH
      const decoded = tryDecodeBase64(trimmed, minLen)
      if (decoded) {
        results.push({
          id: 'base64-decode',
          title: decoded,
          description: 'Base64 解码',
          icon: 'i-ri-text',
          boost: DECODE_BOOST,
          data: { kind: 'extension', value: decoded, isHighlight: true },
        })
      }

      // 编码仅扩展内提供（全局输入普通文本不应误触编码）
      if (ctx?.extensionMode) {
        const encoded = encodeBase64(trimmed)
        if (encoded) {
          results.push({
            id: 'base64-encode',
            title: encoded,
            description: 'Base64 编码',
            icon: 'i-ri-code-s-slash-line',
            boost: 999,
            data: { kind: 'extension', value: encoded },
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
