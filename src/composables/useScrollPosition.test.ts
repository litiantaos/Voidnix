import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref, nextTick } from 'vue'
import { useScrollPosition } from './useScrollPosition'

/// useScrollPosition：界面级滚动位置隔离工具。
/// save 写当前值、restore 读记录（无则归 0）、reset 强制归 0、clear 删记录使下次 restore 归 0。
/// 项目实际用法：仅 tools 往返保留，其余每次显示归顶（靠 clear 保证非 ext→tools 不残留旧记录）。
describe('useScrollPosition', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('restore 未 save 的 key 归顶', async () => {
    const y = ref(0)
    const { restore } = useScrollPosition(y)
    restore('tools')
    await nextTick()
    expect(y.value).toBe(0)
  })

  it('save + restore 命中记录恢复原值', async () => {
    const y = ref(0)
    const { save, restore } = useScrollPosition(y)
    y.value = 120
    save('tools')
    y.value = 0
    restore('tools')
    await nextTick()
    expect(y.value).toBe(120)
  })

  it('多 key 独立隔离互不影响', async () => {
    const y = ref(0)
    const { save, restore } = useScrollPosition(y)
    y.value = 10
    save('tools')
    y.value = 20
    save('home')
    y.value = 0
    restore('tools')
    await nextTick()
    expect(y.value).toBe(10)
    restore('home')
    await nextTick()
    expect(y.value).toBe(20)
  })

  it('reset 强制归顶（即便有记录）', async () => {
    const y = ref(0)
    const pos = useScrollPosition(y)
    y.value = 99
    pos.save('tools')
    pos.reset()
    await nextTick()
    expect(y.value).toBe(0)
    // reset 仅改当前值，不清记录：再 restore 仍命中旧值（同一实例 Map 贯穿生命周期）
    pos.restore('tools')
    await nextTick()
    expect(y.value).toBe(99)
  })

  it('clear 删记录后 restore 归顶', async () => {
    const y = ref(0)
    const { save, clear, restore } = useScrollPosition(y)
    y.value = 55
    save('tools')
    clear('tools')
    y.value = 0
    restore('tools')
    await nextTick()
    expect(y.value).toBe(0)
  })

  it('clear 不存在的 key 无副作用', async () => {
    const y = ref(0)
    const { save, clear, restore } = useScrollPosition(y)
    y.value = 30
    save('tools')
    clear('home') // 不影响 tools 记录
    restore('tools')
    await nextTick()
    expect(y.value).toBe(30)
  })

  it('restore nextTick 异步落地：调用后立即读 y 仍为旧值', async () => {
    const y = ref(0)
    const { save, restore } = useScrollPosition(y)
    y.value = 77
    save('tools')
    y.value = 0
    restore('tools')
    // 同步阶段：DOM 尚未切到新界面，y 保持调用前值（模拟 watch pre-flush 读完 oldKey 后 nextTick 落地）
    expect(y.value).toBe(0)
    await nextTick()
    expect(y.value).toBe(77)
  })
})
