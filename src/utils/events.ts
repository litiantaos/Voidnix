import { ref, watch, onUnmounted, onMounted, type Ref } from 'vue'

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

  onUnmounted(() => {
    el?.removeEventListener('scroll', update)
    stop()
  })

  return { y }
}

export function onKeyStroke(keys: string | string[], handler: (e: KeyboardEvent) => void) {
  const keySet = new Set(Array.isArray(keys) ? keys : [keys])
  const listener = (e: KeyboardEvent) => {
    if (keySet.has(e.key)) handler(e)
  }
  onMounted(() => document.addEventListener('keydown', listener))
  onUnmounted(() => document.removeEventListener('keydown', listener))
}
