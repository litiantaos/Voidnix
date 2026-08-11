import { describe, it, expect } from 'vitest'
import './locales'
import { cleanStreamResult, engineLabel, detectSpeechLang } from './logic'
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

  it('AI 类型固定「AI 翻译」', () => {
    const cfg = {
      type: 'ai',
      selections: [],
      prompt: '',
    } as unknown as TranslateApiConfig
    expect(engineLabel(cfg)).toBe('AI 翻译')
  })
})

describe('detectSpeechLang', () => {
  it('中文（含标点 / 数字）→ zh', () => {
    expect(detectSpeechLang('你好，世界！')).toBe('zh')
    expect(detectSpeechLang('第 3 次测试。')).toBe('zh')
  })

  it('英文 / 拉丁语系 → en', () => {
    expect(detectSpeechLang('Hello, world!')).toBe('en')
    expect(detectSpeechLang('Bonjour le monde')).toBe('en')
  })

  it('日文（假名）→ ja', () => {
    expect(detectSpeechLang('こんにちは')).toBe('ja')
    expect(detectSpeechLang('カタカナ')).toBe('ja')
  })

  it('汉字起始但含假名的日文 → ja（送り仮名）', () => {
    // 「今日は」首字符「今」是汉字，但末尾「は」是假名 → 应判日文
    expect(detectSpeechLang('今日は')).toBe('ja')
  })

  it('韩文（谚文）→ ko', () => {
    expect(detectSpeechLang('안녕하세요')).toBe('ko')
  })

  it('西里尔 → ru', () => {
    expect(detectSpeechLang('Привет мир')).toBe('ru')
  })

  it('阿拉伯 → ar', () => {
    expect(detectSpeechLang('مرحبا بالعالم')).toBe('ar')
  })

  it('泰文 → th', () => {
    expect(detectSpeechLang('สวัสดีชาวโลก')).toBe('th')
  })

  it('天城文 → hi', () => {
    expect(detectSpeechLang('नमस्ते दुनिया')).toBe('hi')
  })

  it('越南文（拉丁扩展附加）→ vi', () => {
    expect(detectSpeechLang('Xin chào thế giới')).toBe('vi')
  })

  it('混合脚本优先返回首个命中的非拉丁语种', () => {
    // 含汉字 + 假名：汉字代码点更靠前出现则 zh，此处「中文混入 English」首个非拉丁为汉字
    expect(detectSpeechLang('测试 English mixed')).toBe('zh')
  })

  it('空串 / 纯符号 → en（兜底）', () => {
    expect(detectSpeechLang('')).toBe('en')
    expect(detectSpeechLang('123 !? ')).toBe('en')
  })
})
