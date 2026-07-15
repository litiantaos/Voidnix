import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { AgentEvent } from '@/types/agent'

const mocks = vi.hoisted(() => {
  type Handler = (e: AgentEvent) => void
  const channels: { onmessage: Handler | null }[] = []
  return {
    channels,
    invoke: vi.fn(async () => undefined),
    Channel: class {
      onmessage: Handler | null = null
      constructor() {
        mocks.channels.push(this)
      }
    },
  }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  Channel: mocks.Channel,
}))

vi.mock('@tauri-apps/plugin-store', () => {
  const mem = new Map<string, unknown>()
  return {
    load: () =>
      Promise.resolve({
        get: (k: string) => Promise.resolve(mem.get(k)),
        set: (k: string, v: unknown) => {
          mem.set(k, v)
          return Promise.resolve()
        },
        save: () => Promise.resolve(),
        clear: () => {
          mem.clear()
          return Promise.resolve()
        },
        onChange: () => Promise.resolve(() => {}),
      }),
  }
})

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import { useAgentChat } from './agent'
import { config, setActiveProviderModelKey } from './config'

beforeEach(async () => {
  mocks.channels.length = 0
  mocks.invoke.mockReset()
  mocks.invoke.mockResolvedValue(undefined)

  config.aiProviders.splice(0, config.aiProviders.length, {
    id: 'p1',
    endpoint: 'https://api.example.com/v1',
    apiKey: 'sk-test',
    models: ['m1'],
  })
  setActiveProviderModelKey('p1::m1')

  const agent = useAgentChat()
  await agent.newConversation()
})

describe('useAgentChat session 守卫', () => {
  it('旧 run 的 completed 不踩踏新 run 的 sessionId', async () => {
    const agent = useAgentChat()

    await agent.sendMessage('one')
    expect(mocks.channels).toHaveLength(1)
    const ch1 = mocks.channels[0]!
    expect(agent.isGenerating.value).toBe(true)

    await agent.abort()
    expect(agent.isGenerating.value).toBe(false)

    await agent.sendMessage('two')
    expect(mocks.channels).toHaveLength(2)
    expect(agent.isGenerating.value).toBe(true)

    // 旧 channel 迟到 completed：不得清掉新 session 的 streaming
    ch1.onmessage?.({ type: 'completed' })
    expect(agent.isGenerating.value).toBe(true)

    mocks.channels[1]!.onmessage?.({ type: 'completed' })
    expect(agent.isGenerating.value).toBe(false)
    expect(agent.status.value).toBe('ready')
  })

  it('旧 run 的 error 不改写新 run 的 status', async () => {
    const agent = useAgentChat()

    await agent.sendMessage('one')
    const ch1 = mocks.channels[0]!
    await agent.abort()

    await agent.sendMessage('two')
    expect(agent.isGenerating.value).toBe(true)

    ch1.onmessage?.({ type: 'error', message: 'old fail' })
    expect(agent.isGenerating.value).toBe(true)
    expect(agent.status.value).toBe('streaming')

    // 错误文案写入旧 assistant 气泡（仍在 messages 里）
    const texts = agent.messages.value
      .filter((m) => m.role === 'assistant')
      .flatMap((m) => m.parts)
      .filter((p) => p.type === 'text')
      .map((p) => (p.type === 'text' ? p.text : ''))
    expect(texts.some((t) => t.includes('old fail'))).toBe(true)

    mocks.channels[1]!.onmessage?.({ type: 'completed' })
    expect(agent.status.value).toBe('ready')
  })

  it('本 run completed 清 session 并 ready', async () => {
    const agent = useAgentChat()
    await agent.sendMessage('hi')
    const ch = mocks.channels[0]!
    expect(agent.isGenerating.value).toBe(true)

    ch.onmessage?.({ type: 'textDelta', text: 'ok' })
    ch.onmessage?.({ type: 'completed' })
    expect(agent.isGenerating.value).toBe(false)
    expect(agent.status.value).toBe('ready')
  })

  it('web_search toolResult 成功时解析 parsed', async () => {
    const agent = useAgentChat()
    await agent.sendMessage('search')
    const ch = mocks.channels[0]!

    ch.onmessage?.({ type: 'toolCallStart', id: 'c1', name: 'web_search' })
    ch.onmessage?.({
      type: 'toolCallArgs',
      id: 'c1',
      args: { query: 'void' },
    })
    ch.onmessage?.({
      type: 'toolResult',
      id: 'c1',
      ok: true,
      output: JSON.stringify({
        answer: 'A',
        hits: [{ title: 'T', url: 'https://x', snippet: 's' }],
      }),
    })

    const assistant = agent.messages.value.find((m) => m.role === 'assistant' && m.streaming)
    const part = assistant?.parts.find((p) => p.type === 'toolCall')
    expect(part && part.type === 'toolCall' && part.parsed?.hits[0]?.title).toBe('T')

    ch.onmessage?.({ type: 'completed' })
  })
})
