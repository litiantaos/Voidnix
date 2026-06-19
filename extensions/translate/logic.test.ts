import { describe, it, expect } from 'vitest'
import { cleanStreamResult, engineLabel } from './logic'
import type { TranslateApiConfig } from './config'

describe('cleanStreamResult', () => {
  it('去代码块围栏（无语言标记）', () => {
    expect(cleanStreamResult('```\nhello\n```')).toBe('hello')
  })

  it('去代码块围栏（带语言标记）', () => {
    expect(cleanStreamResult('```json\n{"a":1}\n```')).toBe('{"a":1}')
  })

  it('去 ASCII 双引号配对', () => {
    expect(cleanStreamResult('"hello world"')).toBe('hello world')
  })

  it('去中文智能引号 “” / 「」 / 『』', () => {
    expect(cleanStreamResult('\u{201C}hello\u{201D}')).toBe('hello')
    expect(cleanStreamResult('\u{300C}你好\u{300D}')).toBe('你好')
    expect(cleanStreamResult('\u{300E}你好\u{300F}')).toBe('你好')
  })

  it('去英文前导文本', () => {
    expect(cleanStreamResult('Here is the translation: hello')).toBe('hello')
    expect(cleanStreamResult("Here's the translation:\nhello")).toBe('hello')
    expect(cleanStreamResult('The translation is hello')).toBe('hello')
  })

  it('去中文前导文本', () => {
    expect(cleanStreamResult('翻译结果：你好')).toBe('你好')
    expect(cleanStreamResult('以下是翻译\n你好')).toBe('你好')
    expect(cleanStreamResult('翻译如下: 测试')).toBe('测试')
  })

  it('围栏 + 前导组合', () => {
    expect(cleanStreamResult('```\n翻译结果：你好\n```')).toBe('你好')
  })

  it('纯文本不变', () => {
    expect(cleanStreamResult('你好世界')).toBe('你好世界')
    expect(cleanStreamResult('hello world')).toBe('hello world')
  })

  it('trim 首尾空白', () => {
    expect(cleanStreamResult('  hello  ')).toBe('hello')
  })

  it('空字符串', () => {
    expect(cleanStreamResult('')).toBe('')
  })
})

describe('engineLabel', () => {
  it('youdao 类型固定「有道翻译」', () => {
    const cfg = { type: 'youdao', appKey: 'k', appSecret: 's' } as TranslateApiConfig
    expect(engineLabel(cfg)).toBe('有道翻译')
  })

  it('AI 类型从 endpoint 取 provider 名', () => {
    const cfg = {
      type: 'ai',
      endpoint: 'https://api.openai.com/v1',
      apiKey: 'k',
    } as unknown as TranslateApiConfig
    expect(engineLabel(cfg)).toBe('OPENAI')
  })

  it('AI 类型 endpoint 空 → fallback「翻译」', () => {
    const cfg = { type: 'ai', endpoint: '', apiKey: 'k' } as unknown as TranslateApiConfig
    expect(engineLabel(cfg)).toBe('翻译')
  })
})
