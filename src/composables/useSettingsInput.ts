import { ref } from 'vue'
import BaseInput from '@/components/ui/BaseInput.vue'

export interface SettingItem {
  id: string
  title: string
  type: 'shortcut' | 'select' | 'input' | 'button'
  value: string | number
  subtitle?: string
  icon?: string
  group?: string
  options?: ({ label: string; value: string | number } | { label: string; options: { label: string; value: string | number }[] })[]
  inputType?: 'text' | 'password'
  placeholder?: string
  action?: () => void
  update?: (val: string | number) => void
}

/**
 * 设置项的交互逻辑。
 *
 * Enter 键由 BaseList 通过 DOM querySelector 统一处理（查找
 * [data-settings-control][tabindex="0"] 元素并执行 focus() + click()）。
 * 这里仅处理需要额外状态管理的 input 编辑生命周期和 button 的延迟触发。
 */
export function useSettingsInput() {
  const inputRefs = ref<Record<string, InstanceType<typeof BaseInput>>>({})
  const passwordVisibility = ref<Record<string, boolean>>({})
  const editingValue = ref<Record<string, string>>({})
  const editingOriginal = ref<Record<string, string>>({})

  function setInputRef(id: string, el: unknown) {
    if (el) inputRefs.value[id] = el as InstanceType<typeof BaseInput>
  }

  function togglePasswordVisibility(id: string) {
    passwordVisibility.value[id] = !passwordVisibility.value[id]
  }

  function startEdit(item: SettingItem) {
    editingOriginal.value[item.id] = String(item.value ?? '')
    editingValue.value[item.id] = String(item.value ?? '')
  }

  function onEditInput(item: SettingItem, val: string) {
    editingValue.value[item.id] = val
  }

  function commitEdit(item: SettingItem) {
    if (!(item.id in editingValue.value)) return
    const val = editingValue.value[item.id]
    const orig = editingOriginal.value[item.id]
    delete editingValue.value[item.id]
    delete editingOriginal.value[item.id]
    if (val !== orig && item.update) {
      item.update(val)
      item.value = val
    }
  }

  function revertEdit(item: SettingItem) {
    if (item.id in editingOriginal.value) {
      if (item.update) item.update(editingOriginal.value[item.id])
      delete editingValue.value[item.id]
      delete editingOriginal.value[item.id]
      inputRefs.value[`si-${item.id}`]?.blur()
    }
  }

  /**
   * 处理设置项的执行逻辑（回车 / 双击）。
   *
   * Enter 键的控件聚焦由 BaseList 内置机制统一处理（DOM querySelector
   * [data-settings-control][tabindex="0"]），这里不再重复聚焦逻辑。
   *
   * @returns true 表示已消费该事件，false 表示未识别需调用方自行处理
   */
  function handleExecute(
    item: SettingItem,
    e?: KeyboardEvent | MouseEvent,
  ): boolean {
    if (e) e.preventDefault()

    if (item.type === 'button' && item.action) {
      setTimeout(() => item.action!(), 150)
      return true
    }

    if (item.type === 'input') {
      if (item.id in editingValue.value) {
        commitEdit(item)
        inputRefs.value[`si-${item.id}`]?.blur()
      } else {
        startEdit(item)
        inputRefs.value[`si-${item.id}`]?.focus()
      }
      return true
    }

    return false
  }

  return {
    inputRefs,
    passwordVisibility,
    editingValue,
    editingOriginal,
    setInputRef,
    togglePasswordVisibility,
    startEdit,
    onEditInput,
    commitEdit,
    revertEdit,
    handleExecute,
  }
}
