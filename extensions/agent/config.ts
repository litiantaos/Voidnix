import { computed } from 'vue'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

export interface SearchProviderConfig {
  id: string
  type: 'tavily'
  apiKey: string
}

/// agent 扩展自管配置（持久化至 extensions/agent/config.json）。
export const config = defineConfig('agent', {
  trustedCommands: [
    'ls',
    'cat',
    'pwd',
    'echo',
    'head',
    'tail',
    'wc',
    'file',
    'stat',
    'date',
    'which',
    'whoami',
    'uname',
    'find',
    'grep',
    'rg',
    'fd',
    'ag',
    'tree',
    'diff',
    'comm',
    'cmp',
    'md5sum',
    'shasum',
    'mkdir',
    'touch',
    'cp',
    'mv',
    'ln',
    'tee',
    'truncate',
    'sed',
    'awk',
    'sort',
    'uniq',
    'cut',
    'tr',
    'paste',
    'expand',
    'jq',
    'yq',
    'bat',
    'git',
  ],
  systemPrompt: '',
  searchProviders: [
    { id: generateRequestId(), type: 'tavily', apiKey: '' },
  ] as SearchProviderConfig[],
  activeSearchProviderId: '',
})

/// 当前激活的搜索提供商（回退到第一个）。
export const activeSearchProvider = computed(
  () =>
    config.searchProviders.find((p) => p.id === config.activeSearchProviderId) ||
    config.searchProviders[0],
)

export function addSearchProvider(): string {
  const id = generateRequestId()
  config.searchProviders.push({ id, type: 'tavily', apiKey: '' })
  config.activeSearchProviderId = id
  return id
}

export function removeSearchProvider(id: string) {
  const idx = config.searchProviders.findIndex((c) => c.id === id)
  if (idx === -1 || config.searchProviders.length <= 1) return
  config.searchProviders.splice(idx, 1)
  if (config.activeSearchProviderId === id) {
    config.activeSearchProviderId = config.searchProviders[0]?.id || ''
  }
}

export function updateSearchProvider(id: string, partial: Partial<SearchProviderConfig>) {
  const p = config.searchProviders.find((c) => c.id === id)
  if (p) Object.assign(p, partial)
}
