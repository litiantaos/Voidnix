import { describe, it, expect } from 'vitest'
import {
  buildRoutes,
  routableModels,
  anthropicModels,
  isAliasValid,
  buildCcPayload,
  fallbackAliases,
} from './logic'
import type { AiProvider } from '@/runtime/ai-providers'
import type { AliasSelection } from './config'

function p(partial: Partial<AiProvider> & Pick<AiProvider, 'id'>): AiProvider {
  return {
    name: '',
    chatEndpoint: 'https://api.example.com/v1',
    responsesEndpoint: '',
    anthropicEndpoint: '',
    models: ['m'],
    keys: [{ id: 'k', label: '主号', apiKey: 'sk-1' }],
    governed: false,
    usageKind: '',
    ...partial,
  }
}

describe('buildRoutes', () => {
  it('声明 Anthropic 端点的提供商进路由,URL/模型/Key 全量带入', () => {
    const routes = buildRoutes([
      p({
        id: 'z',
        name: '智谱',
        anthropicEndpoint: 'https://open.bigmodel.cn/api/anthropic',
        models: ['glm-5.3', 'glm-5.3-flash'],
        keys: [
          { id: 'a', label: '195', apiKey: 'sk-a' },
          { id: 'b', label: '', apiKey: 'sk-b' },
        ],
      }),
    ])
    expect(routes).toHaveLength(1)
    expect(routes[0]).toMatchObject({
      providerId: 'z',
      name: '智谱',
      anthropicUrl: 'https://open.bigmodel.cn/api/anthropic',
      chatUrl: 'https://api.example.com/v1',
      models: ['glm-5.3', 'glm-5.3-flash'],
    })
    expect(routes[0].keys).toEqual([
      { label: '195', apiKey: 'sk-a' },
      { label: 'Key', apiKey: 'sk-b' },
    ])
  })

  it('三种端点全空 / 无非空 Key / 无模型的提供商跳过;仅 chat 端点也参与', () => {
    const routes = buildRoutes([
      p({ id: 'a', chatEndpoint: '' }),
      p({ id: 'b', anthropicEndpoint: 'https://x', keys: [{ id: 'k', label: 'x', apiKey: '  ' }] }),
      p({ id: 'c', anthropicEndpoint: 'https://x', models: [' ', ''] }),
      p({ id: 'd', responsesEndpoint: 'https://r' }),
      p({ id: 'e' }),
    ])
    expect(routes.map((r) => r.providerId)).toEqual(['d', 'e'])
  })

  it('已治理上游标记收敛透传（缺省/true）', () => {
    const routes = buildRoutes([
      p({ id: 'plain', anthropicEndpoint: 'https://x' }),
      p({ id: 'gov', anthropicEndpoint: 'https://y', governed: true }),
    ])
    expect(routes.map((r) => [r.providerId, r.governed])).toEqual([
      ['plain', false],
      ['gov', true],
    ])
  })
})

describe('routableModels / anthropicModels', () => {
  it('按协议标注可达性,CC 侧仅取 Anthropic 集', () => {
    const routes = buildRoutes([
      p({
        id: 'z',
        name: '智谱',
        anthropicEndpoint: 'https://z/api/anthropic',
        responsesEndpoint: 'https://z/responses',
        models: ['glm-5.3', 'glm-5.3-flash'],
      }),
      p({ id: 'd', name: 'DeepSeek', responsesEndpoint: 'https://d/responses', models: ['r1'] }),
    ])
    const models = routableModels(routes)
    expect(models).toHaveLength(3)
    expect(models[0]).toMatchObject({
      model: 'glm-5.3',
      anthropic: true,
      responses: true,
      chat: true,
    })
    expect(models[2]).toMatchObject({ model: 'r1', anthropic: false, responses: true, chat: true })
    expect(anthropicModels(models).map((m) => m.model)).toEqual(['glm-5.3', 'glm-5.3-flash'])
  })
})

describe('isAliasValid', () => {
  const models = anthropicModels(
    routableModels(
      buildRoutes([
        p({ id: 'z', name: '智谱', anthropicEndpoint: 'https://z', models: ['glm-5.3'] }),
      ]),
    ),
  )

  it('提供商与模型均在 Anthropic 路由内才有效', () => {
    expect(isAliasValid(models, { providerId: 'z', model: 'glm-5.3' })).toBe(true)
    expect(isAliasValid(models, { providerId: 'z', model: 'gone' })).toBe(false)
    expect(isAliasValid(models, { providerId: 'x', model: 'glm-5.3' })).toBe(false)
    expect(isAliasValid(models, null)).toBe(false)
  })
})

describe('buildCcPayload', () => {
  const routes = buildRoutes([
    p({
      id: 'z',
      name: '智谱',
      anthropicEndpoint: 'https://z/api/anthropic',
      models: ['glm-5.3', 'glm-5.3-flash'],
    }),
    p({ id: 'd', name: 'DeepSeek', responsesEndpoint: 'https://d/responses', models: ['r1'] }),
  ])

  it('别名有效时写入模型,失效时该键留空(不写 CC env);picker 仅 Anthropic 模型', () => {
    const payload = buildCcPayload(
      routes,
      {
        sonnet: { providerId: 'z', model: 'glm-5.3' },
        opus: null,
        haiku: { providerId: 'd', model: 'r1' },
      },
      8788,
      'default',
    )
    expect(payload).not.toBeNull()
    expect(payload!.sonnetModel).toBe('glm-5.3')
    expect(payload!.haikuModel).toBe('')
    expect(payload!.pickerRows.map((r) => r.model)).toEqual(['glm-5.3', 'glm-5.3-flash'])
    expect(payload!.port).toBe(8788)
  })

  it("context = '1m' = 别名与 picker 统一追加 [1m](已带后缀防双写),label 恒裸名", () => {
    const withSuffix = buildRoutes([
      p({
        id: 'z',
        name: '智谱',
        anthropicEndpoint: 'https://z/api/anthropic',
        models: ['glm-5.3', 'glm-5.3-flash[1m]'],
      }),
    ])
    const payload = buildCcPayload(
      withSuffix,
      {
        sonnet: { providerId: 'z', model: 'glm-5.3' },
        opus: null,
        haiku: { providerId: 'z', model: 'glm-5.3-flash[1m]' },
      },
      8788,
      '1m',
    )
    expect(payload!.sonnetModel).toBe('glm-5.3[1m]')
    expect(payload!.haikuModel).toBe('glm-5.3-flash[1m]')
    expect(payload!.pickerRows).toEqual([
      { model: 'glm-5.3[1m]', label: 'glm-5.3' },
      { model: 'glm-5.3-flash[1m]', label: 'glm-5.3-flash' },
    ])
  })

  it("context = 'default' = 中枢带后缀存储也统一剥成裸名(档位是 CC 侧形态唯一决定因素)", () => {
    const withSuffix = buildRoutes([
      p({
        id: 'z',
        name: '智谱',
        anthropicEndpoint: 'https://z/api/anthropic',
        models: ['glm-5.3[1m]'],
      }),
    ])
    const payload = buildCcPayload(
      withSuffix,
      { sonnet: { providerId: 'z', model: 'glm-5.3[1m]' }, opus: null, haiku: null },
      8788,
      'default',
    )
    expect(payload!.sonnetModel).toBe('glm-5.3')
    expect(payload!.pickerRows).toEqual([{ model: 'glm-5.3', label: 'glm-5.3' }])
  })

  it('无 Anthropic 可路由模型时返回 null(不接线)', () => {
    const onlyResponses = buildRoutes([p({ id: 'd', responsesEndpoint: 'https://d/responses' })])
    expect(
      buildCcPayload(onlyResponses, { sonnet: null, opus: null, haiku: null }, 8788, '1m'),
    ).toBeNull()
  })
})

describe('fallbackAliases', () => {
  it('三档全空兜底首个 Anthropic 模型;无可用模型返回全空', () => {
    const routes = buildRoutes([
      p({ id: 'z', name: '智谱', anthropicEndpoint: 'https://z', models: ['glm-5.3'] }),
    ])
    const fb = fallbackAliases(routableModels(routes))
    const expectSel: AliasSelection = { providerId: 'z', model: 'glm-5.3' }
    expect(fb).toEqual({ sonnet: expectSel, opus: expectSel, haiku: expectSel })

    expect(fallbackAliases([])).toEqual({ sonnet: null, opus: null, haiku: null })
  })
})
