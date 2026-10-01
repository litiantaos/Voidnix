import { describe, it, expect } from 'vitest'
import {
  maskKey,
  formatWindowRemain,
  formatCompactCount,
  formatKeyUsageSubtitle,
  formatDeepseekBalanceSubtitle,
  normalizeZhipuMonitor,
  normalizeDeepseekBalance,
} from './logic'

describe('helpers', () => {
  it('mask / remain / compact', () => {
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
