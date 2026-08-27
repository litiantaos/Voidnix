<template>
  <BaseEmptyState
    v-if="!isConfigured"
    icon="i-ri-settings-3-line"
    :title="t('translate.notConfigured')"
  />

  <div v-else flex="~ col">
    <!-- 顶距交给 scrollContainer CHROME_HEIGHT（已含栏底 gap），勿再 p-t 叠双层 -->
    <div p="x-3 b-3">
      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        rounded="panel"
        :placeholder="t('translate.inputPlaceholder')"
        :rows="1"
        :max-height="0"
        @submit="handleSubmit"
      />
    </div>

    <div v-if="translateResults.length > 0">
      <BaseList
        :items="translateResults"
        v-model:selected-index="selectedIndex"
        navigate-on-input
        @execute="onExecuteResult"
      >
        <template #item="{ item, index }">
          <BaseListItem multiline-title>
            <template #title>
              <div
                v-if="item.loading && !item.translation"
                class="i-ri-loader-4-line animate-spin"
                text="base muted"
              />
              <span v-else leading="relaxed" font="normal" wrap="break-word">
                {{ item.translation }}
              </span>
            </template>
            <template #subtitle>
              <span class="flex-1 min-w-0 truncate">{{ item.engine }}</span>
              <BaseButton
                v-if="!item.loading && item.translation"
                variant="ghost"
                :icon="speakingIndex === index ? 'i-ri-volume-up-fill' : 'i-ri-volume-up-line'"
                :active="speakingIndex === index"
                class="!text-muted !px-1 !shrink-0 !h-auto"
                :title="t('translate.speak')"
                @click.stop="toggleSpeak(item, index)"
              />
            </template>
          </BaseListItem>
        </template>
      </BaseList>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  ref,
  computed,
  watch,
  nextTick,
  onMounted,
  onUnmounted,
  onActivated,
  onDeactivated,
} from 'vue'
import { invoke } from '@tauri-apps/api/core'

import { translateResults, isTranslating, translateText, pendingText, inputText } from './index'
import { config as translateConfig, resolveAiTargets } from './config'
import { refreshEnvSnapshot } from '@/runtime/ai-providers'
import { copyAndHide, useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { CMD } from '@/commands'
import { detectSpeechLang } from './logic'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import type { TranslateResult } from './index'

const appStore = useAppStore()

const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const selectedIndex = ref(0)
/** 正在朗读的结果下标（null = 无）。自然结束 / 被取代 / 停止时复位。 */
const speakingIndex = ref<number | null>(null)

const envTouched = ref(false)
onMounted(async () => {
  await refreshEnvSnapshot()
  envTouched.value = true
})

const isConfigured = computed(() => {
  void envTouched.value
  return translateConfig.configs.some(
    (c) =>
      (c.type === 'youdao' && c.appKey && c.appSecret) ||
      (c.type === 'ai' && resolveAiTargets(c).length > 0),
  )
})

watch(
  pendingText,
  (text) => {
    if (text) {
      inputText.value = text
      pendingText.value = ''
      translateText(text)
      // 选词翻译进入：翻译启动即让出输入框焦点，确保结果出来后回车直接复制（而非触发提交）
      textareaRef.value?.blur()
    }
  },
  { immediate: true },
)

watch(
  () => inputText.value === '',
  (empty) => {
    if (empty) {
      translateResults.value = []
      isTranslating.value = false
    }
  },
)

watch(translateResults, () => {
  selectedIndex.value = 0
  // 旧结果作废：仅在有朗读时停止（避免翻译流式 / splice 触发的空 invoke）
  if (speakingIndex.value !== null) {
    void invoke(CMD.stopSpeech)
    speakingIndex.value = null
  }
})

function handleSubmit() {
  const text = inputText.value.trim()
  if (!text) return
  translateText(text)
  ;(document.activeElement as HTMLElement)?.blur()
}

async function onExecuteResult(result: TranslateResult) {
  if (!result.translation || result.loading) return
  try {
    await copyAndHide(result.translation)
  } catch (e) {
    console.error('Failed to copy:', e)
  }
}

// 朗读：点正在朗读的本项 → 停；点其它项 → 朗读（取代旧朗读）
async function toggleSpeak(item: TranslateResult, index: number) {
  if (!item.translation) return
  if (speakingIndex.value === index) {
    // 先复位再 fire-and-forget 停止：消除 IPC 往返窗口内点新项被覆盖的竞态
    // （speak_text 内部本就会 cancel_current，stop 的 invoke 慢到也无妨）
    speakingIndex.value = null
    void invoke(CMD.stopSpeech)
    return
  }
  speakingIndex.value = index
  try {
    await invoke(CMD.speakText, {
      text: item.translation,
      lang: detectSpeechLang(item.translation),
    })
  } catch (e) {
    console.error('Failed to speak:', e)
    appStore.showStatus(t('translate.speakFailed'), { kind: 'error' })
  } finally {
    // 自然结束 / 被新朗读取代 → 复位（被取代时 index 已被新值覆盖，不误清）
    if (speakingIndex.value === index) speakingIndex.value = null
  }
}

// 正在翻译中（流式未完成）重新激活时不抢焦点；其余情况聚焦输入框。
// 注：选词翻译的焦点让出由 pendingText watch 的 blur 兜底（pendingText 由快捷键
// 异步取词后设置，远晚于 onActivated，无法在激活时判定）
function maybeFocusInput() {
  if (isTranslating.value) return
  // 窗口隐藏（未 key）时跳过聚焦：未激活应用内聚焦可编辑元素会触发 WebKit
  // activateIgnoringOtherApps 抢走前台，干扰 frontmost 捕获时序；由 onWinFocused 补聚焦
  if (!document.hasFocus()) return
  nextTick(() => textareaRef.value?.focus())
}
onMounted(maybeFocusInput)
onActivated(maybeFocusInput)
// 窗口获焦（Focused(true)）后聚焦输入框：覆盖「隐藏时 mount/activate 跳过聚焦」的路径
function onWinFocused() {
  if (appStore.activeExtId !== 'translate' || appStore.activeSubview || appStore.isDialogOpen)
    return
  maybeFocusInput()
}
onMounted(() => window.addEventListener('window-focused', onWinFocused))
onUnmounted(() => window.removeEventListener('window-focused', onWinFocused))
// 切走扩展：停止朗读（KeepAlive 下视图未卸载，仅 deactivate）
onDeactivated(() => {
  if (speakingIndex.value !== null) {
    void invoke(CMD.stopSpeech)
    speakingIndex.value = null
  }
})
</script>
