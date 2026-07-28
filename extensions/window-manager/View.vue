<template>
  <div class="flex-col-full">
    <BaseSettingsList :items="settingsItems">
      <template #trailing-wm-custom-size>
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
          <span text="xs secondary" select="none">×</span>
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
    </BaseSettingsList>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { focusSearchInput } from '@/stores/app'
import { config as wmConfig, clampWidth, clampHeight } from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import type { SettingItem } from '@/types/settings'

const widthInputRef = ref<InstanceType<typeof BaseInput>>()
const heightInputRef = ref<InstanceType<typeof BaseInput>>()

const draftWidth = ref(String(wmConfig.customWidth))
const draftHeight = ref(String(wmConfig.customHeight))
const focusedField = ref<'width' | 'height' | null>(null)

const settingsItems = computed<SettingItem[]>(() => [
  {
    id: 'wm-enabled',
    title: '启用窗口管理',
    subtitle: '鼠标移至任意屏顶部中心激活；多屏可左右迁移窗口',
    type: 'toggle',
    value: wmConfig.enabled,
    update: (v: boolean) => {
      wmConfig.enabled = v
    },
    group: '通用',
  },
  {
    id: 'wm-custom-size',
    title: '自定义尺寸',
    type: 'custom',
    group: '通用',
  },
])

function onFocus(field: 'width' | 'height') {
  focusedField.value = field
  if (field === 'width') draftWidth.value = String(wmConfig.customWidth)
  else draftHeight.value = String(wmConfig.customHeight)
}

function onBlur(field: 'width' | 'height') {
  focusedField.value = null
  if (field === 'width') {
    const n = parseInt(draftWidth.value, 10)
    if (n > 0) wmConfig.customWidth = clampWidth(n)
    draftWidth.value = String(wmConfig.customWidth)
  } else {
    const n = parseInt(draftHeight.value, 10)
    if (n > 0) wmConfig.customHeight = clampHeight(n)
    draftHeight.value = String(wmConfig.customHeight)
  }
  // Tab 切到另一个输入框时焦点转移晚于 blur，延迟一帧再判断
  requestAnimationFrame(() => {
    if (!document.activeElement?.hasAttribute('data-settings-control')) {
      focusSearchInput()
    }
  })
}

function onInputKeydown(e: KeyboardEvent, field: 'width' | 'height') {
  if (e.key === 'Tab' && !e.shiftKey) {
    e.preventDefault()
    if (field === 'width') heightInputRef.value?.focus()
    else widthInputRef.value?.focus()
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
  text-align: center;
}
</style>
