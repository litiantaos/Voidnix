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
  if (extraChecks?.settingsControl && el.hasAttribute('data-settings-control')) return true
  if (extraChecks?.settingsControl && el.classList.contains('custom-select')) return true
  return false
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
  focusable[idx].focus()
}

export function trapFocus(focusable: HTMLElement[], e: KeyboardEvent) {
  if (focusable.length === 0) return
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (e.shiftKey) {
    if (document.activeElement === first) {
      e.preventDefault()
      last.focus()
    }
  } else {
    if (document.activeElement === last) {
      e.preventDefault()
      first.focus()
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
