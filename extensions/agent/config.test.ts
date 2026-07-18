import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: () => Promise.resolve(undefined),
      set: () => Promise.resolve(),
      save: () => Promise.resolve(),
      clear: () => Promise.resolve(),
      delete: () => Promise.resolve(true),
      onChange: () => Promise.resolve(() => {}),
    }),
}))

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import { config, updateSearchProvider } from './config'

beforeEach(() => {
  config.searchProvider.apiKey = ''
  config.systemPrompt = 'test'
  config.providerModelKey = ''
})

describe('updateSearchProvider', () => {
  it('更新 tavily key', () => {
    updateSearchProvider({ apiKey: 'tvly-x' })
    expect(config.searchProvider.apiKey).toBe('tvly-x')
  })
})
