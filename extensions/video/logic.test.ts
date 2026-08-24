import { describe, expect, it } from 'vitest'
import {
  displayPath,
  fileNameFromPath,
  formatBytes,
  formatDuration,
  formatMetaLine,
  summarizeMetas,
} from './logic'

describe('video logic', () => {
  it('path helpers', () => {
    expect(fileNameFromPath('/Users/me/Movies/a.mp4')).toBe('a.mp4')
    expect(displayPath('/Users/me/Movies/a.mp4')).toBe('~/Movies/a.mp4')
  })

  it('formatDuration / formatBytes', () => {
    expect(formatDuration(65)).toBe('1:05')
    expect(formatDuration(3661)).toBe('1:01:01')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5 MB')
    expect(formatBytes(0)).toBe('—')
  })

  it('formatMetaLine', () => {
    const line = formatMetaLine({
      durationSecs: 90,
      width: 1920,
      height: 1080,
      videoCodec: 'h264',
      sizeBytes: 10 * 1024 * 1024,
    })
    expect(line).toContain('1920×1080')
    expect(line).toContain('1:30')
    expect(line).toContain('h264')
  })

  it('summarizeMetas', () => {
    const mk = (durationSecs: number, sizeBytes: number) => ({
      path: '',
      durationSecs,
      width: 1920,
      height: 1080,
      videoCodec: 'h264',
      audioCodec: 'aac',
      sizeBytes,
      container: 'mp4',
    })
    // 汇总已探测项，跳过 null
    expect(summarizeMetas([mk(60, 1024 * 1024), null, mk(60, 1024 * 1024)])).toBe('2:00 · 2 MB')
    // 全未探测 → 空串
    expect(summarizeMetas([null, null])).toBe('')
  })
})
