<template>
  <BaseList
    :items="items"
    v-model:selected-index="selectedIndex"
    group-field="group"
    :group-title="(g: string) => g"
    @execute="toggle"
  >
    <template #item="{ selected, setRef }">
      <BaseListItem
        :ref="setRef"
        title="启用终端自动建议"
        subtitle="Tab 切换备选，→ 接受，Ctrl+X 开关，Ctrl+C 清空"
        :selected="selected"
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
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

const settings = useSettingsStore()
const appStore = useAppStore()
const selectedIndex = ref(0)
const items = [{ group: '通用' }]

const toggle = async () => {
  const newVal = !settings.zshAutosuggestionsEnabled
  try {
    await settings.setZshAutosuggestionsEnabled(newVal)
  } catch (e) {
    appStore.showStatus(`开关失败：${e ?? '未知错误'}`, 4000)
  }
}
</script>
