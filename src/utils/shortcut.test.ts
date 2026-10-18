import { describe, it, expect } from 'vitest'
import { shortcutKeyFromEvent } from './shortcut'

function ev(key: string, code: string) {
  return { key, code }
}

describe('shortcutKeyFromEvent', () => {
  it('Option 合成的特殊字符还原为物理键位', () => {
    // macOS：⌥C → ç、⌥E → Dead、⌥1 → ¡、⌥- → –、⌥Space → 不换行空格
    expect(shortcutKeyFromEvent(ev('ç', 'KeyC'))).toBe('C')
    expect(shortcutKeyFromEvent(ev('Dead', 'KeyE'))).toBe('E')
    expect(shortcutKeyFromEvent(ev('¡', 'Digit1'))).toBe('1')
    expect(shortcutKeyFromEvent(ev('–', 'Minus'))).toBe('-')
    expect(shortcutKeyFromEvent(ev('\u00a0', 'Space'))).toBe('Space')
  })

  it('Shift 改变字符的符号键同样按物理键位落位', () => {
    // ⇧; → :、⇧= → +：字符形式含 + 会破坏快捷键串分隔，且 parser 不识别
    expect(shortcutKeyFromEvent(ev(':', 'Semicolon'))).toBe(';')
    expect(shortcutKeyFromEvent(ev('+', 'Equal'))).toBe('=')
  })

  it('导航键 / 功能键不受 Option 影响，沿用 e.key', () => {
    expect(shortcutKeyFromEvent(ev('ArrowUp', 'ArrowUp'))).toBe('ArrowUp')
    expect(shortcutKeyFromEvent(ev('F1', 'F1'))).toBe('F1')
  })

  it('小键盘按物理键位区分主键区（Code 名与 Rust parser 同源）', () => {
    expect(shortcutKeyFromEvent(ev('1', 'Numpad1'))).toBe('Numpad1')
    expect(shortcutKeyFromEvent(ev('+', 'NumpadAdd'))).toBe('NumpadAdd')
  })

  it('无 code 时回退字符本身，单字母大写', () => {
    expect(shortcutKeyFromEvent(ev('c', ''))).toBe('C')
    expect(shortcutKeyFromEvent(ev('[', ''))).toBe('[')
  })
})
