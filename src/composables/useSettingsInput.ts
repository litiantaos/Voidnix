import { ref, onMounted, onActivated, onDeactivated, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import BaseInput from '@/components/ui/BaseInput.vue'

export interface SettingItem {
  id: string
  title: string
  type: 'shortcut' | 'select' | 'input' | 'button'
  value: string | number
  subtitle?: string
  icon?: string
  group?: string
  options?: (
    | { label: string; value: string | number }
    | { label: string; options: { label: string; value: string | number }[] }
  )[]
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
 *
 * Escape：设置型视图的表单控件聚焦时，esc 先失焦而非退出视图（capture 阶段
 * 拦截 + stopImmediatePropagation，阻止全局 useResultNavigation 的退出逻辑）。
 * 非设置型视图（translate/agent/ocr 等输入型）不调用本 composable，esc 直接退出。
 */
export function useSettingsInput() {
  const inputRefs = ref<Record<string, InstanceType<typeof BaseInput>>>({})
  const passwordVisibility = ref<Record<string, boolean>>({})
  const editingValue = ref<Record<string, string>>({})
  const editingOriginal = ref<Record<string, string>>({})

  // 设置型视图的 Escape：表单控件聚焦时先失焦（不退出视图），由全局 useResultNavigation
  // 之外独立处理。capture 阶段先于全局 bubble listener，stopImmediatePropagation 阻止后者，
  // 使「先失焦再退出」的分层行为仅作用于设置型视图，不影响输入型视图（translate/agent/ocr 等）。
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return
    const appStore = useAppStore()
    if (appStore.isDialogOpen) return
    const el = document.activeElement
    if (!(el instanceof HTMLElement)) return
    const isFormControl =
      el.tagName === 'INPUT' ||
      el.tagName === 'TEXTAREA' ||
      el.tagName === 'SELECT' ||
      el.hasAttribute('contenteditable') ||
      el.hasAttribute('data-settings-control')
    if (isFormControl) {
      e.preventDefault()
      e.stopImmediatePropagation()
      el.blur()
    }
  }

  const add = () => document.addEventListener('keydown', onKeydown, true)
  const remove = () => document.removeEventListener('keydown', onKeydown, true)
  // 调用者均在 ContentView 的 KeepAlive 内：切走时 onUnmounted 不触发，listener 会泄漏到
  // 非设置视图（误伤全局搜索框等 INPUT 的 Esc）。onActivated/onDeactivated 随缓存激活/停用
  // 精确挂载/卸载；保留 onMounted/onUnmounted 兼容非 KeepAlive 调用（add 同函数同参数去重）。
  onMounted(add)
  onActivated(add)
  onDeactivated(remove)
  onUnmounted(remove)
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
  function handleExecute(item: SettingItem, e?: KeyboardEvent | MouseEvent): boolean {
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
