import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/utils/clipboard'
import { uuidv4, nanoId } from './logic'

export default defineExtension({
  meta: {
    id: 'uuid',
    name: 'UUID',
    description: 'UUID / NanoID 生成器',
    icon: 'i-ri-fingerprint-line',
    order: 110,
    keywords: ['uuid', 'guid', 'nanoid', '唯一', '标识'],
  },

  placeholder: '按 Enter 生成 UUID / NanoID（输入数字指定 NanoID 长度）',

  search: {
    dynamic: (query): ProviderResult[] => {
      const trimmed = query.trim()
      const results: ProviderResult[] = []

      if (!trimmed || /^\d+$/.test(trimmed)) {
        const size = trimmed ? parseInt(trimmed, 10) : 21
        results.push({
          id: 'uuid-v4',
          title: uuidv4(),
          description: 'UUID v4',
          icon: 'i-ri-fingerprint-line',
          data: { kind: 'module', value: uuidv4() },
        })
        results.push({
          id: 'nanoid',
          title: nanoId(size),
          description: `NanoID（长度 ${size}）`,
          icon: 'i-ri-shield-keyhole-line',
          data: { kind: 'module', value: nanoId(size) },
        })
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
