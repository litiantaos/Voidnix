<template>
  <div class="flex-col-full">
    <BaseList
      :items="settingsItems"
      v-model:selected-index="settingsSelectedIndex"
      group-field="group"
      :group-title="(g: string) => g"
      @execute="onSettingsExecute"
    >
      <template #item="{ item, selected, setRef }">
        <BaseListItem
          v-if="item.type === 'toggle'"
          :ref="setRef"
          title="布局面板"
          subtitle="鼠标移至屏幕顶部中心激活"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              :variant="wmConfig.dragSnapEnabled ? 'primary' : 'default'"
              @click.stop="wmConfig.dragSnapEnabled = !wmConfig.dragSnapEnabled"
            >
              {{ wmConfig.dragSnapEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
          </template>
        </BaseListItem>

        <BaseListItem v-else :ref="setRef" title="自定义尺寸" :selected="selected">
          <template #trailing>
            <div class="no-number-spin" flex gap="1.5" items="center" @click.stop>
              <BaseInput
                ref="widthInputRef"
                type="number"
                :model-value="draftWidth"
                class="w-16"
                @update:model-value="draftWidth = $event"
                @keydown="onInputKeydown($event, 'width')"
                @focus="onFocus('width')"
                @blur="onBlur('width')"
              />
              <span text="xs tx-subtle" select="none">×</span>
              <BaseInput
                ref="heightInputRef"
                type="number"
                :model-value="draftHeight"
                class="w-16"
                @update:model-value="draftHeight = $event"
                @keydown="onInputKeydown($event, 'height')"
                @focus="onFocus('height')"
                @blur="onBlur('height')"
              />
            </div>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { config as wmConfig } from './config'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseInput from '@/components/ui/BaseInput.vue'

const settingsSelectedIndex = ref(0)

const widthInputRef = ref<InstanceType<typeof BaseInput>>()
const heightInputRef = ref<InstanceType<typeof BaseInput>>()

const draftWidth = ref(String(wmConfig.customWidth))
const draftHeight = ref(String(wmConfig.customHeight))
const focusedField = ref<'width' | 'height' | null>(null)
const cancelOnBlur = ref(false)

interface ToggleItem {
  type: 'toggle'
  id: string
  group: string
}

interface InputItem {
  type: 'input'
  id: string
  group: string
}

type SettingsItem = ToggleItem | InputItem

const settingsItems: SettingsItem[] = [
  { type: 'toggle', id: 'wm-drag-snap', group: '通用' },
  { type: 'input', id: 'wm-custom-size', group: '通用' },
]

function onFocus(field: 'width' | 'height') {
  focusedField.value = field
  if (field === 'width') draftWidth.value = String(wmConfig.customWidth)
  else draftHeight.value = String(wmConfig.customHeight)
}

function onBlur(field: 'width' | 'height') {
  focusedField.value = null
  if (cancelOnBlur.value) {
    cancelOnBlur.value = false
    draftWidth.value = String(wmConfig.customWidth)
    draftHeight.value = String(wmConfig.customHeight)
    return
  }
  if (field === 'width') {
    const n = parseInt(draftWidth.value, 10)
    if (n > 0) wmConfig.customWidth = n
    draftWidth.value = String(wmConfig.customWidth)
  } else {
    const n = parseInt(draftHeight.value, 10)
    if (n > 0) wmConfig.customHeight = n
    draftHeight.value = String(wmConfig.customHeight)
  }
}

function onInputKeydown(e: KeyboardEvent, field: 'width' | 'height') {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopImmediatePropagation()
    cancelOnBlur.value = true
    if (field === 'width') draftWidth.value = String(wmConfig.customWidth)
    else draftHeight.value = String(wmConfig.customHeight)
    ;(e.target as HTMLInputElement).blur()
    document.getElementById('main-search-input')?.focus()
  } else if (e.key === 'Enter') {
    e.preventDefault()
    e.stopImmediatePropagation()
    ;(e.target as HTMLInputElement).blur()
    document.getElementById('main-search-input')?.focus()
  } else if (e.key === 'Tab' && !e.shiftKey) {
    e.preventDefault()
    if (field === 'width') heightInputRef.value?.focus()
    else widthInputRef.value?.focus()
  }
}

function onSettingsExecute(item: SettingsItem) {
  if (item.type === 'toggle') {
    wmConfig.dragSnapEnabled = !wmConfig.dragSnapEnabled
  }
}
</script>

<style scoped>
.no-number-spin :deep(input[type='number']::-webkit-inner-spin-button),
.no-number-spin :deep(input[type='number']::-webkit-outer-spin-button) {
  -webkit-appearance: none;
  margin: 0;
}
.no-number-spin :deep(input[type='number']) {
  -moz-appearance: textfield;
  text-align: center;
}
</style>
