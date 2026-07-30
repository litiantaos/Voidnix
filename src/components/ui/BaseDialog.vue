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
          class="dialog-to radius-panel"
          :class="[sizeClass, { 'has-footer': resolvedShowFooter }]"
          outline="none"
          flex="~ col"
          relative
          z="10"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          :aria-describedby="message ? descId : undefined"
          tabindex="-1"
        >
          <!-- 内容区通铺；顶/底 chrome 浮层盖住，滚动时形成延伸感 -->
          <div class="dialog-body hide-scrollbar" overflow="auto" flex="1" min-h="0">
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

          <div
            class="dialog-chrome dialog-chrome-header"
            text="sm primary"
            font="bold"
            select="none"
          >
            <slot name="header">
              <h3 :id="titleId">
                {{ title }}
              </h3>
            </slot>
          </div>

          <div v-if="resolvedShowFooter" class="dialog-chrome dialog-chrome-footer">
            <slot name="footer">
              <div flex gap="2" justify="between" items="center">
                <div>
                  <slot name="footer-start" />
                </div>
                <div flex gap="2">
                  <BaseButton v-if="showCancel" :active="focusIndex === 0" @click="close('cancel')">
                    {{ cancelLabel || '取消' }}
                  </BaseButton>
                  <BaseButton variant="primary" :active="focusIndex === 1" @click="requestConfirm">
                    {{ okLabel || '确定' }}
                  </BaseButton>
                </div>
              </div>
            </slot>
          </div>
        </div>
      </Transition>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, onDeactivated, nextTick, useId } from 'vue'
import BaseButton from './BaseButton.vue'
import { getFocusableElements, isComposing, trapFocus } from '@/utils/dom'

/** 弹窗关闭来源 */
export type CloseReason = 'cancel' | 'escape' | 'overlay' | 'dismiss'

/**
 * variant 驱动弹窗行为模式：
 * - confirm：确认对话框。默认显示 footer，支持方向键/回车导航，焦点聚焦确认按钮
 * - form：表单面板。默认隐藏 footer。有 footer 时回车提交：
 *   单行 INPUT（方便「改完直接回车创建」）；BaseSelect 自管 Enter（关=开/开=选），不参与提交。
 *   多行 textarea 不提交。焦点聚焦首个输入，遮罩始终可关闭
 *
 * closeOnConfirm：false 时确定/回车只 emit confirm、不关窗，由父级在异步结果后决定卸载
 * （如新建文件：失败 toast 时弹窗保持，避免先关再开）。
 *
 * KeepAlive 切走 / 扩展切换：onDeactivated 以 dismiss 关窗（Teleport 挂 body，不关会残留）。
 */
interface Props {
  title: string
  message?: string
  variant?: 'confirm' | 'form'
  size?: 'sm' | 'md' | 'lg'
  okLabel?: string
  cancelLabel?: string
  showCancel?: boolean
  showFooter?: boolean | null
  /** 确定时是否先关窗再 emit confirm（默认 true） */
  closeOnConfirm?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'confirm',
  size: 'sm',
  showCancel: true,
  showFooter: null,
  closeOnConfirm: true,
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

/** 确定：默认关窗后 emit；closeOnConfirm=false 时仅 emit，父级自行卸窗 */
function requestConfirm() {
  if (props.closeOnConfirm === false) {
    if (closing) return
    emit('confirm')
    return
  }
  close()
}

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

  const target = e.target as HTMLElement
  const isTextarea = target.tagName === 'TEXTAREA' || target.isContentEditable
  const isField =
    target.tagName === 'INPUT' ||
    target.tagName === 'TEXTAREA' ||
    target.tagName === 'SELECT' ||
    target.isContentEditable

  // form：方向键留给弹窗内容（多选列表等），阻止冒泡到外层 BaseList
  if (
    props.variant === 'form' &&
    (e.key === 'ArrowDown' ||
      e.key === 'ArrowUp' ||
      e.key === 'Home' ||
      e.key === 'End' ||
      e.key === 'PageDown' ||
      e.key === 'PageUp')
  ) {
    e.stopPropagation()
    // 不 preventDefault：内容区 listbox 仍可自行处理
    return
  }

  // form + footer 回车提交：仅单行 INPUT（BaseSelect 自管 Enter，不冒泡到此）
  if (
    props.variant === 'form' &&
    resolvedShowFooter.value &&
    e.key === 'Enter' &&
    !isComposing(e) &&
    !isTextarea &&
    target.tagName === 'INPUT'
  ) {
    e.preventDefault()
    e.stopPropagation()
    requestConfirm()
    return
  }

  // 以下按键仅 confirm 模式有效
  if (props.variant !== 'confirm') return

  // 输入框内不拦截方向键和回车
  if (isField) return

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
      requestConfirm()
    } else {
      close('cancel')
    }
  }
}

function onOverlayClick() {
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

// KeepAlive 切走（快捷键换扩展等）：Teleport 已挂 body，必须卸窗，否则残留遮罩
onDeactivated(() => {
  if (closing || !visible.value) return
  close('dismiss')
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
  transition: background-color var(--duration-normal) var(--ease-out);
}
.backdrop-to {
  background-color: var(--color-dialog-overlay);
}
.backdrop-from {
  background-color: transparent;
}

.dialog-active {
  /* 仅 opacity + transform（GPU 合成）；box-shadow 带 32px 大模糊，过渡每帧光栅化致顿，
     且 none→具体阴影插值跨引擎不一致。阴影改为始终在场，靠整体 opacity 淡入出现 */
  transition:
    opacity var(--duration-normal) var(--ease-out),
    transform var(--duration-normal) var(--ease-out);
}
.dialog-to {
  opacity: 1;
  transform: scale(1);
  /* 独立 dialog 面（非 soft-surface）：近实白 + 中性描边 + elevation；内容通铺，chrome 浮层渐隐 */
  overflow: hidden;
  background: var(--dialog-fill);
  border: 1px solid var(--dialog-border);
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
  box-shadow: var(--dialog-shadow);
  /* 标题 / 底栏浮层预留（与 .dialog-chrome 高度对齐） */
  --dialog-chrome-top: 52px;
  --dialog-chrome-bottom: 16px;
}
.dialog-to.has-footer {
  --dialog-chrome-bottom: 64px;
}
.dialog-from {
  opacity: 0;
  transform: scale(0.96);
}

.dialog-body {
  padding: var(--dialog-chrome-top) 16px var(--dialog-chrome-bottom);
}

/*
 * 顶/底 chrome 浮层：实色→透明渐变，内容滚入时有延伸感。
 * header 只展示文案，不吃点击；footer 内控件恢复 pointer-events。
 */
.dialog-chrome {
  position: absolute;
  inset-inline: 0;
  z-index: 2;
  pointer-events: none;
}
.dialog-chrome-header {
  top: 0;
  padding: 16px 16px 20px;
  background: linear-gradient(
    to bottom,
    var(--dialog-fill) 0%,
    var(--dialog-fill) 52%,
    transparent 100%
  );
}
.dialog-chrome-footer {
  bottom: 0;
  padding: 20px 16px 16px;
  background: linear-gradient(
    to top,
    var(--dialog-fill) 0%,
    var(--dialog-fill) 52%,
    transparent 100%
  );
}
.dialog-chrome-footer :deep(button),
.dialog-chrome-footer :deep([role='button']),
.dialog-chrome-footer :deep(a) {
  pointer-events: auto;
}
/* 默认 footer 行容器也要可点（包住按钮与 footer-start） */
.dialog-chrome-footer > * {
  pointer-events: auto;
}
</style>
