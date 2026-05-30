<template>
  <Teleport to="body">
    <div
      class="flex items-center inset-0 justify-center fixed z-100"
      @keydown="onKeyDown"
    >
      <Transition
        appear
        enter-from-class="backdrop-from"
        leave-to-class="backdrop-from"
      >
        <div class="backdrop-to inset-0 absolute" @click="onOverlayClick" />
      </Transition>
      <Transition
        appear
        enter-from-class="dialog-from"
        leave-to-class="dialog-from"
      >
        <div
          ref="dialogRef"
          class="dialog-to outline-none rounded-lg bg-white flex flex-col shadow-md relative z-10"
          :class="sizeClass"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          :aria-describedby="message ? descId : undefined"
          tabindex="-1"
        >
          <div class="text-sm text-tx-primary font-bold p-5">
            <slot name="header">
              <h3 :id="titleId">
                {{ title }}
              </h3>
            </slot>
          </div>

          <div class="hide-scrollbar px-5 flex-1 overflow-auto">
            <slot>
              <p
                v-if="message"
                :id="descId"
                class="text-xs text-tx-subtle leading-relaxed whitespace-pre-wrap break-all"
              >
                {{ message }}
              </p>
            </slot>
          </div>

          <slot v-if="resolvedShowFooter" name="footer">
            <div class="p-5 flex gap-2 justify-between">
              <div>
                <slot name="footer-start" />
              </div>
              <div class="flex gap-2">
                <BaseButton
                  v-if="showCancel"
                  :active="focusIndex === 0"
                  @click="emit('cancel', 'cancel')"
                >
                  {{ cancelLabel || '取消' }}
                </BaseButton>
                <BaseButton
                  variant="primary"
                  :active="focusIndex === 1"
                  @click="emit('confirm')"
                >
                  {{ okLabel || '确定' }}
                </BaseButton>
              </div>
            </div>
          </slot>
        </div>
      </Transition>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, useId } from 'vue'
import BaseButton from './BaseButton.vue'
import { getFocusableElements, trapFocus } from '@/utils/dom'

/** 弹窗关闭来源 */
export type CloseReason = 'cancel' | 'escape' | 'overlay'

/**
 * variant 驱动弹窗行为模式：
 * - confirm：确认对话框。默认显示 footer，支持方向键/回车导航，焦点聚焦确认按钮，warning 禁止遮罩关闭
 * - form：表单面板。默认隐藏 footer，不拦截方向键/回车，焦点聚焦首个输入，遮罩始终可关闭
 */
interface Props {
  title: string
  message?: string
  variant?: 'confirm' | 'form'
  size?: 'sm' | 'md' | 'lg'
  kind?: 'warning' | 'info'
  okLabel?: string
  cancelLabel?: string
  showCancel?: boolean
  showFooter?: boolean | null
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'confirm',
  size: 'sm',
  showCancel: true,
  showFooter: null,
})

const emit = defineEmits<{
  (e: 'confirm'): void
  (e: 'cancel', reason: CloseReason): void
}>()

// --- 派生状态：showFooter 由 variant + 显式 prop 共同决定 ---
// 显式传 showFooter 则尊重传值，否则按 variant 取默认值
const resolvedShowFooter = computed(() => {
  if (props.showFooter !== null) return props.showFooter
  return props.variant === 'confirm'
})

// --- DOM 引用与 ARIA ID ---
const dialogRef = ref<HTMLElement>()
const titleId = useId()
const descId = useId()
let previousFocusEl: HTMLElement | null = null

// --- 焦点索引（仅 confirm 模式有效） ---
const focusIndex = ref(1) // 0: cancel, 1: ok

// --- 尺寸映射 ---
const sizeClass = computed(() => {
  const sizeMap: Record<string, string> = {
    sm: 'w-40% max-h-80vh',
    md: 'w-60% max-h-80vh',
    lg: 'w-90% max-h-90vh',
  }
  return sizeMap[props.size]
})

// --- 限定作用域的键盘处理 ---
function onKeyDown(e: KeyboardEvent) {
  // Escape：始终关闭弹窗
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('cancel', 'escape')
    return
  }

  // Tab：焦点陷阱循环（两种模式通用）
  if (e.key === 'Tab') {
    trapFocus(getFocusableElements(dialogRef.value!), e)
    return
  }

  // 以下按键仅 confirm 模式有效
  if (props.variant !== 'confirm') return

  // 输入框内不拦截方向键和回车
  const target = e.target as HTMLElement
  const isInput =
    target.tagName === 'INPUT' ||
    target.tagName === 'TEXTAREA' ||
    target.tagName === 'SELECT'
  if (isInput) return

  if (e.key === 'ArrowRight') {
    e.preventDefault()
    if (props.showCancel) focusIndex.value = 1
    return
  }

  if (e.key === 'ArrowLeft') {
    e.preventDefault()
    if (props.showCancel) focusIndex.value = 0
    return
  }

  if (e.key === 'Enter') {
    e.preventDefault()
    e.stopPropagation()
    if (focusIndex.value === 1) {
      emit('confirm')
    } else {
      emit('cancel', 'cancel')
    }
  }
}

// --- 遮罩点击 ---
function onOverlayClick() {
  // confirm 模式下 warning 禁止遮罩关闭；form 模式始终可关闭
  if (props.variant === 'confirm' && props.kind === 'warning') return
  emit('cancel', 'overlay')
}

// --- 生命周期：焦点管理 ---
onMounted(() => {
  previousFocusEl = document.activeElement as HTMLElement
  nextTick(() => {
    const focusable = getFocusableElements(dialogRef.value!)
    if (focusable.length > 0) {
      if (props.variant === 'confirm') {
        // 确认型：聚焦确认按钮（最后一个可聚焦元素）
        focusable[focusable.length - 1]?.focus()
      } else {
        // 表单型：聚焦第一个输入
        focusable[0].focus()
      }
    } else if (dialogRef.value) {
      dialogRef.value.focus()
    }
  })
})

onUnmounted(() => {
  if (previousFocusEl && typeof previousFocusEl.focus === 'function') {
    previousFocusEl.focus()
  }
})
</script>

<style scoped>
.backdrop-to {
  background-color: rgba(0, 0, 0, 0.5);
  transition: background-color 200ms ease-out;
}
.backdrop-from {
  background-color: rgba(0, 0, 0, 0);
}

.dialog-to {
  transform: scale(1);
  transition: transform 200ms ease-out;
}
.dialog-from {
  transform: scale(0.95);
}
</style>
