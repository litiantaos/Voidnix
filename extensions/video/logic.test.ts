import { describe, expect, it } from 'vitest'
import {
  actionLabel,
  buildOutputName,
  displayPath,
  fileNameFromPath,
  formatBytes,
  formatDuration,
  formatMetaLine,
  sanitizeStem,
  stemFromPath,
} from './logic'

describe('video logic', () => {
  it('action labels', () => {
    expect(actionLabel('compress', 'mp4')).toBe('compressed')
    expect(actionLabel('extract-audio', 'm4a')).toBe('audio')
    expect(actionLabel('convert', 'gif')).toBe('converted')
    expect(actionLabel('convert', 'webm')).toBe('converted')
  })

  it('buildOutputName', () => {
    expect(buildOutputName('clip', 'compress', 'mp4')).toBe('clip.compressed.mp4')
    expect(buildOutputName('a/b', 'convert', 'gif')).toBe('a_b.converted.gif')
  })

  it('sanitizeStem strips controls and separators', () => {
    expect(sanitizeStem('a/b\\c')).toBe('a_b_c')
  })

  it('path helpers', () => {
    expect(fileNameFromPath('/Users/me/Movies/a.mp4')).toBe('a.mp4')
    expect(stemFromPath('/Users/me/Movies/a.mp4')).toBe('a')
    expect(displayPath('/Users/me/Movies/a.mp4')).toBe('~/Movies/a.mp4')
  })

  it('formatDuration / formatBytes', () => {
    expect(formatDuration(65)).toBe('1:05')
    expect(formatDuration(3661)).toBe('1:01:01')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB')
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
})
