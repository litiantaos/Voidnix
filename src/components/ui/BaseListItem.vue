<template>
  <div
    class="group text-tx-primary p-3 rounded-lg flex gap-3 select-none"
    :class="[
      selected ? 'ui-active' : 'bg-transparent',
      multilineTitle ? 'items-start' : 'items-center',
    ]"
    role="option"
    :aria-selected="selected"
    @click="emit('click', $event)"
    @dblclick="emit('dblclick', $event)"
  >
    <div
      v-if="icon || $slots.icon"
      class="rounded-md flex shrink-0 h-9 w-9 items-center justify-center overflow-hidden"
      :class="iconWrapperClass || 'bg-black/4'"
    >
      <slot name="icon">
        <i v-if="icon" :class="[icon, iconClass]" class="text-sm"></i>
      </slot>
    </div>
    <div class="flex flex-1 flex-col min-w-0 justify-center">
      <div
        class="text-sm font-medium"
        :class="[{ 'mb-0.5': subtitle || $slots.subtitle }, !multilineTitle ? 'truncate' : '']"
      >
        <slot name="title">{{ title }}</slot>
      </div>
      <div
        v-if="subtitle || $slots.subtitle"
        class="text-xs text-tx-muted flex w-full items-center overflow-hidden"
      >
        <slot name="subtitle">{{ subtitle }}</slot>
      </div>
    </div>
    <div v-if="$slots.trailing" class="flex-none" :class="multilineTitle ? 'mt-1' : ''">
      <slot name="trailing" />
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  selected?: boolean
  title?: string
  subtitle?: string
  icon?: string
  iconClass?: string
  iconWrapperClass?: string
  multilineTitle?: boolean
}>()

const emit = defineEmits<{
  click: [MouseEvent]
  dblclick: [MouseEvent]
}>()
</script>
