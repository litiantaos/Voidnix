<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" shortcut-id="translate" />

    <!-- 有道 -->
    <BaseDialog
      v-if="showYoudaoModal"
      title="有道翻译"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveYoudao"
      @cancel="showYoudaoModal = false"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">APP ID</span>
          <BaseInput
            v-model="youdaoForm.appKey"
            :type="passwordVisible ? 'text' : 'password'"
            placeholder="App ID"
          >
            <template #suffix>
              <BaseButton
                variant="ghost"
                :icon="passwordVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
                class="!text-muted !px-1 !shrink-0 !h-auto"
                @click.stop="passwordVisible = !passwordVisible"
              />
            </template>
          </BaseInput>
        </div>
        <div class="form-field">
          <span class="form-label">APP SECRET</span>
          <BaseInput
            v-model="youdaoForm.appSecret"
            :type="passwordVisible ? 'text' : 'password'"
            placeholder="App Secret"
          >
            <template #suffix>
              <BaseButton
                variant="ghost"
                :icon="passwordVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
                class="!text-muted !px-1 !shrink-0 !h-auto"
                @click.stop="passwordVisible = !passwordVisible"
              />
            </template>
          </BaseInput>
        </div>
      </div>
    </BaseDialog>

    <!-- AI：多选中枢模型 + 提示词 + 跳转提供商 -->
    <BaseDialog
      v-if="showAiModal"
      title="AI 翻译"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveAi"
      @cancel="showAiModal = false"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <div flex items="center" gap="2" class="form-label w-full">
            <span class="flex-1">{{ modelFieldLabel }}</span>
            <BaseButton
              v-if="modelOptions.length > 0"
              variant="ghost"
              icon="i-ri-settings-3-line"
              title="管理提供商"
              @click="goAiProviders"
            />
          </div>

          <div v-if="modelOptions.length === 0">
            <BaseButton class="self-start" icon="i-ri-key-2-line" @click="goAiProviders">
              打开 AI 提供商
            </BaseButton>
          </div>

          <!-- 选中仅图标变化（checkbox 空心/实心）；聚焦用边框色 -->
          <div
            v-else
            role="listbox"
            aria-multiselectable="true"
            aria-label="选择翻译模型"
            flex="~ col"
            gap="1.5"
            class="hide-scrollbar max-h-48 overflow-y-auto"
            @keydown="onModelListKeydown"
          >
            <BaseButton
              v-for="(opt, index) in modelOptions"
              :key="opt.key"
              :ref="(el) => setModelBtnRef(el, index)"
              role="option"
              :aria-selected="selectedKeySet.has(opt.key)"
              :tabindex="modelFocusIndex === index ? 0 : -1"
              class="model-option w-full !justify-start"
              :class="modelFocusIndex === index ? 'model-option-focused' : ''"
              :icon="
                selectedKeySet.has(opt.key)
                  ? 'i-ri-checkbox-circle-fill'
                  : 'i-ri-checkbox-blank-circle-line'
              "
              @click="onModelClick(opt.key, index)"
              @focus="modelFocusIndex = index"
            >
              <span class="text-left flex-1 min-w-0 truncate">{{ opt.displayLabel }}</span>
              <span text="xs muted" class="shrink-0 max-w-28 truncate">{{
                opt.providerLabel
              }}</span>
            </BaseButton>
          </div>
        </div>

        <div class="form-field">
          <span class="form-label">提示词</span>
          <BaseTextarea
            v-model="aiForm.prompt"
            :rows="5"
            :max-height="0"
            :auto-resize="false"
            :submit-on-enter="false"
            placeholder="Translate the following text from {fromLang} to {toLang}:\n\n{text}"
          />
        </div>
      </div>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, type ComponentPublicInstance } from 'vue'
import {
  config as translateConfig,
  getYoudaoConfig,
  getAiConfig,
  updateYoudaoConfig,
  updateAiConfig,
  selectionKey,
  parseSelectionKey,
  effectiveAiSelections,
  type AiModelSelection,
} from './config'
import {
  config as aiProvidersConfig,
  providerDisplayName,
  selectionDisplayLabel,
  hasMultiKeyProvider,
} from '@/runtime/ai-providers'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import type { SettingItem } from '@/types/settings'

const appStore = useAppStore()

const { value: translateShortcutValue, update: handleTranslateShortcutChange } = useShortcutConfig(
  'translate',
  'Alt+T',
)

const handleTargetLangChange = async (val: string | number) => {
  translateConfig.targetLang = String(val)
}

function youdaoSubtitle(): string {
  const c = getYoudaoConfig()
  return c.appKey && c.appSecret ? '已配置' : '未配置'
}

function formatAiSelectionSummary(s: {
  providerId: string
  keyId?: string
  model: string
}): string {
  return selectionDisplayLabel(s.providerId, s.keyId, s.model)
}

function aiSubtitle(): string {
  // 读时 effective + 依赖中枢，中枢改 models 后摘要立刻正确
  void aiProvidersConfig.providers
  const selections = effectiveAiSelections(getAiConfig().selections)
  const n = selections.length
  if (n === 0) return '未选择模型'
  const labels = selections.map(formatAiSelectionSummary)
  if (n === 1) return labels[0] || '未选择模型'
  if (n <= 3) return labels.join('、')
  return `${labels.slice(0, 2).join('、')} 等 ${n} 个`
}

// ── 有道弹窗 ──────────────────────────────────────────────

const showYoudaoModal = ref(false)
const passwordVisible = ref(false)
const youdaoForm = ref({ appKey: '', appSecret: '' })

function openYoudao() {
  const c = getYoudaoConfig()
  youdaoForm.value = { appKey: c.appKey, appSecret: c.appSecret }
  passwordVisible.value = false
  showYoudaoModal.value = true
}

function saveYoudao() {
  updateYoudaoConfig({
    appKey: youdaoForm.value.appKey.trim(),
    appSecret: youdaoForm.value.appSecret.trim(),
  })
  showYoudaoModal.value = false
}

// ── AI 弹窗 ───────────────────────────────────────────────

const showAiModal = ref(false)
const aiForm = ref({ prompt: '' })
/** 选中的 `providerId::model` 列表（ref 数组保证模板响应） */
const selectedKeyList = ref<string[]>([])
const selectedKeySet = computed(() => new Set(selectedKeyList.value))
/** 键盘焦点行（与是否勾选独立） */
const modelFocusIndex = ref(0)
const modelBtnEls = ref<(HTMLElement | null)[]>([])

const modelOptions = computed(() => {
  type Opt = {
    key: string
    providerId: string
    keyId: string
    model: string
    /** 主文案：单 Key 仅模型；多 Key `模型 · 备注` */
    displayLabel: string
    /** 右侧次要：仅提供商名（Key 已进主文案） */
    providerLabel: string
  }
  const opts: Opt[] = []
  for (const p of aiProvidersConfig.providers) {
    const base = providerDisplayName(p)
    const keys = p.keys?.length ? p.keys : []
    if (keys.length === 0) continue
    for (const k of keys) {
      for (const m of p.models) {
        const model = m.trim()
        if (!model) continue
        opts.push({
          key: selectionKey({ providerId: p.id, keyId: k.id, model }),
          providerId: p.id,
          keyId: k.id,
          model,
          displayLabel: selectionDisplayLabel(p.id, k.id, model),
          providerLabel: base,
        })
      }
    }
  }
  return opts
})

/** 存在多 Key 时字段名点明凭证维度 */
const modelFieldLabel = computed(() => (hasMultiKeyProvider() ? '模型与 Key' : '模型'))

function setModelBtnRef(el: Element | ComponentPublicInstance | null, index: number) {
  if (!el) {
    modelBtnEls.value[index] = null
    return
  }
  modelBtnEls.value[index] =
    el instanceof HTMLElement ? el : ((el as ComponentPublicInstance).$el as HTMLElement)
}

function focusModel(index: number) {
  const n = modelOptions.value.length
  if (n === 0) return
  const i = Math.max(0, Math.min(index, n - 1))
  modelFocusIndex.value = i
  nextTick(() => {
    modelBtnEls.value[i]?.focus()
    modelBtnEls.value[i]?.scrollIntoView({ block: 'nearest' })
  })
}

function openAi() {
  const c = getAiConfig()
  aiForm.value = { prompt: c.prompt }
  selectedKeyList.value = effectiveAiSelections(c.selections).map(selectionKey)
  modelFocusIndex.value = 0
  modelBtnEls.value = []
  showAiModal.value = true
  // 等 BaseDialog 首焦完成后再抢到模型列表，避免 ↑↓ 无接收方
  nextTick(() => {
    nextTick(() => {
      if (modelOptions.value.length > 0) focusModel(0)
    })
  })
}

function toggleModel(key: string) {
  const list = selectedKeyList.value
  const i = list.indexOf(key)
  selectedKeyList.value = i >= 0 ? list.filter((k) => k !== key) : [...list, key]
}

function onModelClick(key: string, index: number) {
  modelFocusIndex.value = index
  toggleModel(key)
}

function onModelListKeydown(e: KeyboardEvent) {
  const n = modelOptions.value.length
  if (n === 0) return

  if (e.key === 'ArrowDown') {
    e.preventDefault()
    e.stopPropagation()
    focusModel(modelFocusIndex.value + 1)
    return
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    e.stopPropagation()
    focusModel(modelFocusIndex.value - 1)
    return
  }
  if (e.key === 'Home') {
    e.preventDefault()
    e.stopPropagation()
    focusModel(0)
    return
  }
  if (e.key === 'End') {
    e.preventDefault()
    e.stopPropagation()
    focusModel(n - 1)
    return
  }
  if (e.key === ' ' || e.key === 'Enter') {
    e.preventDefault()
    e.stopPropagation()
    const opt = modelOptions.value[modelFocusIndex.value]
    if (opt) toggleModel(opt.key)
  }
}

function goAiProviders() {
  showAiModal.value = false
  appStore.setActiveExtension('ai-providers')
}

function saveAi() {
  const selections: AiModelSelection[] = []
  for (const key of selectedKeyList.value) {
    const sel = parseSelectionKey(key)
    if (sel) selections.push(sel)
  }
  updateAiConfig({
    selections,
    prompt: aiForm.value.prompt,
  })
  showAiModal.value = false
}

// ── 列表 ──────────────────────────────────────────────────

const allItems = computed<SettingItem[]>(() => {
  // 触达 configs + 中枢（effective 摘要）
  void translateConfig.configs.length
  void getYoudaoConfig().appKey
  void getAiConfig().selections.length
  void getAiConfig().prompt
  void aiProvidersConfig.providers

  return [
    {
      id: 'translate-shortcut',
      title: '启动快捷键',
      type: 'shortcut',
      group: '通用',
      value: translateShortcutValue.value,
      update: handleTranslateShortcutChange,
    },
    {
      id: 'translate-target-lang',
      title: '目标语言',
      type: 'select',
      group: '通用',
      value: translateConfig.targetLang,
      options: [
        { label: '中文', value: 'zh' },
        { label: '英文', value: 'en' },
        { label: '日文', value: 'ja' },
        { label: '韩文', value: 'ko' },
        { label: '法文', value: 'fr' },
        { label: '德文', value: 'de' },
        { label: '西班牙文', value: 'es' },
      ],
      update: handleTargetLangChange,
    },
    {
      id: 'service-youdao',
      title: '有道翻译',
      subtitle: youdaoSubtitle(),
      type: 'action',
      group: '翻译服务',
      action: openYoudao,
    },
    {
      id: 'service-ai',
      title: 'AI 翻译',
      subtitle: aiSubtitle(),
      type: 'action',
      group: '翻译服务',
      action: openAi,
    },
  ]
})
</script>

<style scoped>
/* 始终保留 1px 边；选中不改面，仅图标切换；聚焦改框色 */
.model-option {
  border: 1px solid var(--soft-chip-border) !important;
  box-shadow: none !important;
}
.model-option-focused {
  border-color: var(--focus-ring-color) !important;
}
</style>
