import { describe, it, expect } from 'vitest'
import {
  shellSingleQuote,
  baseUrlEnvName,
  buildExportPayload,
  resolveEnvKey,
  isZhipuCodingEndpoint,
  anthropicModelFromZhipu,
  ZHIPU_ANTHROPIC_BASE_URL,
  maskKey,
  formatWindowRemain,
  formatCompactCount,
  formatKeyUsageSubtitle,
  formatDeepseekBalanceSubtitle,
  normalizeZhipuMonitor,
  normalizeDeepseekBalance,
  envLabelTag,
  assignKeyEnvNames,
} from './logic'
import type { AiProvider } from '@/runtime/ai-providers'

function p(
  id: string,
  endpoint: string,
  keys: { id: string; label: string; apiKey: string }[],
  models: string[] = ['m'],
): AiProvider {
  return {
    id,
    name: '',
    endpoint,
    models,
    keys,
    usageKind: '',
    envKey: '',
  }
}

describe('resolveEnvKey', () => {
  it('知名端点锁死工具约定名', () => {
    expect(resolveEnvKey(p('z', 'https://open.bigmodel.cn/api/coding/paas/v4', []))).toBe(
      'ZHIPU_API_KEY',
    )
    expect(resolveEnvKey(p('d', 'https://api.deepseek.com', []))).toBe('DEEPSEEK_API_KEY')
    expect(isZhipuCodingEndpoint('https://open.bigmodel.cn/x')).toBe(true)
  })

  it('envKey 显式优先', () => {
    const prov = p('z', 'https://open.bigmodel.cn/api/coding/paas/v4', [])
    prov.envKey = 'CUSTOM_API_KEY'
    expect(resolveEnvKey(prov)).toBe('CUSTOM_API_KEY')
  })
})

describe('buildExportPayload', () => {
  it('第一套完整配置导出 OPENAI_*', () => {
    const { envText } = buildExportPayload({
      providers: [
        p(
          'a',
          'https://api.openai.com/v1',
          [{ id: 'k', label: '默认', apiKey: 'sk-secret' }],
          ['gpt-4o'],
        ),
      ],
    })
    expect(envText).toContain("export OPENAI_API_KEY='sk-secret'")
    expect(envText).toContain("export OPENAI_BASE_URL='https://api.openai.com/v1'")
    expect(envText).toContain('first complete provider')
  })

  it('多 key：第一把规范名 + 中文备注回退 KEY{n}，不丢第二把', () => {
    const { envText } = buildExportPayload({
      providers: [
        p(
          'd',
          'https://api.deepseek.com',
          [
            { id: 'k1', label: '主号', apiKey: 'ds1' },
            { id: 'k2', label: '备用', apiKey: 'ds2' },
          ],
          ['chat'],
        ),
      ],
    })
    expect(envText).toContain("export DEEPSEEK_API_KEY='ds1'")
    expect(envText).toContain("export DEEPSEEK_KEY2_API_KEY='ds2'")
    expect(envText).not.toMatch(/DEEPSEEK___/)
  })

  it('多 key 英文备注用 TAG 后缀', () => {
    const { envText } = buildExportPayload({
      providers: [
        p(
          'd',
          'https://api.deepseek.com',
          [
            { id: 'k1', label: 'main', apiKey: 'ds1' },
            { id: 'k2', label: 'backup', apiKey: 'ds2' },
          ],
          ['chat'],
        ),
      ],
    })
    expect(envText).toContain("export DEEPSEEK_API_KEY='ds1'")
    expect(envText).toContain("export DEEPSEEK_BACKUP_API_KEY='ds2'")
  })

  it('envLabelTag / assignKeyEnvNames', () => {
    expect(envLabelTag('主号')).toBe('')
    expect(envLabelTag('backup-1')).toBe('BACKUP_1')
    const names = assignKeyEnvNames(
      p(
        'd',
        'https://api.deepseek.com',
        [
          { id: 'k1', label: '主号', apiKey: 'a' },
          { id: 'k2', label: '备用', apiKey: 'b' },
          { id: 'k3', label: '备用', apiKey: 'c' },
        ],
        ['m'],
      ),
    )
    expect(names.map((n) => n.envName)).toEqual([
      'DEEPSEEK_API_KEY',
      'DEEPSEEK_KEY2_API_KEY',
      'DEEPSEEK_KEY3_API_KEY',
    ])
  })

  it('两个单 Key 同端点：第二套序号兜底不丢', () => {
    const taken = new Set<string>()
    const a = assignKeyEnvNames(
      p('d1', 'https://api.deepseek.com', [{ id: 'k1', label: '默认', apiKey: 'ds1' }]),
      taken,
    )
    const b = assignKeyEnvNames(
      p('d2', 'https://api.deepseek.com', [{ id: 'k2', label: '默认', apiKey: 'ds2' }]),
      taken,
    )
    expect(a.map((n) => n.envName)).toEqual(['DEEPSEEK_API_KEY'])
    expect(b.map((n) => n.envName)).toEqual(['DEEPSEEK_KEY1_API_KEY'])
    const { envText } = buildExportPayload({
      providers: [
        p('d1', 'https://api.deepseek.com', [{ id: 'k1', label: '默认', apiKey: 'ds1' }]),
        p('d2', 'https://api.deepseek.com', [{ id: 'k2', label: '默认', apiKey: 'ds2' }]),
      ],
    })
    expect(envText).toContain("export DEEPSEEK_API_KEY='ds1'")
    expect(envText).toContain("export DEEPSEEK_KEY1_API_KEY='ds2'")
  })

  it('智谱 → ZHIPU_API_KEY + ANTHROPIC_*；DeepSeek → DEEPSEEK_API_KEY', () => {
    const { envText } = buildExportPayload({
      providers: [
        p(
          'd',
          'https://api.deepseek.com',
          [{ id: 'k', label: '默认', apiKey: 'sk-ds' }],
          ['deepseek-v4-pro'],
        ),
        p(
          'z',
          'https://open.bigmodel.cn/api/coding/paas/v4',
          [{ id: 'k', label: '195', apiKey: 'sk-zhipu' }],
          ['glm-5.2'],
        ),
      ],
    })
    expect(envText).toContain("export DEEPSEEK_API_KEY='sk-ds'")
    expect(envText).toContain("export ZHIPU_API_KEY='sk-zhipu'")
    expect(envText).not.toContain('BIGMODEL_API_KEY')
    expect(envText).toContain(`export ANTHROPIC_AUTH_TOKEN='sk-zhipu'`)
    expect(envText).toContain(`export ANTHROPIC_BASE_URL='${ZHIPU_ANTHROPIC_BASE_URL}'`)
    expect(envText).toContain("export ANTHROPIC_DEFAULT_SONNET_MODEL='glm-5.2[1M]'")
    expect(anthropicModelFromZhipu(['glm-5.2'])).toBe('glm-5.2[1M]')
    expect(anthropicModelFromZhipu(['glm-5.2[1M]'])).toBe('glm-5.2[1M]')
  })
})

describe('helpers', () => {
  it('shell / mask / remain / compact', () => {
    expect(shellSingleQuote("a'b")).toBe(`'a'\\''b'`)
    expect(baseUrlEnvName('DEEPSEEK_API_KEY')).toBe('DEEPSEEK_BASE_URL')
    expect(maskKey('sk-abcdefghij')).toMatch(/…/)
    expect(maskKey('sk-abcdefghijklmnop')).toMatch(/^sk-abc…mnop$/)
    expect(formatWindowRemain(Date.now() + 2.3 * 3_600_000, 'h')).toBe('2.3h')
    expect(formatWindowRemain(Date.now() + 2.3 * 86_400_000, 'd')).toBe('2.3d')
    expect(formatCompactCount(12_300)).toBe('12.3K')
    expect(formatCompactCount(2_500_000)).toBe('2.5M')
    expect(formatCompactCount(1_200_000_000)).toBe('1.2B')
  })

  it('normalizeZhipuMonitor / subtitle', () => {
    const now = Date.now()
    const m = normalizeZhipuMonitor({
      level: 'max',
      expired: false,
      fiveHour: { percentage: 12, nextResetTime: now + 2.3 * 3_600_000 },
      weekly: { percentage: 34, nextResetTime: now + 2.3 * 86_400_000 },
      totalCalls: 10,
      totalTokens: 1_200_000_000,
      tokensSeries: [1, 2, 3],
    })
    expect(m.kind).toBe('zhipu')
    expect(m.level).toBe('max')
    const sub = formatKeyUsageSubtitle('sk-abcdefghijklmnop', m, now)
    expect(sub).toBe('sk-abc…mnop · MAX · 5h 12% (2.3h) · 7d 34% (2.3d) · 30d 1.2B')
    // 无重置时间 → 横杠
    const mNoReset = normalizeZhipuMonitor({
      level: 'max',
      fiveHour: { percentage: 12, nextResetTime: 0 },
      weekly: { percentage: 34 },
      totalTokens: 100,
      tokensSeries: [1],
    })
    expect(formatKeyUsageSubtitle('sk-abcdefghijklmnop', mNoReset, now)).toBe(
      'sk-abc…mnop · MAX · 5h 12% (—) · 7d 34% (—) · 30d 100',
    )
    // snake_case 兜底
    const m2 = normalizeZhipuMonitor({
      level: 'lite',
      total_calls: 99,
      total_tokens: 1000,
      tokens_series: [1, 2],
      five_hour: { percentage: 1, next_reset_time: Date.now() + 1000 },
    })
    expect(m2.totalCalls).toBe(99)
    expect(m2.tokensSeries).toEqual([1, 2])
  })

  it('normalizeDeepseekBalance / subtitle', () => {
    const m = normalizeDeepseekBalance({
      is_available: true,
      balance_infos: [
        {
          currency: 'CNY',
          total_balance: '110.00',
          granted_balance: '10.00',
          topped_up_balance: '100.00',
        },
      ],
    })
    expect(m.kind).toBe('deepseek')
    expect(m.isAvailable).toBe(true)
    expect(m.balanceInfos[0].totalBalance).toBe('110.00')
    const sub = formatDeepseekBalanceSubtitle('sk-abcdefghijklmnop', m)
    expect(sub).toMatch(/sk-abc…/)
    expect(sub).toMatch(/¥110\.00/)
  })
})
