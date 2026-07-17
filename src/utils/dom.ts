const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled]):not([tabindex="-1"])',
  'input:not([disabled]):not([tabindex="-1"])',
  'select:not([disabled]):not([tabindex="-1"])',
  'textarea:not([disabled]):not([tabindex="-1"])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ')

export function getFocusableElements(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll(FOCUSABLE_SELECTOR))
}

export function isComposing(e: KeyboardEvent): boolean {
  return !!(e.isComposing || e.keyCode === 229)
}

export function isFormControl(
  el: Element | null | undefined,
  extraChecks?: { settingsControl?: boolean },
): boolean {
  if (!el) return false
  const tag = el.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (el.hasAttribute('contenteditable')) return true
  // P4-fe3：统一走 data attr（BaseSelect 已设 data-settings-control），
  // 不再耦合具体 class 名（旧 el.classList.contains('custom-select') 脆弱）
  if (extraChecks?.settingsControl && el.hasAttribute('data-settings-control')) return true
  return false
}

/**
 * 程序化键盘聚焦。
 * 裸 focus() 不匹配 :focus-visible；{ focusVisible:true } 在部分 WK 无效，
 * 故同步挂 is-keyboard-focus 作样式兜底，blur 时卸下。
 */
export function focusFromKeyboard(el: HTMLElement) {
  document.querySelectorAll('.is-keyboard-focus').forEach((node) => {
    node.classList.remove('is-keyboard-focus')
  })
  el.focus({ focusVisible: true })
  el.classList.add('is-keyboard-focus')
  el.addEventListener(
    'blur',
    () => {
      el.classList.remove('is-keyboard-focus')
    },
    { once: true },
  )
}

export function cycleFocus(focusable: HTMLElement[], e: KeyboardEvent) {
  if (focusable.length === 0) return
  e.preventDefault()
  const active = document.activeElement as HTMLElement
  let idx = focusable.indexOf(active)
  if (e.shiftKey) {
    idx = idx <= 0 ? focusable.length - 1 : idx - 1
  } else {
    idx = idx < 0 || idx >= focusable.length - 1 ? 0 : idx + 1
  }
  focusFromKeyboard(focusable[idx])
}

export function trapFocus(focusable: HTMLElement[], e: KeyboardEvent) {
  if (focusable.length === 0) return
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (e.shiftKey) {
    if (document.activeElement === first) {
      e.preventDefault()
      focusFromKeyboard(last)
    }
  } else {
    if (document.activeElement === last) {
      e.preventDefault()
      focusFromKeyboard(first)
    }
  }
}

export function wrapIndex(current: number, length: number, direction: 'up' | 'down'): number {
  if (length === 0) return 0
  if (direction === 'down') {
    return current >= length - 1 ? 0 : current + 1
  }
  return current <= 0 ? length - 1 : current - 1
}
