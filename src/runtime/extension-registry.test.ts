import { describe, it, expect, afterEach } from 'vitest'
import {
  defineExtension,
  getAllExtensions,
  getExtension,
  _resetForTest,
} from './extension-registry'
import type { Extension } from './types'

function makeExt(id: string): Extension {
  return defineExtension({
    meta: { id, name: id, icon: 'i-ri-test-line', order: 1 },
  })
}

afterEach(() => _resetForTest())

describe('extension-registry', () => {
  it('defineExtension 注册并返回原对象', () => {
    const ext = makeExt('reg-return')
    expect(ext.meta.id).toBe('reg-return')
    expect(getAllExtensions()).toEqual([ext])
  })

  it('getExtension 按 id 查找', () => {
    makeExt('reg-find')
    expect(getExtension('reg-find')?.meta.id).toBe('reg-find')
    expect(getExtension('reg-missing')).toBeUndefined()
  })

  it('getAllExtensions 保持注册顺序', () => {
    makeExt('reg-order-a')
    makeExt('reg-order-b')
    const all = getAllExtensions()
    expect(all.length).toBe(2)
    const ia = all.findIndex((e) => e.meta.id === 'reg-order-a')
    const ib = all.findIndex((e) => e.meta.id === 'reg-order-b')
    expect(ia).toBeLessThan(ib)
  })
})
