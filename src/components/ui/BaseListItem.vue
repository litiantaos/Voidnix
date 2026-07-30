<template>
  <div
    flex
    p="3"
    gap="3"
    select="none"
    text="primary"
    :class="{ 'items-start': multilineTitle, 'items-center': !multilineTitle }"
  >
    <div
      v-if="icon || $slots.icon"
      class="radius-ctrl flex-center h-9 w-9"
      :class="iconWrapperClass || 'fill-mist'"
      shrink="0"
      overflow="hidden"
    >
      <slot name="icon">
        <i v-if="icon" :class="icon" text="sm"></i>
      </slot>
    </div>
    <div flex="~ col 1" min-w="0" justify="center">
      <div text="sm" font="medium" :class="[titleClass, { truncate: !multilineTitle }]">
        <slot name="title">{{ title }}</slot>
      </div>
      <div v-if="hasSubtitle" text="xs muted" flex w="full" items="center" overflow="hidden">
        <slot name="subtitle">
          <span class="flex-1 min-w-0 truncate">{{ subtitle }}</span>
        </slot>
      </div>
    </div>
    <div v-if="hasTrailing" flex="none" :class="{ 'h-9 flex items-center': multilineTitle }">
      <slot name="trailing" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, useSlots, Comment } from 'vue'

const props = defineProps<{
  title?: string
  subtitle?: string
  icon?: string
  iconWrapperClass?: string
  multilineTitle?: boolean
  /** 标题色调：accent（强调，如代理当前节点）/ danger（危险操作，如移除） */
  tone?: 'accent' | 'danger'
}>()

const slots = useSlots()

/** 标题语义色：tone 驱动，元素自身声明优先于 .ui-active 继承色 */
const titleClass = computed(() => {
  if (props.tone === 'accent') return 'text-accent'
  if (props.tone === 'danger') return 'text-danger'
  return undefined
})

/** 副标题是否实际有内容：prop 或 slot 渲染出非注释节点。
 *  空 slot（条件全 false）返回注释节点数组，v-if 据此跳过整行渲染。 */
const hasSubtitle = computed(() => {
  if (props.subtitle) return true
  const v = slots.subtitle?.()
  return !!v && v.some((n) => n.type !== Comment)
})

/** trailing slot 是否实际渲染出非注释节点。
 *  避免空 slot（如 ResultItem 的 source 不存在）仍渲染空 div 参与 flex gap，导致内容右侧多余留白。 */
const hasTrailing = computed(() => {
  const v = slots.trailing?.()
  return !!v && v.some((n) => n.type !== Comment)
})
</script>
