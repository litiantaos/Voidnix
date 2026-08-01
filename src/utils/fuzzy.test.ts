import { describe, it, expect, beforeAll } from 'vitest'
import {
  scoreFields,
  frequencyBoost,
  keywordMatch,
  scoreExtensionEntry,
  pinyinReady,
} from './fuzzy'

// 拼音模块延迟加载：测试前等待就绪
beforeAll(async () => {
  await pinyinReady()
})

describe('scoreFields', () => {
  it('空 query 返回 0', () => {
    expect(scoreFields(['hello'], '')).toBe(0)
    expect(scoreFields(['hello'], '  ')).toBe(0)
  })

  it('空字段返回 0', () => {
    expect(scoreFields([], 'test')).toBe(0)
    expect(scoreFields([null, undefined], 'test')).toBe(0)
  })

  describe('英文子串匹配', () => {
    it('精确匹配得分最高', () => {
      const exact = scoreFields(['hello'], 'hello')
      const prefix = scoreFields(['hello world'], 'hello')
      expect(exact).toBeGreaterThan(prefix)
    })

    it('前缀匹配优于中间包含', () => {
      const prefix = scoreFields(['hello'], 'hel')
      const contain = scoreFields(['abchel'], 'hel')
      expect(prefix).toBeGreaterThan(contain)
    })

    it('大小写不敏感', () => {
      expect(scoreFields(['Hello World'], 'hello')).toBeGreaterThan(0)
      expect(scoreFields(['Hello World'], 'HELLO')).toBeGreaterThan(0)
      expect(scoreFields(['Hello World'], 'world')).toBeGreaterThan(0)
    })

    it('不匹配返回 0', () => {
      expect(scoreFields(['hello'], 'xyz')).toBe(0)
    })
  })

  describe('拼音匹配', () => {
    it('拼音首字母匹配中文', () => {
      expect(scoreFields(['微信'], 'wx')).toBeGreaterThan(0)
      expect(scoreFields(['微信'], 'weixin')).toBeGreaterThan(0)
    })

    it('全拼匹配', () => {
      expect(scoreFields(['计算器'], 'jisuanqi')).toBeGreaterThan(0)
    })

    it('中文 query 走子串匹配而非拼音', () => {
      expect(scoreFields(['你好世界'], '你好')).toBeGreaterThan(0)
    })

    it('纯英文文本不触发拼音匹配', () => {
      expect(scoreFields(['hello'], 'hel')).toBeGreaterThan(0)
    })

    it('ü→v 约定', () => {
      expect(scoreFields(['绿色'], 'lvse')).toBeGreaterThan(0)
    })
  })

  describe('多字段权重', () => {
    it('null 字段不消耗权重衰减', () => {
      const first = scoreFields(['hello', null], 'hel')
      const onlySecond = scoreFields([null, 'hello'], 'hel')
      expect(first).toBe(onlySecond)
    })

    it('后续字段衰减', () => {
      const s1 = scoreFields(['hello'], 'hel')
      const s2 = scoreFields(['xxx', 'hello'], 'hel')
      expect(s1).toBeGreaterThan(s2)
    })
  })
})

describe('keywordMatch', () => {
  it('空 query 或空 keywords 返回 0', () => {
    expect(keywordMatch(['usd'], '')).toBe(0)
    expect(keywordMatch([], 'usd')).toBe(0)
    expect(keywordMatch([null, undefined], 'usd')).toBe(0)
  })

  it('正向匹配：query 是 keyword 子串', () => {
    expect(keywordMatch(['usd', 'cny'], 'usd')).toBeGreaterThan(0)
    expect(keywordMatch(['currency exchange'], 'currency')).toBeGreaterThan(0)
  })

  it('反向匹配：keyword 是 query 子串（多词 query）', () => {
    // scoreFields 对此返回 0（query 比 keyword 长），keywordMatch 反向降权命中
    expect(keywordMatch(['usd'], '100 usd')).toBeGreaterThan(0)
    expect(keywordMatch(['汇率'], '美元汇率')).toBeGreaterThan(0)
    expect(keywordMatch(['ip'], 'ip 查询')).toBeGreaterThan(0)
  })

  it('反向匹配降权 0.5（弱于正向精确匹配）', () => {
    const exact = keywordMatch(['usd'], 'usd')
    const reverse = keywordMatch(['usd'], '100 usd')
    expect(exact).toBeGreaterThan(reverse)
  })

  it('拼音匹配中文 keyword', () => {
    expect(keywordMatch(['汇率'], 'hl')).toBeGreaterThan(0)
    expect(keywordMatch(['计算'], 'js')).toBeGreaterThan(0)
  })

  it('无任何命中返回 0', () => {
    expect(keywordMatch(['usd'], 'xyz')).toBe(0)
    expect(keywordMatch(['汇率'], 'abc')).toBe(0)
  })
})

describe('scoreExtensionEntry', () => {
  it('name / id 正向命中', () => {
    expect(scoreExtensionEntry({ name: '计算器', id: 'calculator' }, '计算')).toBeGreaterThan(0)
    expect(scoreExtensionEntry({ name: 'Base64', id: 'base64' }, 'base64')).toBeGreaterThan(0)
  })

  it('keywords 反向命中（与 keywordMatch 一致）', () => {
    expect(
      scoreExtensionEntry({ name: '汇率', id: 'currency', keywords: ['usd'] }, '100 usd'),
    ).toBeGreaterThan(0)
  })

  it('无命中返回 0', () => {
    expect(scoreExtensionEntry({ name: '时间', id: 'time', keywords: ['date'] }, 'xyz')).toBe(0)
  })
})

describe('frequencyBoost', () => {
  it('0 或负数返回 0', () => {
    expect(frequencyBoost(0)).toBe(0)
    expect(frequencyBoost(-1)).toBe(0)
  })

  it('单调递增', () => {
    expect(frequencyBoost(1)).toBeGreaterThan(0)
    expect(frequencyBoost(10)).toBeGreaterThan(frequencyBoost(1))
    expect(frequencyBoost(100)).toBeGreaterThan(frequencyBoost(10))
  })

  it('上限 1500', () => {
    expect(frequencyBoost(100000)).toBeLessThanOrEqual(1500)
    expect(frequencyBoost(999999)).toBeLessThanOrEqual(1500)
  })

  it('具体值校验', () => {
    expect(frequencyBoost(1)).toBe(150)
    expect(frequencyBoost(10)).toBe(519)
  })
})
