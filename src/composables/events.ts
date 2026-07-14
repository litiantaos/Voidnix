import { ref, watch, onUnmounted, onMounted, type Ref } from 'vue'
import { isFormControl } from '@/utils/dom'

export function useScroll(element: Ref<HTMLElement | undefined>) {
  const y = ref(0)
  let el: HTMLElement | undefined

  function update() {
    if (el) y.value = el.scrollTop
  }

  const stop = watch(
    element,
    (newEl, oldEl) => {
      oldEl?.removeEventListener('scroll', update)
      newEl?.addEventListener('scroll', update, { passive: true })
      el = newEl
      update()
    },
    { immediate: true },
  )

  watch(y, (newY) => {
    if (el && el.scrollTop !== newY) {
      el.scrollTop = newY
    }
  })

  onUnmounted(() => {
    el?.removeEventListener('scroll', update)
    stop()
  })

  return { y }
}

export function onKeyStroke(
  keys: string | string[],
  handler: (e: KeyboardEvent) => void,
  opts?: { ignoreFormControls?: boolean },
) {
  const keySet = new Set(Array.isArray(keys) ? keys : [keys])
  const listener = (e: KeyboardEvent) => {
    if (keySet.has(e.key)) {
      if (
        opts?.ignoreFormControls &&
        isFormControl(document.activeElement, { settingsControl: true })
      ) {
        return
      }
      handler(e)
    }
  }
  onMounted(() => document.addEventListener('keydown', listener))
  onUnmounted(() => document.removeEventListener('keydown', listener))
}
