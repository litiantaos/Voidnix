import { describe, it, expect } from 'vitest'
import {
  IMAGE_EXTENSIONS,
  fileNameFromPath,
  displayPath,
  formatBytes,
  buildOutputPath,
} from './logic'

describe('image/logic', () => {
  describe('fileNameFromPath', () => {
    it('从完整路径提取文件名', () => {
      expect(fileNameFromPath('/Users/test/photo.jpg')).toBe('photo.jpg')
      expect(fileNameFromPath('photo.jpg')).toBe('photo.jpg')
    })
  })

  describe('displayPath', () => {
    it('home 路径缩写为 ~', () => {
      expect(displayPath('/Users/john/Desktop/img.png')).toBe('~/Desktop/img.png')
    })
    it('非 home 路径原样', () => {
      expect(displayPath('/tmp/test.png')).toBe('/tmp/test.png')
    })
  })

  describe('formatBytes', () => {
    it('空值显示 —', () => {
      expect(formatBytes(0)).toBe('—')
      expect(formatBytes(-1)).toBe('—')
    })
    it('正常值格式化', () => {
      expect(formatBytes(1024)).toBe('1 KB')
      expect(formatBytes(1048576)).toBe('1 MB')
    })
  })

  describe('IMAGE_EXTENSIONS', () => {
    it('包含常见格式', () => {
      expect(IMAGE_EXTENSIONS).toContain('png')
      expect(IMAGE_EXTENSIONS).toContain('jpg')
      expect(IMAGE_EXTENSIONS).toContain('heic')
      expect(IMAGE_EXTENSIONS).toContain('webp')
    })
  })

  describe('buildOutputPath', () => {
    it('默认 nobg 后缀', () => {
      expect(buildOutputPath('/tmp/photo.jpg')).toBe('/tmp/photo.nobg.png')
    })
    it('指定输出目录', () => {
      expect(buildOutputPath('/tmp/photo.jpg', '/output')).toBe('/output/photo.nobg.png')
    })
    it('自定义后缀', () => {
      expect(buildOutputPath('/tmp/photo.jpg', undefined, 'stitch')).toBe('/tmp/photo.stitch.png')
    })
    it('无目录的文件名', () => {
      expect(buildOutputPath('photo.jpg')).toBe('photo.nobg.png')
    })
    it('净化控制字符', () => {
      const result = buildOutputPath('/tmp/bad\nname.jpg')
      expect(result).not.toContain('\n')
    })
  })
})
