import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'

const encodeBase64 = (str: string): string => {
  try {
    return btoa(
      encodeURIComponent(str).replace(
        /%([0-9A-F]{2})/g,
        function toSolidBytes(_match, p1) {
          return String.fromCharCode(Number('0x' + p1))
        },
      ),
    )
  } catch {
    return ''
  }
}

const decodeBase64 = (str: string): string => {
  try {
    return decodeURIComponent(
      atob(str)
        .split('')
        .map(function (c) {
          return '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2)
        })
        .join(''),
    )
  } catch {
    return ''
  }
}

const mod: AppModule = {
  id: 'base64',
  name: 'Base64 编解码',
  description: '将文本进行 Base64 编码或解码',
  icon: 'i-ri-code-box-line',
  keywords: ['base64', 'encode', 'decode', '编码', '解码'],
  placeholder: '输入文本',
  order: 6,
  onSearch: async (query) => {
    if (
      'base64'.includes(query.toLowerCase()) ||
      '编码'.includes(query) ||
      '解码'.includes(query)
    ) {
      return [
        {
          id: 'base64-module',
          title: 'Base64 编解码',
          description: '打开 Base64 编解码扩展',
          module: 'base64',
          icon: 'i-ri-code-box-line',
          score: 100,
          data: { kind: 'module', moduleId: 'base64' },
        },
      ]
    }
    return []
  },
  onModuleSearch: async (query) => {
    const trimmed = query.trim()
    if (!trimmed) return []

    const results: SearchResult[] = []

    const encoded = encodeBase64(trimmed)
    if (encoded) {
      results.push({
        id: 'encoded',
        title: encoded,
        description: 'Base64 编码结果',
        module: 'base64',
        icon: 'i-ri-code-s-slash-line',
        data: { isHighlight: true },
      })
    }

    if (/^[A-Za-z0-9+/=]+$/.test(trimmed) && trimmed.length % 4 === 0) {
      const decoded = decodeBase64(trimmed)
      if (decoded) {
        results.push({
          id: 'decoded',
          title: decoded,
          description: 'Base64 解码结果',
          module: 'base64',
          icon: 'i-ri-text',
          data: { isHighlight: true },
        })
      }
    }

    return results
  },
  onExecute: async (result) => {
    try {
      await writeText(result.title)
      getCurrentWindow().hide()
    } catch (e) {
      console.error('Failed to copy base64:', e)
    }
  },
}

registerModule(mod)