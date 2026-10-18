/**
 * 快捷键键位 token（`Alt+C` 里的 `C`）。
 *
 * 不能直接用 `e.key`：macOS 按住 Option 时按键会落成合成字符——`⌥C` 是 `ç`、
 * `⌥E` 是死键 `Dead`、`⌥-` 是 `–`、`⌥Space` 是不换行空格——这些字符既不是
 * 用户按下的键，也无法被 Rust 侧 `Shortcut::from_str` 解析注册（快捷键录入
 * 直接失效）。全局快捷键按物理键位（global-hotkey `Code`）生效，故可打印键
 * 一律以 `e.code` 为准；导航键 / 功能键不受 Option 影响，沿用 `e.key`。
 */
const CODE_KEYS: Record<string, string> = {
  Backquote: '`',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
  Space: 'Space',
  Numpad0: 'Numpad0',
  Numpad1: 'Numpad1',
  Numpad2: 'Numpad2',
  Numpad3: 'Numpad3',
  Numpad4: 'Numpad4',
  Numpad5: 'Numpad5',
  Numpad6: 'Numpad6',
  Numpad7: 'Numpad7',
  Numpad8: 'Numpad8',
  Numpad9: 'Numpad9',
  NumpadAdd: 'NumpadAdd',
  NumpadSubtract: 'NumpadSubtract',
  NumpadMultiply: 'NumpadMultiply',
  NumpadDivide: 'NumpadDivide',
  NumpadDecimal: 'NumpadDecimal',
  NumpadEqual: 'NumpadEqual',
  NumpadEnter: 'NumpadEnter',
}

export function shortcutKeyFromEvent(e: Pick<KeyboardEvent, 'key' | 'code'>): string {
  const letter = /^Key([A-Z])$/.exec(e.code)
  if (letter) return letter[1]
  const digit = /^Digit([0-9])$/.exec(e.code)
  if (digit) return digit[1]
  const mapped = CODE_KEYS[e.code]
  if (mapped) return mapped
  // 合成事件 / 无 code 环境回退字符本身；单字母统一大写（与 `Alt+C` 写法一致）
  return /^[a-zA-Z]$/.test(e.key) ? e.key.toUpperCase() : e.key
}
