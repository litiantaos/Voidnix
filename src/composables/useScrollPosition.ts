import { type Ref, nextTick } from 'vue'

export function useScrollPosition(scrollTop: Ref<number>) {
  const saved = new Map<string, number>()

  function save(key: string) {
    saved.set(key, scrollTop.value)
  }

  function restore(key: string) {
    nextTick(() => {
      scrollTop.value = saved.get(key) ?? 0
    })
  }

  function reset() {
    nextTick(() => {
      scrollTop.value = 0
    })
  }

  return { save, restore, reset }
}
