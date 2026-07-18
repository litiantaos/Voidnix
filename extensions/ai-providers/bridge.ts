import { ref } from 'vue'

/** Actions（搜索栏）→ View 打开「添加提供商」弹窗。 */
export const createProviderTick = ref(0)

export function requestCreateProvider() {
  createProviderTick.value++
}
