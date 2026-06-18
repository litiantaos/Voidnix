import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { copyAndHide } from '@/utils/clipboard'

function encodeBase64(str: string): string {
  try {
    return btoa(
      encodeURIComponent(str).replace(/%([0-9A-F]{2})/g, (_, p1) =>
        String.fromCharCode(Number('0x' + p1)),
      ),
    )
  } catch {
    return ''
  }
}

function decodeBase64(str: string): string {
  try {
    return decodeURIComponent(
      atob(str)
        .split('')
        .map((c) => '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2))
        .join(''),
    )
  } catch {
    return ''
  }
}

const module: AppModule = {
  id: 'base64',
  name: 'Base64',
  description: 'Base64 编解码工具',
  icon: 'i-ri-code-s-slash-line',
  order: 100,
  keywords: ['编码', '解码', 'encode', 'decode', 'base64'],

  placeholder: '输入文本进行 Base64 编解码',

  onModuleSearch: async (query: string): Promise<SearchResult[]> => {
    const trimmed = query.trim()
    if (!trimmed) return []

    const results: SearchResult[] = []

    const encoded = encodeBase64(trimmed)
    if (encoded) {
      results.push({
        id: 'base64-encode',
        title: encoded,
        module: 'base64',
        description: 'Base64 编码',
        icon: 'i-ri-code-s-slash-line',
        score: 1000,
        data: { kind: 'result', value: encoded },
      })
    }

    if (/^[A-Za-z0-9+/=]+$/.test(trimmed) && trimmed.length % 4 === 0) {
      const decoded = decodeBase64(trimmed)
      if (decoded) {
        results.push({
          id: 'base64-decode',
          title: decoded,
          module: 'base64',
          description: 'Base64 解码',
          icon: 'i-ri-text',
          score: 999,
          data: { kind: 'result', value: decoded },
        })
      }
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
