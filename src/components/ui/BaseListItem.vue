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
        <img
          v-if="isImageIcon"
          :src="iconSrc"
          h="[57%]"
          max-w="[57%]"
          w="[57%]"
          object="contain"
          alt=""
        />
        <i v-else-if="icon" :class="icon" text="sm"></i>
      </slot>
    </div>
    <div flex="~ col 1" min-w="0" justify="center">
      <div text="sm" font="medium" :class="[titleClass, { truncate: !multilineTitle }]">
        <slot name="title">{{ title }}</slot>
      </div>
      <div
        v-if="hasSlot('subtitle')"
        text="xs muted"
        flex
        w="full"
        items="center"
        overflow="hidden"
      >
        <slot name="subtitle">
          <span class="flex-1 min-w-0 truncate">{{ subtitle }}</span>
        </slot>
      </div>
    </div>
    <div
      v-if="hasSlot('trailing')"
      flex="none"
      :class="{ 'h-9 flex items-center': multilineTitle }"
    >
      <slot name="trailing" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, useSlots, Comment } from 'vue'

const props = defineProps<{
  title?: string
  subtitle?: string
  /** 字体图标类（i- 前缀）或 base64 图片图标（应用图标，与 ResultIcon 同优先级语义） */
  icon?: string
  iconWrapperClass?: string
  multilineTitle?: boolean
  /** 标题色调：accent（强调，如代理当前节点）/ danger（危险操作，如移除） */
  tone?: 'accent' | 'danger'
}>()

/** 图片图标判定与 src 组装（与 ResultIcon 同源：i- 前缀 = 字体图标，其余 = base64）。
 *  面板行图片图标取 57%（ResultIcon 115% 的一半），外框（fill-mist 圆角底）保留 */
const isImageIcon = computed(() => !!props.icon && !props.icon.startsWith('i-'))
const iconSrc = computed(() => {
  const i = props.icon
  if (!i) return ''
  return i.startsWith('data:') ? i : 'data:image/png;base64,' + i
})

const slots = useSlots()

/** 标题语义色：tone 驱动，元素自身声明优先于 .ui-active 继承色 */
const titleClass = computed(() => {
  if (props.tone === 'accent') return 'text-accent'
  if (props.tone === 'danger') return 'text-danger'
  return undefined
})

/** 副标题是否实际有内容：prop 或 slot 渲染出非注释节点。空 slot（条件全 false）
 *  返回注释节点数组，v-if 据此跳过整行渲染。
 *  不用 computed：slot 集合形态由父 render 驱动（动态 slot 经 DYNAMIC_SLOTS force
 *  update 同步到本组件），不在响应式系统内——slot 缺席分支的依赖集为空，false 会被
 *  永久缓存，父组件再度传入 slot 也不失效（brew 更新按钮运行态清空后不恢复的根因） */
function hasSlot(name: 'subtitle' | 'trailing'): boolean {
  if (name === 'subtitle' && props.subtitle) return true
  const v = slots[name]?.()
  return !!v && v.some((n) => n.type !== Comment)
}
</script>
