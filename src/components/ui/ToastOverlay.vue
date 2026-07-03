<template>
  <Teleport to="body">
    <TransitionGroup
      :key="overlayKey"
      tag="div"
      class="flex flex-col gap-2 pointer-events-none bottom-4 right-4 fixed z-50"
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 translate-y-2 scale-95"
      enter-to-class="opacity-100 translate-y-0 scale-100"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="opacity-100 translate-y-0 scale-100"
      leave-to-class="opacity-0 translate-y-2 scale-95"
    >
      <div v-for="t in toasts" :key="t.id" class="dropdown-panel">
        <div class="text-sm font-medium px-3 py-1.5 rounded-md flex gap-2 items-center">
          <i
            :class="
              t.kind === 'error'
                ? 'i-ri-error-warning-line text-red-500'
                : 'i-ri-check-line text-accent'
            "
          />
          <span text="tx-secondary">{{ t.message }}</span>
        </div>
      </div>
    </TransitionGroup>
  </Teleport>
</template>

<script setup lang="ts">
import { toasts, overlayKey } from '@/composables/useToast'
</script>
