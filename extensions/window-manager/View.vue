<template>
  <div class="flex-col-full">
    <BaseList
      :items="settingsItems"
      v-model:selected-index="settingsSelectedIndex"
      @execute="onSettingsExecute"
    >
      <template #item="{ item, selected, setRef }">
        <BaseListItem
          v-if="item.type === 'toggle'"
          :ref="setRef"
          title="拖拽分屏"
          subtitle="拖动窗口到屏幕顶部触发"
          icon="i-ri-drag-move-2-line"
          :selected="selected"
        >
          <template #trailing>
            <BaseButton
              :variant="settings.wmDragSnapEnabled ? 'primary' : 'default'"
              @click.stop="settings.setWmDragSnapEnabled(!settings.wmDragSnapEnabled)"
            >
              {{ settings.wmDragSnapEnabled ? '已开启' : '已关闭' }}
            </BaseButton>
          </template>
        </BaseListItem>

        <BaseListItem
          v-else
          :ref="setRef"
          title="自定义尺寸"
          subtitle="自定义布局使用"
          icon="i-ri-ruler-line"
          :selected="selected"
        >
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
import { useSettingsStore } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseInput from '@/components/ui/BaseInput.vue'

const settings = useSettingsStore()
const settingsSelectedIndex = ref(0)

const widthInputRef = ref<InstanceType<typeof BaseInput>>()
const heightInputRef = ref<InstanceType<typeof BaseInput>>()

const draftWidth = ref(String(settings.wmCustomWidth))
const draftHeight = ref(String(settings.wmCustomHeight))
const focusedField = ref<'width' | 'height' | null>(null)
const cancelOnBlur = ref(false)

interface ToggleItem {
  type: 'toggle'
  id: string
}

interface InputItem {
  type: 'input'
  id: string
}

type SettingsItem = ToggleItem | InputItem

const settingsItems: SettingsItem[] = [
  { type: 'toggle', id: 'wm-drag-snap' },
  { type: 'input', id: 'wm-custom-size' },
]

function onFocus(field: 'width' | 'height') {
  focusedField.value = field
  if (field === 'width') draftWidth.value = String(settings.wmCustomWidth)
  else draftHeight.value = String(settings.wmCustomHeight)
}

function onBlur(field: 'width' | 'height') {
  focusedField.value = null
  if (cancelOnBlur.value) {
    cancelOnBlur.value = false
    draftWidth.value = String(settings.wmCustomWidth)
    draftHeight.value = String(settings.wmCustomHeight)
    return
  }
  if (field === 'width') {
    const n = parseInt(draftWidth.value, 10)
    if (n > 0) settings.setWmCustomWidth(n)
    draftWidth.value = String(settings.wmCustomWidth)
  } else {
    const n = parseInt(draftHeight.value, 10)
    if (n > 0) settings.setWmCustomHeight(n)
    draftHeight.value = String(settings.wmCustomHeight)
  }
}

function onInputKeydown(e: KeyboardEvent, field: 'width' | 'height') {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopImmediatePropagation()
    cancelOnBlur.value = true
    if (field === 'width') draftWidth.value = String(settings.wmCustomWidth)
    else draftHeight.value = String(settings.wmCustomHeight)
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

function onSettingsExecute() {}
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
