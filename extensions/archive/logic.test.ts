import { describe, expect, it } from 'vitest'
import {
  baseName,
  defaultArchiveName,
  formatBytes,
  isArchivePath,
  normalizeOutputName,
  type CompressFormat,
} from './logic'

describe('isArchivePath', () => {
  it('识别常见归档扩展名', () => {
    expect(isArchivePath('/a/b/pkg.zip')).toBe(true)
    expect(isArchivePath('/a/b/pkg.tar.gz')).toBe(true)
    expect(isArchivePath('/a/b/x.7z')).toBe(true)
    expect(isArchivePath('/a/b/y.RAR')).toBe(true)
    expect(isArchivePath('/a/b/b.tgz')).toBe(true)
    expect(isArchivePath('/a/b/书.漫画.cbz'.replace('.cbz', '.epub'))).toBe(true)
    expect(isArchivePath('/a/b/p.tar.zst')).toBe(true)
    expect(isArchivePath('/a/b/ubuntu.iso')).toBe(true)
  })

  it('非归档路径不误判', () => {
    expect(isArchivePath('/a/b/notes.txt')).toBe(false)
    expect(isArchivePath('/a/b/data.gzx')).toBe(false)
    expect(isArchivePath('/a/b/noext')).toBe(false)
    expect(isArchivePath('/a/b/.DS_Store')).toBe(false)
    // 裸压缩流不算（Rust detect 不认，仅 .tar.* 双扩展可解）——与 finder-ext 镜像同语义
    expect(isArchivePath('/a/b/data.gz')).toBe(false)
    expect(isArchivePath('/a/b/db.dump.xz')).toBe(false)
  })
})

describe('defaultArchiveName', () => {
  it('单输入取去扩展名主干', () => {
    expect(defaultArchiveName(['/Users/x/项目资料'], 'zip' as CompressFormat)).toBe('项目资料.zip')
    expect(defaultArchiveName(['/Users/x/a.tar.gz'], 'zip' as CompressFormat)).toBe('a.zip')
    expect(defaultArchiveName(['/Users/x/笔记'], 'sevenZ' as CompressFormat)).toBe('笔记.7z')
  })

  it('多输入用通用名', () => {
    expect(defaultArchiveName(['/a.txt', '/b.txt'], 'zip' as CompressFormat)).toBe('archive.zip')
  })
})

describe('normalizeOutputName', () => {
  it('按当前格式统一扩展名', () => {
    expect(normalizeOutputName('报告.zip', 'sevenZ')).toBe('报告.7z')
    expect(normalizeOutputName('报告.7z', 'zip')).toBe('报告.zip')
    expect(normalizeOutputName('a.tar.gz', 'zip')).toBe('a.zip')
    expect(normalizeOutputName('报告', 'zip')).toBe('报告.zip')
    expect(normalizeOutputName(' 报告.ZIP ', 'zip')).toBe('报告.zip')
    expect(normalizeOutputName('.zip', 'zip')).toBe('archive.zip')
  })
})

describe('formatBytes / baseName', () => {
  it('人性化字节', () => {
    expect(formatBytes(null)).toBe('—')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2048)).toBe('2.0 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB')
  })

  it('baseName 去目录', () => {
    expect(baseName('/a/b/c.zip')).toBe('c.zip')
  })
})
