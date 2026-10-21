import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import './locales'
import Preview from './Preview.vue'

const { writeText } = vi.hoisted(() => ({ writeText: vi.fn(() => Promise.resolve()) }))
const { showToast } = vi.hoisted(() => ({ showToast: vi.fn() }))
const { open } = vi.hoisted(() => ({ open: vi.fn(() => Promise.resolve()) }))

vi.mock('@/utils/clipboard', () => ({ writeText }))
vi.mock('@/composables/useToast', () => ({ showToast }))
vi.mock('@tauri-apps/plugin-shell', () => ({ open }))

/// renderMarkdown 代码块契约（语言栏 + 复制按钮 + pre/code）。
/// happy-dom 的 DOMPurify 会剥掉 .md-code 外壳，真渲染输出由 utils/markdown 单测覆盖，
/// 此处手工注入以验证 Preview 的点击委托。
const CODE_BLOCK_HTML =
  '<div class="md-code">' +
  '<div class="md-code-bar"><button type="button" class="md-code-copy">' +
  '<i class="i-ri-file-copy-line"></i></button></div>' +
  '<pre><code>const a = 1</code></pre></div>'

function mountWithCodeBlock() {
  const w = mount(Preview, { props: { text: 'x' } })
  const root = w.find('.notes-preview').element as HTMLElement
  root.insertAdjacentHTML('beforeend', CODE_BLOCK_HTML)
  return { w, btn: w.find('.md-code-copy') }
}

describe('notes Preview', () => {
  beforeEach(() => {
    writeText.mockClear()
    showToast.mockClear()
    open.mockClear()
  })

  it('渲染 markdown 到 .markdown-body', () => {
    const w = mount(Preview, { props: { text: '# 标题\n\n正文' } })
    // happy-dom 下 DOMPurify 会剥块级外壳（详见 agent view-logic 测试同环境注释），断言文本即可
    const body = w.find('.markdown-body')
    expect(body.text()).toContain('标题')
    expect(body.text()).toContain('正文')
  })

  it('空文本显示空态,不渲染 markdown 容器', () => {
    const w = mount(Preview, { props: { text: '' } })
    expect(w.find('.markdown-body').exists()).toBe(false)
    expect(w.find('.notes-preview').text()).toBe('还没有内容')
  })

  it('代码块复制:写剪贴板 + 图标翻转 check', async () => {
    const { btn } = mountWithCodeBlock()
    await btn.trigger('click')
    await flushPromises()

    expect(writeText).toHaveBeenCalledWith('const a = 1')
    expect(btn.find('i').classes()).toContain('i-ri-check-line')
  })

  it('代码块复制失败:错误 toast,图标不翻转', async () => {
    writeText.mockRejectedValueOnce(new Error('ipc failed'))
    const { btn } = mountWithCodeBlock()
    await btn.trigger('click')
    await flushPromises()

    expect(showToast).toHaveBeenCalledWith(expect.any(String), { kind: 'error' })
    expect(btn.find('i').classes()).toContain('i-ri-file-copy-line')
  })

  it('链接点击经系统浏览器打开', async () => {
    const w = mount(Preview, { props: { text: '[链接](https://example.com/a)' } })
    await w.find('.markdown-body a').trigger('click')
    expect(open).toHaveBeenCalledWith('https://example.com/a')
  })
})
