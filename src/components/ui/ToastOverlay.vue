<template>
  <Teleport to="body">
    <TransitionGroup
      :key="overlayKey"
      tag="div"
      class="flex flex-col gap-2 pointer-events-none bottom-3 right-3 fixed z-9999"
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 translate-y-2 scale-95"
      enter-to-class="opacity-100 translate-y-0 scale-100"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100 translate-y-0 scale-100"
      leave-to-class="opacity-0 translate-y-2 scale-95"
    >
      <div
        v-for="t in toasts"
        :key="t.id"
        class="dropdown-panel max-w-96 pointer-events-auto"
        @mouseenter="onEnter"
        @mouseleave="onLeave"
      >
        <div class="text-sm font-medium px-3 py-1.5 radius-ctrl flex gap-2 min-w-0 items-center">
          <i
            class="shrink-0"
            :class="
              t.kind === 'error'
                ? 'i-ri-error-warning-line text-danger'
                : 'i-ri-check-line text-accent'
            "
          />
          <span
            text="secondary"
            class="leading-snug text-justify min-w-0 select-text break-words"
            >{{ t.message }}</span
          >
        </div>
      </div>
    </TransitionGroup>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { toasts, overlayKey, pauseAllToasts, resumeAllToasts } from '@/composables/useToast'

// 悬浮计数：任一 toast 悬浮即冻结全部倒计时，给用户从容选择/阅读文本的时间。
// 避免悬浮某条时其他条倒计时到期消失 → 重排 → 鼠标位移触发 mouseleave 误恢复。
const hoverCount = ref(0)
function onEnter() {
  hoverCount.value++
  if (hoverCount.value === 1) pauseAllToasts()
}
function onLeave() {
  hoverCount.value = Math.max(0, hoverCount.value - 1)
  if (hoverCount.value === 0) resumeAllToasts()
}
// clearToasts（hideWindow）清空列表时同步复位引用计数：
// 否则 hover→隐藏→再显示 后 hoverCount 卡在 >0，hover 暂停功能静默失效。
watch(
  () => toasts.value.length,
  (n) => {
    if (n === 0) hoverCount.value = 0
  },
)
</script>
