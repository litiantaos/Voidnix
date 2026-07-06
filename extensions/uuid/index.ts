import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { uuidv4, nanoId } from './logic'

export default defineExtension({
  meta: {
    id: 'uuid',
    name: 'UUID',
    description: 'UUID / NanoID 生成',
    icon: 'i-ri-fingerprint-line',
    order: 70,
    keywords: ['uuid', 'guid', 'nanoid', '唯一', '标识'],
  },

  disableSearchInput: true,

  search: {
    dynamic: (): ProviderResult[] => {
      const results: ProviderResult[] = []
      const uuid = uuidv4()
      const nano = nanoId(21)
      results.push({
        id: 'uuid-v4',
        title: uuid,
        description: 'UUID v4',
        icon: 'i-ri-fingerprint-line',
        data: { kind: 'module', value: uuid },
      })
      results.push({
        id: 'nanoid',
        title: nano,
        description: 'NanoID',
        icon: 'i-ri-shield-keyhole-line',
        data: { kind: 'module', value: nano },
      })
      return results
    },
  },

  onExecute: async (result) => {
    if (result.data?.value) {
      copyAndHide(result.data.value as string)
    }
  },
})
