<template>
  <Teleport to="body">
    <div flex items="center" inset="0" justify="center" fixed z="100" @keydown="onKeyDown">
      <Transition
        appear
        enter-from-class="backdrop-from"
        enter-active-class="backdrop-active"
        leave-active-class="backdrop-active"
        leave-to-class="backdrop-from"
      >
        <div v-if="visible" class="backdrop-to" inset="0" absolute @click="onOverlayClick" />
      </Transition>
      <Transition
        appear
        enter-from-class="dialog-from"
        enter-active-class="dialog-active"
        leave-active-class="dialog-active"
        leave-to-class="dialog-from"
      >
        <div
          v-if="visible"
          ref="dialogRef"
          class="dialog-to"
          outline="none"
          rounded="lg"
          bg="white"
          flex="~ col"
          shadow="md"
          relative
          z="10"
          :class="sizeClass"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          :aria-describedby="message ? descId : undefined"
          tabindex="-1"
        >
          <div text="sm primary" font="bold" p="5" select="none">
            <slot name="header">
              <h3 :id="titleId">
                {{ title }}
              </h3>
            </slot>
          </div>

          <div class="hide-scrollbar" p="x-5" flex="1" overflow="auto">
            <slot>
              <p
                v-if="message"
                :id="descId"
                text="xs secondary"
                leading="relaxed"
                whitespace="pre-wrap"
                break="all"
              >
                {{ message }}
              </p>
            </slot>
          </div>

          <slot v-if="resolvedShowFooter" name="footer">
            <div p="5" flex gap="2" justify="between">
              <div>
                <slot name="footer-start" />
              </div>
              <div flex gap="2">
                <BaseButton v-if="showCancel" :active="focusIndex === 0" @click="close('cancel')">
                  {{ cancelLabel || '取消' }}
                </BaseButton>
                <BaseButton variant="primary" :active="focusIndex === 1" @click="close()">
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
  confirm: []
  cancel: [reason: CloseReason]
}>()

const resolvedShowFooter = computed(() => {
  if (props.showFooter !== null) return props.showFooter
  return props.variant === 'confirm'
})

const dialogRef = ref<HTMLElement>()
const titleId = useId()
const descId = useId()
let previousFocusEl: HTMLElement | null = null

const focusIndex = ref(1)

const visible = ref(true)
let closing = false
let closeTimer: ReturnType<typeof setTimeout> | null = null

function close(reason?: CloseReason) {
  if (closing) return
  closing = true
  visible.value = false
  // M-fe2：保存 timer handle，onUnmounted 清理避免组件在过渡期内被父级 v-if 干掉后仍 emit
  closeTimer = setTimeout(() => {
    closeTimer = null
    if (reason === undefined) emit('confirm')
    else emit('cancel', reason)
  }, 200)
}

const sizeClass = computed(() => {
  const sizeMap: Record<string, string> = {
    sm: 'w-40% max-h-80vh',
    md: 'w-60% max-h-80vh',
    lg: 'w-90% max-h-90vh',
  }
  return sizeMap[props.size]
})

function onKeyDown(e: KeyboardEvent) {
  // Escape：始终关闭弹窗
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    close('escape')
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
    target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT'
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
      close()
    } else {
      close('cancel')
    }
  }
}

function onOverlayClick() {
  if (props.variant === 'confirm' && props.kind === 'warning') return
  close('overlay')
}

onMounted(() => {
  previousFocusEl = document.activeElement as HTMLElement
  nextTick(() => {
    const el = dialogRef.value
    if (!el) return
    const focusable = getFocusableElements(el)
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
  // M-fe2：组件提前销毁时清 timer，避免已卸载组件继续 emit
  if (closeTimer) {
    clearTimeout(closeTimer)
    closeTimer = null
  }
  if (previousFocusEl && typeof previousFocusEl.focus === 'function') {
    previousFocusEl.focus()
  }
})
</script>

<style scoped>
.backdrop-active {
  transition: background-color 200ms ease-out;
}
.backdrop-to {
  background-color: rgba(0, 0, 0, 0.5);
}
.backdrop-from {
  background-color: rgba(0, 0, 0, 0);
}

.dialog-active {
  transition:
    opacity 200ms ease-out,
    transform 200ms ease-out;
}
.dialog-to {
  opacity: 1;
  transform: scale(1);
}
.dialog-from {
  opacity: 0;
  transform: scale(0.96);
}
</style>
