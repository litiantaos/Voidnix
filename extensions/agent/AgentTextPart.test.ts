import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import AgentTextPart from './AgentTextPart.vue'

/// renderMarkdown mock 为确定性标记：组件测试只验分块/DOM 保留/无游离杂物，
/// markdown 管线由 view-logic.test.ts + 真浏览器验证覆盖（happy-dom 下 DOMPurify
/// 剥标签行为不稳定，不作为断言对象）。
vi.mock('@/utils/markdown', () => ({
  renderMarkdown: (s: string) => `<p>${s}</p>`,
}))
vi.mock('@/utils/clipboard', () => ({ writeText: vi.fn() }))
vi.mock('@/composables/useToast', () => ({ showToast: vi.fn() }))
import './locales'

/** 非空游离文本节点（模板残留杂物，如孤立的行号） */
function strayTextNodes(el: HTMLElement): Text[] {
  return Array.from(el.childNodes).filter(
    (n): n is Text => n.nodeType === 3 && (n.textContent || '').trim() !== '',
  )
}

describe('AgentTextPart 增量渲染', () => {
  it('流式态：块容器 + 拖尾，无游离文本节点', () => {
    const w = mount(AgentTextPart, {
      props: { text: 'para one\n\npara two\n\nlast', streaming: true },
    })
    const root = w.element as HTMLElement
    expect(strayTextNodes(root)).toHaveLength(0)
    const els = Array.from(root.children)
    expect(els).toHaveLength(3)
    expect(els[0]!.className).toBe('md-solid')
    expect(els[1]!.className).toBe('md-solid')
    expect(els[2]!.className).toBe('md-tail')
    expect(els[2]!.textContent).toBe('last')
    // 块内容经 renderMarkdown 渲染（块文本已去尾换行）
    expect(els[0]!.querySelector('p')?.textContent).toBe('para one')
  })

  it('收尾保留前缀块 DOM：同一 v-for 仅类名/末块变化，tail 卸载', async () => {
    const w = mount(AgentTextPart, { props: { text: 'a\n\nb\n\nc', streaming: true } })
    expect(w.findAll('.md-solid')).toHaveLength(2) // a、b 为完成块，c 为 tail
    const firstEl = w.findAll('.md-solid')[0]!.element

    await w.setProps({ streaming: false })
    const fulls = w.findAll('.md-full')
    expect(fulls).toHaveLength(3) // a、b、c 全为块
    expect(fulls[0]!.element).toBe(firstEl) // 前缀元素未卸载重建
    expect(w.find('.md-tail').exists()).toBe(false)
    expect(strayTextNodes(w.element as HTMLElement)).toHaveLength(0)
  })

  it('流式尾行并入末块：宽松列表项收尾时块内容更新而非新增', async () => {
    // 流式：blocks=['1. a'] + tail '2. b'；收尾合并为一块
    const w = mount(AgentTextPart, { props: { text: '1. a\n\n2. b', streaming: true } })
    expect(w.findAll('.md-solid')).toHaveLength(1)

    await w.setProps({ streaming: false })
    expect(w.findAll('.md-full')).toHaveLength(1)
    expect(w.findAll('p')[0]!.text()).toBe('1. a\n\n2. b')
  })
})
