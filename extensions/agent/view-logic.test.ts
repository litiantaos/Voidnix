import { describe, it, expect, beforeEach } from 'vitest'
import {
  streamView,
  splitStreamBlocks,
  resetStreamViewCache,
  showToolBody,
  toolIcon,
  toolLabel,
  toolDetail,
  isStreamingText,
  getMessageText,
  buildHistoryLabel,
  streamLayoutKey,
  partKey,
} from './view-logic'
import './locales'
import {
  renderMarkdown,
  renderCodeBlock,
  renderListItemHtml,
  sanitizeCodeLang,
} from '@/utils/markdown'
import type { AgentMessage, AgentPart } from '@/types/agent'

function tool(
  partial: Partial<Extract<AgentPart, { type: 'toolCall' }>> & { name: string },
): Extract<AgentPart, { type: 'toolCall' }> {
  return {
    type: 'toolCall',
    id: partial.id ?? 't1',
    name: partial.name,
    state: partial.state ?? 'done',
    args: partial.args,
    output: partial.output,
    parsed: partial.parsed,
  }
}

beforeEach(() => {
  resetStreamViewCache()
})

describe('streamView', () => {
  it('无换行时全部为 tail', () => {
    expect(streamView('hello')).toEqual({ text: 'hello', blocks: [], tail: 'hello', lineKey: 0 })
  })

  it('含换行时 blocks 为 solid 分块，tail 为末行', () => {
    const v = streamView('a\nb')
    expect(v.blocks).toEqual(['a'])
    expect(v.tail).toBe('b')
    expect(v.lineKey).toBe(1)
  })

  it('同 text 返回同一缓存引用', () => {
    const a = streamView('x\ny')
    const b = streamView('x\ny')
    expect(a).toBe(b)
  })
})

describe('splitStreamBlocks（流式增量分块）', () => {
  it('空串与纯空白无块', () => {
    expect(splitStreamBlocks('')).toEqual([])
    expect(splitStreamBlocks('\n\n\n')).toEqual([])
  })

  it('段落按顶层空行分块', () => {
    expect(splitStreamBlocks('a\n\nb\n')).toEqual(['a', 'b'])
  })

  it('连续空行不产生空块', () => {
    expect(splitStreamBlocks('a\n\n\n\nb\n\n')).toEqual(['a', 'b'])
  })

  it('无空行分隔的整体为一块（含紧凑列表）', () => {
    expect(splitStreamBlocks('- a\n- b\ntail text')).toEqual(['- a\n- b\ntail text'])
  })

  it('宽松列表经空行延续保持一块（序号不断）', () => {
    expect(splitStreamBlocks('- a\n\n- b')).toEqual(['- a\n\n- b'])
    expect(splitStreamBlocks('1. a\n\n2. b')).toEqual(['1. a\n\n2. b'])
  })

  it('嵌套缩进延续不切列表', () => {
    expect(splitStreamBlocks('- a\n  - sub\n\n- b')).toEqual(['- a\n  - sub\n\n- b'])
  })

  it('多段引用保持一块', () => {
    expect(splitStreamBlocks('> a\n\n> b')).toEqual(['> a\n\n> b'])
  })

  it('fence 内空行不切块，fence 结束后新块', () => {
    expect(splitStreamBlocks('```ts\na\n\nb\n```\n\nafter')).toEqual([
      '```ts\na\n\nb\n```',
      'after',
    ])
  })

  it('波浪线 fence 同样生效', () => {
    expect(splitStreamBlocks('~~~\nx\n\ny\n~~~\n\nz')).toEqual(['~~~\nx\n\ny\n~~~', 'z'])
  })

  it('列表项的缩进续行不切块', () => {
    expect(splitStreamBlocks('- a\n  wrapped\n  more\n\n- b')).toEqual([
      '- a\n  wrapped\n  more\n\n- b',
    ])
  })

  it('流式尾行并入末块：宽松列表项收尾时与前块合并', () => {
    // 流式态：solid 到 '1. a\n\n'，tail 为 '2. b'
    const sv = streamView('1. a\n\n2. b')
    expect(sv.blocks).toEqual(['1. a'])
    expect(sv.tail).toBe('2. b')
    // 收尾态：尾行并入，整列表成一块（序号连续）
    expect(splitStreamBlocks('1. a\n\n2. b')).toEqual(['1. a\n\n2. b'])
  })

  it('列表后接非延续段落切两块', () => {
    expect(splitStreamBlocks('- a\n\nplain')).toEqual(['- a', 'plain'])
  })

  it('流式尾段（无收尾空行）保留为末块', () => {
    expect(splitStreamBlocks('a\n\nb')).toEqual(['a', 'b'])
  })

  it('分块重组无损：join 后与原文一致', () => {
    const text = '# t\n\n- a\n\n- b\n\n```rs\nlet x = 1;\n\nlet y = 2;\n```\n\n> q\n\n> r\n'
    const rejoined = splitStreamBlocks(text)
      .map((b) => b + '\n\n')
      .join('')
      .trimEnd()
    expect(rejoined).toBe(text.trimEnd())
  })
})

describe('showToolBody', () => {
  it('web_search 成功有 answer 则展示', () => {
    expect(showToolBody(tool({ name: 'web_search', state: 'done', parsed: '摘要内容' }))).toBe(true)
  })

  it('web_search 成功无 answer 不展示', () => {
    expect(
      showToolBody(
        tool({ name: 'web_search', state: 'done', output: '{"hits":[]}', parsed: undefined }),
      ),
    ).toBe(false)
  })

  it('web_search 失败有 output 则展示', () => {
    expect(showToolBody(tool({ name: 'web_search', state: 'failed', output: 'missing key' }))).toBe(
      true,
    )
  })

  it('run_command 有 output 则展示', () => {
    expect(showToolBody(tool({ name: 'run_command', state: 'done', output: 'ok' }))).toBe(true)
  })

  it('running 不展示', () => {
    expect(showToolBody(tool({ name: 'run_command', state: 'running', output: 'x' }))).toBe(false)
  })
})

describe('tool helpers', () => {
  it('toolIcon', () => {
    expect(toolIcon('web_search')).toContain('search')
    expect(toolIcon('run_command')).toContain('terminal')
  })

  it('toolLabel 返回语义类别', () => {
    expect(toolLabel('web_search')).toBe('搜索')
    expect(toolLabel('run_command')).toBe('命令')
    expect(toolLabel('xxx')).toBe('工具')
  })

  it('toolDetail 解析 run_command / web_search', () => {
    expect(toolDetail(tool({ name: 'run_command', args: { cmd: 'ls', args: ['-la'] } }))).toBe(
      'ls -la',
    )
    expect(toolDetail(tool({ name: 'web_search', args: { query: '  void  ' } }))).toBe('void')
    expect(toolDetail(tool({ name: 'x', args: {} }))).toBe('')
  })
})

describe('isStreamingText / getMessageText', () => {
  it('仅最后一个 text part 在 streaming 时为 true', () => {
    const msg: AgentMessage = {
      id: 'a',
      role: 'assistant',
      streaming: true,
      parts: [
        { type: 'text', text: 'a' },
        { type: 'toolCall', id: '1', name: 'run_command', state: 'done' },
        { type: 'text', text: 'b' },
      ],
    }
    expect(isStreamingText(msg, 0)).toBe(false)
    expect(isStreamingText(msg, 2)).toBe(true)
    expect(getMessageText(msg)).toBe('ab')
  })

  it('非 streaming 全 false', () => {
    const msg: AgentMessage = {
      id: 'a',
      role: 'assistant',
      parts: [{ type: 'text', text: 'x' }],
    }
    expect(isStreamingText(msg, 0)).toBe(false)
  })
})

describe('buildHistoryLabel', () => {
  it('折叠空白并保留短文本', () => {
    expect(buildHistoryLabel('  hello   world  ', 1)).toBe('hello world')
  })

  it('超长文本截断并加省略号', () => {
    const long = 'a'.repeat(80)
    const out = buildHistoryLabel(long, 1, 60)
    expect(out).toHaveLength(61)
    expect(out.endsWith('…')).toBe(true)
  })

  it('空文本回退序号占位', () => {
    expect(buildHistoryLabel('   ', 3)).toBe('#3')
    expect(buildHistoryLabel('', 1)).toBe('#1')
  })
})

describe('streamLayoutKey', () => {
  it('随 streaming 文本长度变化', () => {
    const base: AgentMessage[] = [
      {
        id: 'a',
        role: 'assistant',
        streaming: true,
        parts: [{ type: 'text', text: 'hi' }],
      },
    ]
    const k1 = streamLayoutKey(base, true)
    base[0]!.parts[0] = { type: 'text', text: 'hello' }
    const k2 = streamLayoutKey(base, true)
    expect(k1).not.toBe(k2)
    expect(streamLayoutKey(base, false)).not.toBe(k2)
  })
})

describe('partKey', () => {
  it('tool 用 id，notice/reasoning/text 用 type+index', () => {
    expect(partKey({ type: 'toolCall', id: 'c1', name: 'x', state: 'done' }, 0)).toBe('c1')
    expect(partKey({ type: 'notice', kind: 'error', text: 'e' }, 2)).toBe('notice-error-2')
    expect(partKey({ type: 'reasoning', text: '思考' }, 1)).toBe('reasoning-1')
    expect(partKey({ type: 'text', text: 'a' }, 1)).toBe('text-1')
  })
})

describe('sanitizeCodeLang / renderCodeBlock', () => {
  it('语言只取首 token 且白名单', () => {
    expect(sanitizeCodeLang('ts')).toBe('ts')
    expect(sanitizeCodeLang('  rust  ')).toBe('rust')
    expect(sanitizeCodeLang('ts {hl}')).toBe('ts')
    expect(sanitizeCodeLang('a<script>')).toBe('')
    expect(sanitizeCodeLang(undefined)).toBe('')
  })

  it('代码块含语言标签、复制按钮与转义内容', () => {
    const html = renderCodeBlock('a <b>&', 'ts')
    expect(html).toContain('class="md-code"')
    expect(html).toContain('md-code-lang')
    expect(html).toContain('>ts<')
    expect(html).toContain('md-code-copy')
    expect(html).toContain('language-ts')
    expect(html).toContain('a &lt;b&gt;&amp;')
    expect(html).not.toContain('a <b>&')
  })

  it('无语言时仍有复制按钮', () => {
    const html = renderCodeBlock('x')
    expect(html).toContain('md-code-lang--empty')
    expect(html).toContain('md-code-copy')
    expect(html).not.toContain('language-')
  })
})

describe('renderMarkdown', () => {
  it('空串返回空', () => {
    expect(renderMarkdown('')).toBe('')
    expect(renderMarkdown(null)).toBe('')
  })

  it('渲染段落并给 a 加 target', () => {
    const html = renderMarkdown('[x](https://example.com)')
    expect(html).toContain('href="https://example.com"')
    expect(html).toContain('target="_blank"')
    expect(html).toContain('noopener')
  })

  it('fenced 代码块带外壳与语言', () => {
    const html = renderMarkdown('```python\nprint(1)\n```')
    expect(html).toContain('md-code')
    expect(html).toContain('md-code-copy')
    expect(html).toContain('python')
    expect(html).toContain('print(1)')
  })

  it('有序/无序列表共用固定标记外壳', () => {
    // happy-dom + DOMPurify 会剥掉 ul/ol 外壳，断言项级结构即可
    const ul = renderMarkdown('- a\n- b')
    const ol = renderMarkdown('1. x\n2. y')
    expect(ul).toContain('md-li-mark')
    expect(ul).toContain('md-li-body')
    expect(ul).toContain('•')
    expect(ol).toContain('md-li-mark')
    expect(ol).toContain('1.')
    expect(ol).toContain('2.')
    expect(renderListItemHtml('•', 'a')).toContain('class="md-li"')
  })
})
