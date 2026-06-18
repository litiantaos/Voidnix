import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { copyAndHide } from '@/utils/clipboard'

function uuidv4(): string {
  return crypto.randomUUID()
}

function nanoId(size = 21): string {
  const chars = 'Useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict'
  const bytes = crypto.getRandomValues(new Uint8Array(size))
  let id = ''
  for (let i = 0; i < size; i++) {
    id += chars[bytes[i] % chars.length]
  }
  return id
}

const module: AppModule = {
  id: 'uuid',
  name: 'UUID',
  description: 'UUID / NanoID 生成器',
  icon: 'i-ri-fingerprint-line',
  order: 110,
  keywords: ['uuid', 'guid', 'nanoid', '唯一', '标识'],

  placeholder: '按 Enter 生成 UUID / NanoID（输入数字指定 NanoID 长度）',

  onModuleSearch: async (query: string): Promise<SearchResult[]> => {
    const trimmed = query.trim()
    const results: SearchResult[] = []

    if (!trimmed || /^\d+$/.test(trimmed)) {
      const size = trimmed ? parseInt(trimmed, 10) : 21
      results.push({
        id: 'uuid-v4',
        title: uuidv4(),
        module: 'uuid',
        description: 'UUID v4',
        icon: 'i-ri-fingerprint-line',
        score: 1000,
        data: { kind: 'result', value: uuidv4() },
      })
      results.push({
        id: 'nanoid',
        title: nanoId(size),
        module: 'uuid',
        description: `NanoID（长度 ${size}）`,
        icon: 'i-ri-shield-keyhole-line',
        score: 999,
        data: { kind: 'result', value: nanoId(size) },
      })
    }

    return results
  },

  async onExecute(result: SearchResult) {
    if (result.data?.value) {
      copyAndHide(result.data.value as string)
    }
  },
}

registerModule(module)
