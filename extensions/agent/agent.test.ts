import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { AgentEvent } from '@/types/agent'
import './locales'

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
import { config as agentConfig, setProviderModelKey } from './config'
import { config as aiProvidersConfig } from '@/runtime/ai-providers'

beforeEach(async () => {
  mocks.channels.length = 0
  mocks.invoke.mockReset()
  mocks.invoke.mockResolvedValue(undefined)

  aiProvidersConfig.providers.splice(0, aiProvidersConfig.providers.length, {
    id: 'p1',
    name: '',
    endpoint: 'https://api.example.com/v1',
    models: ['m1'],
    keys: [{ id: 'k1', label: '默认', apiKey: 'sk-test' }],
    usageKind: '',
    envKey: '',
    responsesEndpoint: '',
  })
  agentConfig.providerModelKey = ''
  setProviderModelKey('p1::k1::m1')

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

  it('旧 run 的 error 不改写新 run 的 status，也不改写已收尾气泡', async () => {
    const agent = useAgentChat()

    await agent.sendMessage('one')
    const ch1 = mocks.channels[0]!
    await agent.abort()

    await agent.sendMessage('two')
    expect(agent.isGenerating.value).toBe(true)

    ch1.onmessage?.({ type: 'error', message: 'old fail' })
    expect(agent.isGenerating.value).toBe(true)
    expect(agent.status.value).toBe('streaming')

    // 旧气泡已 abort finalize：晚到 error 不得再写 notice
    const firstAssistant = agent.messages.value.find((m) => m.role === 'assistant')
    expect(firstAssistant?.parts.some((p) => p.type === 'notice' && p.kind === 'aborted')).toBe(
      true,
    )
    expect(
      firstAssistant?.parts.some(
        (p) => p.type === 'notice' && p.kind === 'error' && p.text.includes('old fail'),
      ),
    ).toBe(false)

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

  it('reasoningDelta 累积成 reasoning part（不回灌 LLM）', async () => {
    const agent = useAgentChat()
    await agent.sendMessage('hi')
    const ch = mocks.channels[0]!

    ch.onmessage?.({ type: 'reasoningDelta', text: '分析' })
    ch.onmessage?.({ type: 'reasoningDelta', text: '问题' })
    ch.onmessage?.({ type: 'textDelta', text: '回答' })
    ch.onmessage?.({ type: 'completed' })

    const assistant = agent.messages.value.find((m) => m.role === 'assistant')
    const reasoning = assistant?.parts.find((p) => p.type === 'reasoning')
    expect(reasoning && reasoning.type === 'reasoning' && reasoning.text).toBe('分析问题')
    // reasoning 不进 LLM 上下文（toLlmMessages 只取 text）
    const { toLlmMessages } = await import('./logic')
    const llm = toLlmMessages(agent.messages.value)
    expect(llm.some((m) => m.content?.includes('分析问题'))).toBe(false)
    expect(llm.some((m) => m.content === '回答')).toBe(true)
  })

  it('web_search toolResult 成功时解析 answer 摘要', async () => {
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
    expect(part && part.type === 'toolCall' && part.parsed).toBe('A')

    ch.onmessage?.({ type: 'completed' })
  })

  it('abort 写入 aborted notice 并结束 streaming', async () => {
    const agent = useAgentChat()
    await agent.sendMessage('hi')
    const ch = mocks.channels[0]!
    ch.onmessage?.({ type: 'textDelta', text: 'partial' })

    await agent.abort()
    expect(agent.isGenerating.value).toBe(false)

    const assistant = agent.messages.value.find((m) => m.role === 'assistant')
    expect(assistant?.streaming).toBeFalsy()
    const notice = assistant?.parts.find((p) => p.type === 'notice')
    expect(notice && notice.type === 'notice' && notice.kind).toBe('aborted')
    expect(notice && notice.type === 'notice' && notice.text).toBe('已中止')
  })

  it('error 写入 error notice', async () => {
    const agent = useAgentChat()
    await agent.sendMessage('hi')
    const ch = mocks.channels[0]!
    ch.onmessage?.({ type: 'error', message: 'boom' })

    expect(agent.status.value).toBe('error')
    const assistant = agent.messages.value.find((m) => m.role === 'assistant')
    const notice = assistant?.parts.find((p) => p.type === 'notice')
    expect(notice && notice.type === 'notice' && notice.kind).toBe('error')
    expect(notice && notice.type === 'notice' && notice.text).toBe('boom')
  })

  it('abort 后晚到 textDelta / tool 不写入气泡', async () => {
    const agent = useAgentChat()
    await agent.sendMessage('hi')
    const ch = mocks.channels[0]!
    ch.onmessage?.({ type: 'textDelta', text: 'partial' })
    await agent.abort()

    ch.onmessage?.({ type: 'textDelta', text: ' after-abort' })
    ch.onmessage?.({ type: 'toolCallStart', id: 'late', name: 'run_command' })
    ch.onmessage?.({ type: 'error', message: 'late error' })

    const assistant = agent.messages.value.find((m) => m.role === 'assistant')
    expect(assistant?.parts).toHaveLength(2) // text + aborted notice
    expect(assistant?.parts[0]).toEqual({ type: 'text', text: 'partial' })
    expect(assistant?.parts[1]).toMatchObject({ type: 'notice', kind: 'aborted' })
    // 晚到 error 不得再加 notice；status 已 ready（非本 run）
    expect(agent.status.value).toBe('ready')
  })

  it('体积超限时截断旧 toolCall output 和 reasoning（保留最新一轮）', async () => {
    const agent = useAgentChat()
    await agent.newConversation()

    // 手动构造超体积历史：每项 300K，total ~900K 远超 400K 上限
    const big = 'x'.repeat(300_000)
    agent.messages.value.push(
      {
        id: 'm1',
        role: 'assistant',
        parts: [
          { type: 'reasoning', text: big },
          {
            type: 'toolCall',
            id: 't1',
            name: 'run_command',
            state: 'done',
            output: big,
          },
          { type: 'text', text: 'done1' },
        ],
      },
      {
        id: 'm2',
        role: 'assistant',
        parts: [
          {
            type: 'toolCall',
            id: 't2',
            name: 'web_search',
            state: 'done',
            output: big,
          },
          { type: 'text', text: 'done2' },
        ],
      },
    )

    // 触发 trimHistory（sendMessage 内部调用）
    await agent.sendMessage('next')
    await agent.abort()

    const m1 = agent.messages.value.find((m) => m.id === 'm1')
    // 旧 reasoning 被截断（最先处理，total 从 ~900K 降到 ~600K 仍超限）
    const r1 = m1?.parts.find((p) => p.type === 'reasoning')
    expect(r1 && r1.type === 'reasoning' && r1.text.includes('已截断')).toBe(true)
    expect(r1 && r1.type === 'reasoning' && r1.text.length < 1700).toBe(true)
    // 旧 toolCall output 也被截断（total 降到 ~300K 才停）
    const t1 = m1?.parts.find((p) => p.type === 'toolCall' && p.id === 't1')
    expect(t1 && t1.type === 'toolCall' && t1.output?.includes('已截断')).toBe(true)
    expect(t1 && t1.type === 'toolCall' && (t1.output?.length ?? 0) < 1700).toBe(true)
  })
})
