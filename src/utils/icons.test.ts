import { describe, it, expect } from 'vitest'
import { getFileIcon } from './icons'
import type { SearchResult } from '@/runtime/types'

function makeResult(
  overrides: Omit<Partial<SearchResult>, 'data'> & { data?: Record<string, unknown> },
): SearchResult {
  const { data, ...rest } = overrides
  return {
    id: 'test',
    title: rest.title ?? 'file',
    module: 'test',
    ...rest,
    data: { kind: 'file', ...data },
  } as SearchResult
}

describe('getFileIcon', () => {
  describe('文件夹类型', () => {
    it('.app 文件夹', () => {
      const r = makeResult({
        title: 'Safari.app',
        data: { kind: 'folder', path: '/Applications/Safari.app' },
      })
      expect(getFileIcon(r)).toEqual({ icon: 'i-ri-apps-2-fill', color: 'text-blue-500' })
    })

    it('包含 .app 的路径', () => {
      const r = makeResult({
        title: 'Contents',
        data: { kind: 'folder', path: '/Safari.app/Contents' },
      })
      expect(getFileIcon(r).icon).toBe('i-ri-apps-2-fill')
    })

    it('.git 目录', () => {
      const r = makeResult({ title: '.git', data: { kind: 'folder', path: '/project/.git' } })
      expect(getFileIcon(r)).toEqual({ icon: 'i-ri-git-branch-fill', color: 'text-orange-500' })
    })

    it('普通文件夹', () => {
      const r = makeResult({
        title: 'Documents',
        data: { kind: 'folder', path: '/Users/test/Documents' },
      })
      expect(getFileIcon(r)).toEqual({ icon: 'i-ri-folder-fill', color: 'text-zinc-500' })
    })
  })

  describe('文件扩展名', () => {
    it('图片文件', () => {
      const r = makeResult({ data: { path: '/img/photo.png' } })
      expect(getFileIcon(r).icon).toBe('i-ri-image-fill')
    })

    it('代码文件', () => {
      for (const ext of ['ts', 'js', 'vue', 'rs', 'py']) {
        const r = makeResult({ data: { path: `/file.${ext}` } })
        expect(getFileIcon(r).icon).toBe('i-ri-code-fill')
      }
    })

    it('PDF 文件', () => {
      const r = makeResult({ data: { path: '/doc/file.pdf' } })
      expect(getFileIcon(r)).toEqual({ icon: 'i-ri-file-pdf-fill', color: 'text-red-500' })
    })

    it('压缩文件', () => {
      for (const ext of ['zip', 'tar', 'gz', '7z']) {
        const r = makeResult({ data: { path: `/archive.${ext}` } })
        expect(getFileIcon(r).icon).toBe('i-ri-file-zip-fill')
      }
    })

    it('音频文件', () => {
      const r = makeResult({ data: { path: '/music/song.mp3' } })
      expect(getFileIcon(r).icon).toBe('i-ri-music-fill')
    })

    it('视频文件', () => {
      const r = makeResult({ data: { path: '/video/movie.mp4' } })
      expect(getFileIcon(r).icon).toBe('i-ri-video-fill')
    })

    it('未知扩展名返回默认图标', () => {
      const r = makeResult({ data: { path: '/file.xyz' } })
      expect(getFileIcon(r)).toEqual({ icon: 'i-ri-file-fill', color: 'text-muted' })
    })

    it('无路径信息返回默认图标', () => {
      const r = makeResult({})
      expect(getFileIcon(r).icon).toBe('i-ri-file-fill')
    })
  })
})
