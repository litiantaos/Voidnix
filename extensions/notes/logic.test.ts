import { describe, it, expect } from 'vitest'
import { toChars, diffChars, buildIndexMap, ANIM_MAX_DIFF, FLIP_WINDOW } from './logic'

describe('toChars', () => {
  it('BMP 字符逐一拆分', () => {
    expect(toChars('abc')).toEqual(['a', 'b', 'c'])
  })

  it('换行/空格保留为独立单元', () => {
    expect(toChars('a b\nc')).toEqual(['a', ' ', 'b', '\n', 'c'])
  })

  it('emoji 代理对不拆半', () => {
    expect(toChars('a😀b')).toEqual(['a', '😀', 'b'])
  })
})

describe('diffChars', () => {
  it('全相同零变更', () => {
    expect(diffChars(['a', 'b'], ['a', 'b'])).toEqual({
      prefix: 2,
      suffix: 0,
      removed: 0,
      added: 0,
    })
  })

  it('双空零变更', () => {
    expect(diffChars([], [])).toEqual({ prefix: 0, suffix: 0, removed: 0, added: 0 })
  })

  it('末尾追加', () => {
    expect(diffChars(['a'], ['a', 'b'])).toEqual({ prefix: 1, suffix: 0, removed: 0, added: 1 })
  })

  it('末尾删除', () => {
    expect(diffChars(['a', 'b'], ['a'])).toEqual({ prefix: 1, suffix: 0, removed: 1, added: 0 })
  })

  it('中间插入', () => {
    expect(diffChars(['a', 'c'], ['a', 'b', 'c'])).toEqual({
      prefix: 1,
      suffix: 1,
      removed: 0,
      added: 1,
    })
  })

  it('中间删除', () => {
    expect(diffChars(['a', 'b', 'c'], ['a', 'c'])).toEqual({
      prefix: 1,
      suffix: 1,
      removed: 1,
      added: 0,
    })
  })

  it('选中替换(删+增并存)', () => {
    expect(diffChars(['a', 'b', 'c'], ['a', 'x', 'c'])).toEqual({
      prefix: 1,
      suffix: 1,
      removed: 1,
      added: 1,
    })
  })

  it('空→非空 / 非空→空', () => {
    expect(diffChars([], ['h', 'i'])).toEqual({ prefix: 0, suffix: 0, removed: 0, added: 2 })
    expect(diffChars(['h', 'i'], [])).toEqual({ prefix: 0, suffix: 0, removed: 2, added: 0 })
  })

  it('前后缀竞争时取最大前缀(单点编辑假设)', () => {
    // 'aa'→'a':前缀吃掉 1,removed=1
    expect(diffChars(['a', 'a'], ['a'])).toEqual({ prefix: 1, suffix: 0, removed: 1, added: 0 })
    // 'ab'→'b':前缀 0,后缀 1
    expect(diffChars(['a', 'b'], ['b'])).toEqual({ prefix: 0, suffix: 1, removed: 1, added: 0 })
  })

  it('emoji 作为整体单元参与 diff', () => {
    expect(diffChars(['a', '😀'], ['a'])).toEqual({ prefix: 1, suffix: 0, removed: 1, added: 0 })
  })
})

describe('buildIndexMap', () => {
  it('纯 ASCII 双向恒等', () => {
    const m = buildIndexMap('abc')
    expect(m.cu2cp).toEqual([0, 1, 2, 3])
    expect(m.cpStart).toEqual([0, 1, 2, 3])
  })

  it('空串', () => {
    const m = buildIndexMap('')
    expect(m.cu2cp).toEqual([0])
    expect(m.cpStart).toEqual([0])
  })

  it('emoji 代理对:cp 粒度映射,无稀疏洞', () => {
    const m = buildIndexMap('a😀b')
    // cu:0='a' 1-2='😀' 3='b';cp:0='a' 1='😀' 2='b'
    expect(m.cu2cp).toEqual([0, 1, 2, 2, 3])
    expect(m.cpStart).toEqual([0, 1, 3, 4])
  })

  it('selectionStart(=cpStart[cp]) 与 cu2cp 互逆', () => {
    const text = 'a😀b\n文'
    const m = buildIndexMap(text)
    for (let cp = 0; cp <= toChars(text).length; cp++) {
      expect(m.cu2cp[m.cpStart[cp]]).toBe(cp)
    }
  })
})

describe('动画降级阈值', () => {
  it('ANIM_MAX_DIFF / FLIP_WINDOW 为正整数常量', () => {
    expect(Number.isInteger(ANIM_MAX_DIFF)).toBe(true)
    expect(ANIM_MAX_DIFF).toBeGreaterThan(0)
    expect(Number.isInteger(FLIP_WINDOW)).toBe(true)
    expect(FLIP_WINDOW).toBeGreaterThan(0)
  })
})
