import { describe, it, expect, vi } from 'vitest'

// http_get 恒失败：验证网络失败时用户看到显式报错行而非静默空结果
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockRejectedValue(new Error('network down')),
}))

import ext from './index'
import type { SearchContext } from '@/runtime/types'

const dynamic = ext.search!.dynamic

function ctx(extensionMode: boolean): SearchContext {
  return { signal: new AbortController().signal, extensionMode }
}

describe('currency 网络失败呈现', () => {
  it('货币查询失败返回显式报错行（全局模式）', async () => {
    const r = await dynamic('100 usd', ctx(false))
    expect(r).toHaveLength(1)
    expect(r[0].id).toBe('rates-err')
    expect(r[0].data?.kind).toBe('extension')
    // 回车激活 currency 进入重试流（防静默藏窗）
    expect(r[0].data?.extId).toBe('currency')
  })

  it('非货币查询失败不报错（避免离线刷屏）', async () => {
    const r = await dynamic('hello', ctx(false))
    expect(r).toHaveLength(0)
  })

  it('扩展内空 query（参考汇率）失败返回报错行', async () => {
    const r = await dynamic('', ctx(true))
    expect(r).toHaveLength(1)
    expect(r[0].id).toBe('rates-err')
  })

  it('全局空 query 不触发网络', async () => {
    const r = await dynamic('', ctx(false))
    expect(r).toHaveLength(0)
  })
})
