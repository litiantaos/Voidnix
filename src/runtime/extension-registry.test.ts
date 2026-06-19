import { describe, it, expect } from 'vitest'
import { defineExtension, getAllExtensions, getExtension } from './extension-registry'
import type { Extension } from './types'

function makeExt(id: string): Extension {
  return defineExtension({
    meta: { id, name: id, icon: 'i-ri-test-line', order: 1 },
  })
}

describe('extension-registry', () => {
  it('defineExtension 注册并返回原对象', () => {
    const ext = makeExt('reg-return')
    expect(ext.meta.id).toBe('reg-return')
    expect(getAllExtensions()).toContain(ext)
  })

  it('getExtension 按 id 查找', () => {
    makeExt('reg-find')
    expect(getExtension('reg-find')?.meta.id).toBe('reg-find')
    expect(getExtension('reg-missing')).toBeUndefined()
  })

  it('getAllExtensions 保持注册顺序', () => {
    const before = getAllExtensions().length
    makeExt('reg-order-a')
    makeExt('reg-order-b')
    const all = getAllExtensions()
    expect(all.length).toBe(before + 2)
    const ia = all.findIndex((e) => e.meta.id === 'reg-order-a')
    const ib = all.findIndex((e) => e.meta.id === 'reg-order-b')
    expect(ia).toBeLessThan(ib)
  })
})
