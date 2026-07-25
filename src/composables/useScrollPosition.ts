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

  /// 删除某界面的记录：使后续 restore 归顶（区别于 reset 仅写当前 scrollTop）
  function clear(key: string) {
    saved.delete(key)
  }

  return { save, restore, reset, clear }
}
