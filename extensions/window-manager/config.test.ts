import { describe, it, expect, vi } from 'vitest'

// config.ts 模块顶层有 defineConfig + watch immediate 副作用（invoke /
// getAllWebviewWindows / plugin-store load）。mock 掉全部 Tauri 依赖，使模块在
// vitest（非 Tauri）环境安全加载，仅测 clampWidth / clampHeight / BOUNDS 纯函数。
// store 返回空值（无磁盘数据），保持 defaults。
vi.mock('@/utils/tauri', () => ({ isTauri: false }))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: () => Promise.resolve(null),
      set: () => Promise.resolve(),
      save: () => Promise.resolve(),
    }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getAllWebviewWindows: vi.fn().mockResolvedValue([]),
}))

// vi.mock 被 hoist 到 import 之前，顶层 import 安全
import { clampWidth, clampHeight, BOUNDS } from './config'

describe('window-manager BOUNDS', () => {
  it('与 Rust WIDTH_BOUNDS / HEIGHT_BOUNDS floor/cap 对齐', () => {
    // 权威源 native/mod.rs WIDTH_BOUNDS=(200,4096) / HEIGHT_BOUNDS=(200,4096)
    expect(BOUNDS.customWidth.floor).toBe(200)
    expect(BOUNDS.customWidth.cap).toBe(4096)
    expect(BOUNDS.customHeight.floor).toBe(200)
    expect(BOUNDS.customHeight.cap).toBe(4096)
  })
})

describe('clampWidth', () => {
  it('区间内原值返回', () => {
    expect(clampWidth(1000)).toBe(1000)
    expect(clampWidth(300)).toBe(300)
  })

  it('低于 floor 钳到 floor', () => {
    expect(clampWidth(100)).toBe(200)
    expect(clampWidth(0)).toBe(200)
    expect(clampWidth(-50)).toBe(200)
  })

  it('高于 cap 钳到 cap', () => {
    expect(clampWidth(5000)).toBe(4096)
    expect(clampWidth(99999)).toBe(4096)
  })
})

describe('clampHeight', () => {
  it('区间内原值返回', () => {
    expect(clampHeight(800)).toBe(800)
  })

  it('低于 floor 钳到 floor', () => {
    expect(clampHeight(100)).toBe(200)
  })

  it('高于 cap 钳到 cap', () => {
    expect(clampHeight(5000)).toBe(4096)
  })
})
