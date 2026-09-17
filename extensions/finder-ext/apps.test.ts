import { describe, expect, it, vi } from 'vitest'

// apps.ts 模块顶层注册失效事件监听，mock 掉 Tauri API（buildCandidates 为纯函数不受影响）
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))

import { buildCandidates, type AppEntry } from './apps'

const A = (id: string, name: string, path = `/Applications/${name}.app`): AppEntry => ({
  id,
  name,
  path,
})

describe('buildCandidates', () => {
  const installed = [
    A('1', 'VS Code'),
    A('2', 'Zed'),
    A('3', 'Preview'),
    A('4', 'Safari'),
    A('5', 'TextEdit'),
  ]
  const ls = [
    '/Applications/TextEdit.app', // LS 偏好序：默认应用在前
    '/Applications/VS Code.app',
    '/Applications/Zed.app',
    '/Applications/NotIndexed.app', // join 不上（缓存目录脏项）→ 剔除
    '/Applications/Safari.app',
    '/Applications/Preview.app',
  ]

  it('MRU 置顶 + LS 偏好序补足，join 不上的剔除', () => {
    const items = buildCandidates(installed, ls, ['/Applications/Preview.app'])
    expect(items.map((i) => i.name)).toEqual(['Preview', 'TextEdit', 'VS Code', 'Zed', 'Safari'])
  })

  it('去重：MRU 与 LS 重叠不重复出现', () => {
    const items = buildCandidates(installed, ls, ['/Applications/TextEdit.app'])
    expect(items[0].name).toBe('TextEdit')
    expect(items.filter((i) => i.name === 'TextEdit')).toHaveLength(1)
  })

  it('cap 上限截断', () => {
    const items = buildCandidates(installed, ls, [], 3)
    expect(items.map((i) => i.name)).toEqual(['TextEdit', 'VS Code', 'Zed'])
  })

  it('无选区（LS 空）时仅 MRU；已卸载 MRU 剔除', () => {
    expect(buildCandidates(installed, [], ['/Applications/Gone.app'])).toEqual([])
    const items = buildCandidates(installed, [], ['/Applications/Zed.app'])
    expect(items.map((i) => i.name)).toEqual(['Zed'])
  })
})
