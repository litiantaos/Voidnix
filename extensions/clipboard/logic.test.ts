import { describe, it, expect } from 'vitest'
import { matchText, filterByQuery, filterByType } from './logic'
import type { ClipboardItem } from './index'
import './locales'

function makeItem(partial: Partial<ClipboardItem>): ClipboardItem {
  return {
    id: 'x',
    content: 'text',
    content_type: 'text',
    source_app: 'app',
    created_at: '',
    is_favorite: false,
    score: 0,
    file_size: null,
    image_width: null,
    image_height: null,
    ...partial,
  }
}

describe('matchText', () => {
  it('文本类型返回原文', () => {
    expect(matchText(makeItem({ content: 'hello world' }))).toBe('hello world')
  })

  it('图片类型返回语义占位（便于按"图片"检索）', () => {
    expect(matchText(makeItem({ content_type: 'image', content: 'data:...' }))).toBe('图片 image')
  })

  it('文件类型拼接 file 关键词', () => {
    expect(matchText(makeItem({ content_type: 'file', content: '/a/b.txt' }))).toBe(
      '文件 file /a/b.txt',
    )
  })
})

describe('filterByQuery', () => {
  const items = [
    makeItem({ id: '1', content: 'hello world' }),
    makeItem({ id: '2', content: 'goodbye friend' }),
    makeItem({ id: '3', content: 'say hello again' }),
  ]

  it('空 query 原样返回', () => {
    expect(filterByQuery(items, '')).toEqual(items)
    expect(filterByQuery(items, '   ')).toEqual(items)
  })

  it('按 query 过滤并保留 score > 0 项', () => {
    const out = filterByQuery(items, 'hello')
    expect(out.map((i) => i.id).sort()).toEqual(['1', '3'])
    expect(out.every((i) => i.score > 0)).toBe(true)
  })

  it('结果按 score 降序', () => {
    const out = filterByQuery(items, 'hello')
    for (let i = 1; i < out.length; i++) {
      expect(out[i - 1].score).toBeGreaterThanOrEqual(out[i].score)
    }
  })

  it('无匹配返回空数组', () => {
    expect(filterByQuery(items, 'zzznotmatch')).toEqual([])
  })

  it('不修改入参数组', () => {
    const snapshot = items.map((i) => ({ ...i }))
    filterByQuery(items, 'hello')
    expect(items).toEqual(snapshot)
  })
})

describe('filterByType', () => {
  const items = [
    makeItem({ id: '1', content_type: 'text', content: 'a' }),
    makeItem({ id: '2', content_type: 'image', content: 'img' }),
    makeItem({ id: '3', content_type: 'file', content: '/x.txt' }),
    makeItem({ id: '4', content_type: 'text', content: 'b' }),
  ]

  it("'all' 原样返回", () => {
    expect(filterByType(items, 'all')).toBe(items)
  })

  it('按 text 过滤', () => {
    expect(filterByType(items, 'text').map((i) => i.id)).toEqual(['1', '4'])
  })

  it('按 image 过滤', () => {
    expect(filterByType(items, 'image').map((i) => i.id)).toEqual(['2'])
  })

  it('按 file 过滤', () => {
    expect(filterByType(items, 'file').map((i) => i.id)).toEqual(['3'])
  })

  it('不修改入参数组', () => {
    const snapshot = items.map((i) => ({ ...i }))
    filterByType(items, 'image')
    expect(items).toEqual(snapshot)
  })
})
