import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import BaseProgress from './BaseProgress.vue'

describe('BaseProgress', () => {
  it('确定进度：填充宽度 = value%（四舍五入），aria-valuenow 同步', () => {
    const w = mount(BaseProgress, { props: { value: 42.4 } })
    expect(w.find('.progress-fill').attributes('style')).toContain('width: 42%')
    expect(w.find('[role="progressbar"]').attributes('aria-valuenow')).toBe('42')
    w.unmount()
  })

  it('默认 accent + md 6px；tone/size 可切语义色与细条', () => {
    const def = mount(BaseProgress, { props: { value: 50 } })
    expect(def.find('.progress-fill').classes()).toContain('bg-accent')
    expect(def.find('.progress-track').classes()).toContain('h-1.5')
    def.unmount()

    const meter = mount(BaseProgress, {
      props: { value: 72, tone: 'warning', size: 'sm' },
    })
    expect(meter.find('.progress-fill').classes()).toContain('bg-warning')
    expect(meter.find('.progress-track').classes()).toContain('h-0.5')
    meter.unmount()
  })

  it('value 越界钳制到 0-100', () => {
    const over = mount(BaseProgress, { props: { value: 150 } })
    expect(over.find('[role="progressbar"]').attributes('aria-valuenow')).toBe('100')
    const under = mount(BaseProgress, { props: { value: -3 } })
    expect(under.find('[role="progressbar"]').attributes('aria-valuenow')).toBe('0')
    over.unmount()
    under.unmount()
  })

  it('不定进度（null）：渲染循环跑条，无 aria-valuenow', () => {
    const w = mount(BaseProgress, { props: { value: null } })
    expect(w.find('.progress-indeterminate').exists()).toBe(true)
    expect(w.find('[role="progressbar"]').attributes('aria-valuenow')).toBeUndefined()
    w.unmount()
  })
})
