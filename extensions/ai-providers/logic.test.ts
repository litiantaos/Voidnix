import { describe, it, expect } from 'vitest'
import {
  shellSingleQuote,
  baseUrlEnvName,
  buildExportPayload,
  maskKey,
  formatWindowRemain,
  formatCompactCount,
  formatKeyUsageSubtitle,
  formatDeepseekBalanceSubtitle,
  normalizeZhipuMonitor,
  normalizeDeepseekBalance,
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

  it('多 key 命名导出', () => {
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
    expect(envText).toContain('ds1')
    expect(envText).toContain('DEEPSEEK')
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
    expect(sub).toBe(
      'sk-abc…mnop · MAX · 5h 12% / 2.3h · 7d 34% / 2.3d · 30d 1.2B tokens',
    )
    // 无重置时间 → 横杠
    const mNoReset = normalizeZhipuMonitor({
      level: 'max',
      fiveHour: { percentage: 12, nextResetTime: 0 },
      weekly: { percentage: 34 },
      totalTokens: 100,
      tokensSeries: [1],
    })
    expect(formatKeyUsageSubtitle('sk-abcdefghijklmnop', mNoReset, now)).toBe(
      'sk-abc…mnop · MAX · 5h 12% / — · 7d 34% / — · 30d 100 tokens',
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
