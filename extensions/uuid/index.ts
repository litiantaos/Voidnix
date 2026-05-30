import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { moduleSelfResult } from '@/core/module-helpers'
import { copyAndHide } from '@/utils/clipboard'

const generateNanoId = (size = 21) => {
  const urlAlphabet = 'useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict'
  let id = ''
  let i = size
  while (i > 0) {
    i -= 1
    id += urlAlphabet[(Math.random() * 64) | 0]
  }
  return id
}

const mod: AppModule = {
  id: 'uuid',
  name: 'UUID 生成',
  description: '生成 UUID v4',
  icon: 'i-ri-fingerprint-line',
  keywords: ['uuid', 'guid', '生成', 'generate'],
  placeholder: '输入数字批量生成，例如: 10',
  order: 7,
  onSearch: async (query) => {
    if (!query.trim()) return []
    if ('uuid'.includes(query.toLowerCase()) || 'guid'.includes(query.toLowerCase())) {
      return [moduleSelfResult(mod)]
    }
    return []
  },
  onModuleSearch: async (query) => {
    const count = parseInt(query)
    const items: SearchResult[] = []

    if (isNaN(count) || count <= 1) {
      const uuid = crypto.randomUUID()
      const nano = generateNanoId()
      items.push(
        { id: 'v4-standard', title: uuid, description: 'UUID v4 (标准)', module: 'uuid' },
        {
          id: 'v4-nodash',
          title: uuid.replace(/-/g, ''),
          description: 'UUID v4 (无短横线)',
          module: 'uuid',
        },
        {
          id: 'v4-upper',
          title: uuid.toUpperCase(),
          description: 'UUID v4 (大写)',
          module: 'uuid',
        },
        {
          id: 'nanoid',
          title: nano,
          description: 'NanoID',
          module: 'uuid',
          icon: 'i-ri-key-2-line',
        },
      )
    } else {
      const total = Math.min(count, 100)
      for (let i = 0; i < total; i++) {
        const uuid = crypto.randomUUID()
        items.push({
          id: `multi-${i}`,
          title: uuid,
          description: `UUID v4 (${i + 1})`,
          module: 'uuid',
        })
      }
    }
    return items
  },
  onExecute: async (result) => {
    try {
      await copyAndHide(result.title)
    } catch (e) {
      console.error('Failed to copy UUID:', e)
    }
  },
}

registerModule(mod)
