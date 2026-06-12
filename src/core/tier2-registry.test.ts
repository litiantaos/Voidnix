import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@/utils/clipboard', () => ({
  copyAndHide: vi.fn<(text: string) => Promise<void>>(),
}))

import { copyAndHide } from '@/utils/clipboard'
import { handleTier2Action } from './tier2-registry'

type CallWorker = (method: string, ...params: unknown[]) => Promise<unknown>

describe('handleTier2Action', () => {
  let callWorker: CallWorker

  beforeEach(() => {
    vi.clearAllMocks()
    callWorker = vi.fn<CallWorker>().mockResolvedValue(undefined) as unknown as CallWorker
    ;(
      copyAndHide as unknown as ReturnType<typeof vi.fn<(text: string) => Promise<void>>>
    ).mockResolvedValue(undefined)
  })

  it('execute + item 无 actions + title 非空 → 框架自动复制，不转发 worker', async () => {
    await handleTier2Action('execute', { item: { id: 'a', title: 'hello world' } }, callWorker)

    expect(copyAndHide).toHaveBeenCalledWith('hello world')
    expect(callWorker).not.toHaveBeenCalled()
  })

  it('title 自动 trim，前后空白被裁剪', async () => {
    await handleTier2Action('execute', { item: { id: 'a', title: '  pad  ' } }, callWorker)

    expect(copyAndHide).toHaveBeenCalledWith('pad')
  })

  it('execute + title 为空 → 回落 worker onAction', async () => {
    const item = { id: 'a', title: '' }
    await handleTier2Action('execute', { item }, callWorker)

    expect(copyAndHide).not.toHaveBeenCalled()
    expect(callWorker).toHaveBeenCalledWith('onAction', 'execute', { item })
  })

  it('execute + item 声明 actions 数组 → 穿透到 worker（扩展显式接管）', async () => {
    const item = {
      id: 'a',
      title: 'something',
      actions: [{ id: 'open-url', title: 'Open', primary: true }],
    }
    await handleTier2Action('execute', { item }, callWorker)

    expect(copyAndHide).not.toHaveBeenCalled()
    expect(callWorker).toHaveBeenCalledWith('onAction', 'execute', { item })
  })

  it('非 execute actionId 一律转发 worker', async () => {
    const item = { id: 'a', title: 'x' }
    await handleTier2Action('copy', { item }, callWorker)

    expect(copyAndHide).not.toHaveBeenCalled()
    expect(callWorker).toHaveBeenCalledWith('onAction', 'copy', { item })
  })

  it('payload 无 item → 转发 worker', async () => {
    await handleTier2Action('execute', {}, callWorker)

    expect(copyAndHide).not.toHaveBeenCalled()
    expect(callWorker).toHaveBeenCalledWith('onAction', 'execute', {})
  })

  it('await copyAndHide 完成后再返回', async () => {
    let resolved = false
    ;(
      copyAndHide as unknown as ReturnType<typeof vi.fn<(text: string) => Promise<void>>>
    ).mockImplementation(async () => {
      await new Promise((r) => setTimeout(r, 10))
      resolved = true
    })

    await handleTier2Action('execute', { item: { id: 'a', title: 'x' } }, callWorker)

    expect(resolved).toBe(true)
  })
})
