import { describe, it, expect } from 'vitest'
import { uuidv4, nanoId } from './logic'

describe('uuidv4', () => {
  it('generates valid UUID v4 format', () => {
    const id = uuidv4()
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
  })

  it('generates unique values', () => {
    const ids = new Set(Array.from({ length: 100 }, () => uuidv4()))
    expect(ids.size).toBe(100)
  })
})

describe('nanoId', () => {
  it('generates default 21-character ID', () => {
    expect(nanoId().length).toBe(21)
  })

  it('respects custom size', () => {
    expect(nanoId(10).length).toBe(10)
    expect(nanoId(32).length).toBe(32)
  })

  it('generates unique values', () => {
    const ids = new Set(Array.from({ length: 100 }, () => nanoId()))
    expect(ids.size).toBe(100)
  })

  it('uses URL-safe characters only', () => {
    const id = nanoId(100)
    expect(id).toMatch(/^[A-Za-z0-9_-]+$/)
  })
})
