<template>
  <BaseList
    :items="items"
    v-model:selected-index="selectedIndex"
    keyboard-navigation
    @execute="toggle"
  >
    <template #item="{ selected, setRef, select }">
      <BaseListItem
        :ref="setRef"
        title="启用终端自动建议"
        subtitle="Tab 切换备选，→ 接受，Ctrl+X 关闭"
        :selected="selected"
        @click="select()"
      >
        <template #trailing>
          <BaseButton
            :variant="settings.zshAutosuggestionsEnabled ? 'primary' : 'default'"
            @click.stop="toggle"
          >
            {{ settings.zshAutosuggestionsEnabled ? '已开启' : '已关闭' }}
          </BaseButton>
        </template>
      </BaseListItem>
    </template>
  </BaseList>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

const settings = useSettingsStore()
const selectedIndex = ref(0)
const items = [{}]

const toggle = async () => {
  const newVal = !settings.zshAutosuggestionsEnabled
  await settings.setZshAutosuggestionsEnabled(newVal)
}
</script>
