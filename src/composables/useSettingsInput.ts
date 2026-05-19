import { ref } from 'vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'

export interface SettingItem {
  id: string
  title: string
  type: 'shortcut' | 'select' | 'input' | 'button'
  value: string | number
  subtitle?: string
  icon?: string
  options?: ({ label: string; value: string | number } | { label: string; options: { label: string; value: string | number }[] })[]
  inputType?: 'text' | 'password'
  placeholder?: string
  action?: () => void
  update?: (val: string | number) => void
}

export function useSettingsInput() {
  const selectRefs = ref<Record<string, InstanceType<typeof BaseSelect>>>({})
  const inputRefs = ref<Record<string, InstanceType<typeof BaseInput>>>({})
  const shortcutRefs = ref<Record<string, InstanceType<typeof ShortcutInput>>>({})
  const passwordVisibility = ref<Record<string, boolean>>({})
  const editingValue = ref<Record<string, string>>({})
  const editingOriginal = ref<Record<string, string>>({})

  function setSelectRef(id: string, el: unknown) {
    if (el) selectRefs.value[id] = el as InstanceType<typeof BaseSelect>
  }

  function setInputRef(id: string, el: unknown) {
    if (el) inputRefs.value[id] = el as InstanceType<typeof BaseInput>
  }

  function setShortcutRef(id: string, el: unknown) {
    if (el) shortcutRefs.value[id] = el as InstanceType<typeof ShortcutInput>
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
   * 处理设置项的执行逻辑（回车/双击）。
   * @returns true 表示已消费该事件（标准类型），false 表示未识别需调用方自行处理
   */
  function handleExecute(
    item: SettingItem,
    e?: KeyboardEvent | MouseEvent,
  ): boolean {
    if (e) e.preventDefault()

    if (item.type === 'button' && item.action) {
      setTimeout(() => item.action!(), 150)
      return true
    } else if (item.type === 'shortcut') {
      const ref = shortcutRefs.value[`si-${item.id}`]
      if (ref) {
        ref.focus()
        ref.startRecording()
      }
      return true
    } else if (item.type === 'select') {
      const ref = selectRefs.value[`si-${item.id}`]
      if (ref) {
        ref.focus()
        ref.toggleOpen()
      }
      return true
    } else if (item.type === 'input') {
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
    selectRefs,
    inputRefs,
    shortcutRefs,
    passwordVisibility,
    editingValue,
    editingOriginal,
    setSelectRef,
    setInputRef,
    setShortcutRef,
    togglePasswordVisibility,
    startEdit,
    onEditInput,
    commitEdit,
    revertEdit,
    handleExecute,
  }
}
