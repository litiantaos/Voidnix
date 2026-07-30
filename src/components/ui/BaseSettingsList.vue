<template>
  <BaseList
    :items="items"
    v-model:selected-index="localIndex"
    group-field="group"
    :group-title="(g: string) => g"
    @execute="onExecute"
  >
    <template v-if="$slots['group-title']" #group-title="slotProps">
      <slot name="group-title" v-bind="slotProps" />
    </template>
    <template #item="{ item }">
      <BaseListItem
        :id="`si-${item.id}`"
        :title="item.title"
        :subtitle="item.subtitle"
        :icon="item.icon"
        :tone="item.tone"
      >
        <template v-if="item.type === 'shortcut'" #trailing>
          <ShortcutInput
            :model-value="item.value"
            :shortcut-id="shortcutId || item.id"
            @update:model-value="item.update"
          />
        </template>
        <template v-else-if="item.type === 'select'" #trailing>
          <BaseSelect
            :model-value="item.value"
            :options="item.options"
            @update:model-value="item.update"
          />
        </template>
        <template v-else-if="item.type === 'toggle'" #trailing>
          <BaseButton
            :variant="item.value ? 'primary' : 'default'"
            @click.stop="item.update(!item.value)"
          >
            {{ item.value ? '已开启' : '已关闭' }}
          </BaseButton>
        </template>
        <template v-else-if="item.type === 'button'" #trailing>
          <BaseButton :variant="item.variant ?? 'default'" @click.stop="item.action">{{
            item.label
          }}</BaseButton>
        </template>
        <template v-else-if="item.type === 'custom'" #trailing>
          <slot :name="`trailing-${item.id}`" :item="item" />
        </template>
      </BaseListItem>
    </template>
  </BaseList>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import type { SettingItem } from '@/types/settings'

const props = defineProps<{
  items: SettingItem[]
  shortcutId?: string
  selectedIndex?: number
}>()

const emit = defineEmits<{
  'update:selectedIndex': [index: number]
  /** shortcut/select/toggle/button/action 已内置处理；custom 与未识别类型透传给消费者 */
  execute: [item: SettingItem]
}>()

const localIndex = ref(props.selectedIndex ?? 0)
watch(
  () => props.selectedIndex,
  (v) => {
    localIndex.value = v ?? 0
  },
)
watch(localIndex, (v) => emit('update:selectedIndex', v))

function onExecute(item: SettingItem) {
  // 优先聚焦可交互控件（select/shortcut/custom 内 input 等，均标记 data-settings-control）
  const control = document.querySelector(
    `#si-${item.id} [data-settings-control][tabindex="0"]`,
  ) as HTMLElement | null
  if (control) {
    control.focus()
    control.click()
    return
  }
  if (item.type === 'toggle') {
    item.update(!item.value)
    return
  }
  if (item.type === 'action' || item.type === 'button') {
    item.action()
    return
  }
  emit('execute', item)
}
</script>
