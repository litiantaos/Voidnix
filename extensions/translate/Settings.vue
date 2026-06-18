<template>
  <div class="flex-col-full-pb">
    <BaseList
      :items="allItems"
      v-model:selected-index="selectedIndex"
      :group-field="(item: TranslateSettingsItem) => item.group"
      :group-title="(g: string) => g"
      @execute="(item: TranslateSettingsItem) => onExecute(item)"
    >
      <template #group-title="{ group }">
        <div flex items="center">
          <span>{{ group }}</span>
          <BaseButton
            v-if="group === '翻译服务'"
            class="ml-auto"
            icon="i-ri-add-line"
            @click.stop="openCreateModal()"
          />
        </div>
      </template>

      <template #item="{ item, selected, setRef }">
        <!-- 快捷键 -->
        <BaseListItem
          v-if="item.type === 'shortcut'"
          :ref="setRef"
          :id="`si-${SHORTCUT_ITEM_ID}`"
          title="启动快捷键"
          :selected="selected"
        >
          <template #trailing>
            <ShortcutInput
              :model-value="translateShortcutValue"
              shortcut-id="translate"
              @update:model-value="handleTranslateShortcutChange"
            />
          </template>
        </BaseListItem>

        <!-- 目标语言 -->
        <BaseListItem
          v-else-if="item.type === 'lang'"
          :ref="setRef"
          :id="`si-${LANG_ITEM_ID}`"
          title="目标语言"
          :selected="selected"
        >
          <template #trailing>
            <BaseSelect
              :model-value="translateConfig.targetLang"
              :options="targetLangOptions"
              @update:model-value="(val: string | number) => handleTargetLangChange(String(val))"
            />
          </template>
        </BaseListItem>

        <!-- 翻译服务 -->
        <BaseListItem
          v-else
          :ref="setRef"
          :title="providerLabel(item.config)"
          :subtitle="
            item.config.type === 'ai'
              ? item.config.models.filter(Boolean).join('、') || '未配置模型'
              : item.config.appKey
                ? '已配置'
                : '未配置'
          "
          :selected="selected"
        />
      </template>
    </BaseList>

    <!-- 编辑弹窗 -->
    <BaseDialog
      v-if="showConfigModal"
      :title="isCreating ? '添加翻译服务' : '编辑翻译服务'"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveConfigModal"
      @cancel="closeConfigModal"
    >
      <div flex="~ col" gap="4">
        <!-- 有道表单 -->
        <template v-if="editingType === 'youdao'">
          <div class="form-field">
            <span class="form-label">APP ID</span>
            <BaseInput
              v-model="youdaoForm.appKey"
              :type="passwordVisible ? 'text' : 'password'"
              placeholder="App ID"
            >
              <template #suffix>
                <button
                  class="i-ri-eye-line hover:text-black/60"
                  text="black/35"
                  shrink="0"
                  :class="{ 'i-ri-eye-off-line': passwordVisible }"
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
                <button
                  class="i-ri-eye-line hover:text-black/60"
                  text="black/35"
                  shrink="0"
                  :class="{ 'i-ri-eye-off-line': passwordVisible }"
                  @click.stop="passwordVisible = !passwordVisible"
                />
              </template>
            </BaseInput>
          </div>
        </template>

        <!-- AI 表单 -->
        <template v-else>
          <div class="form-field">
            <span class="form-label">API URL</span>
            <BaseInput v-model="aiForm.endpoint" placeholder="https://api.openai.com/v1" />
          </div>

          <div class="form-field">
            <span class="form-label">API KEY</span>
            <BaseInput
              v-model="aiForm.apiKey"
              :type="passwordVisible ? 'text' : 'password'"
              placeholder="sk-..."
            >
              <template #suffix>
                <button
                  class="i-ri-eye-line hover:text-black/60"
                  text="black/35"
                  shrink="0"
                  :class="{ 'i-ri-eye-off-line': passwordVisible }"
                  @click.stop="passwordVisible = !passwordVisible"
                />
              </template>
            </BaseInput>
          </div>

          <div class="form-field">
            <span class="form-label">模型</span>
            <div class="form-field">
              <div v-for="(_, index) in aiForm.models" :key="index" flex gap="1.5" items="center">
                <BaseInput v-model="aiForm.models[index]" placeholder="gpt-4o" class="flex-1" />
                <BaseButton
                  v-if="index > 0"
                  class="text-red-500"
                  icon="i-ri-close-line"
                  @click="removeModel(index)"
                />
                <BaseButton v-else icon="i-ri-add-line" @click="addModel" />
              </div>
            </div>
          </div>

          <div class="form-field">
            <span class="form-label">提示词</span>
            <BaseTextarea
              v-model="aiForm.prompt"
              placeholder="Translate the following text from {fromLang} to {toLang}:\n\n{text}"
            />
          </div>
        </template>
      </div>

      <template #footer-start>
        <BaseButton
          v-if="canDeleteConfig"
          class="text-red-500 hover:text-red-600"
          @click="deleteAndClose"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { type TranslateApiConfig, config as translateConfig, addTranslateConfig, updateTranslateConfig, removeTranslateConfig } from './config'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { providerLabelFromUrl } from '@/utils/provider'
import { useSettingsInput } from '@/composables/useSettingsInput'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

useSettingsInput()

const SHORTCUT_ITEM_ID = 'translate-shortcut'
const LANG_ITEM_ID = 'translate-target-lang'

const { value: translateShortcutValue, update: handleTranslateShortcutChange } = useShortcutConfig(
  'translate',
  'CommandOrControl+Shift+T',
)

const targetLangOptions = [
  { label: '中文', value: 'zh' },
  { label: '英文', value: 'en' },
  { label: '日文', value: 'ja' },
  { label: '韩文', value: 'ko' },
  { label: '法文', value: 'fr' },
  { label: '德文', value: 'de' },
  { label: '西班牙文', value: 'es' },
]

const handleTargetLangChange = async (val: string) => {
  translateConfig.targetLang = val
}

function providerLabel(config: TranslateApiConfig): string {
  if (config.type === 'youdao') return '有道翻译'
  return providerLabelFromUrl(config.endpoint, '翻译')
}

// ── 编辑弹窗状态 ──────────────────────────────────────────

interface YoudaoForm {
  appKey: string
  appSecret: string
}

interface AiForm {
  endpoint: string
  apiKey: string
  models: string[]
  prompt: string
}

const editingConfigId = ref('')
const isCreating = ref(false)
const showConfigModal = ref(false)
const youdaoForm = ref<YoudaoForm>({ appKey: '', appSecret: '' })
const aiForm = ref<AiForm>({
  endpoint: '',
  apiKey: '',
  models: [''],
  prompt: '',
})
const passwordVisible = ref(false)

function openConfigModal(config: TranslateApiConfig) {
  editingConfigId.value = config.id
  isCreating.value = false

  if (config.type === 'youdao') {
    youdaoForm.value = {
      appKey: config.appKey,
      appSecret: config.appSecret,
    }
  } else {
    aiForm.value = {
      endpoint: config.endpoint,
      apiKey: config.apiKey,
      models: config.models.length > 0 ? [...config.models] : [''],
      prompt: config.prompt,
    }
  }

  passwordVisible.value = false
  showConfigModal.value = true
}

function openCreateModal() {
  editingConfigId.value = ''
  isCreating.value = true
  aiForm.value = { endpoint: '', apiKey: '', models: [''], prompt: '' }
  passwordVisible.value = false
  showConfigModal.value = true
}

function closeConfigModal() {
  showConfigModal.value = false
  editingConfigId.value = ''
  isCreating.value = false
}

async function saveConfigModal() {
  if (isCreating.value) {
    const id = await addTranslateConfig()
    const models = aiForm.value.models.filter((m) => m.trim())
    await updateTranslateConfig(id, {
      type: 'ai',
      endpoint: aiForm.value.endpoint,
      apiKey: aiForm.value.apiKey,
      models,
      prompt: aiForm.value.prompt,
    })
  } else {
    if (!editingConfigId.value) return
    const config = translateConfig.configs.find((c) => c.id === editingConfigId.value)
    if (!config) return

    if (config.type === 'youdao') {
      await updateTranslateConfig(editingConfigId.value, {
        appKey: youdaoForm.value.appKey,
        appSecret: youdaoForm.value.appSecret,
      })
    } else {
      const models = aiForm.value.models.filter((m) => m.trim())
      await updateTranslateConfig(editingConfigId.value, {
        type: 'ai',
        endpoint: aiForm.value.endpoint,
        apiKey: aiForm.value.apiKey,
        models,
        prompt: aiForm.value.prompt,
      })
    }
  }
  closeConfigModal()
}

function addModel() {
  aiForm.value.models.push('')
}

function removeModel(index: number) {
  aiForm.value.models.splice(index, 1)
  if (aiForm.value.models.length === 0) {
    aiForm.value.models.push('')
  }
}

const canDeleteConfig = computed(() => {
  if (isCreating.value) return false
  const config = translateConfig.configs.find((c) => c.id === editingConfigId.value)
  if (!config || config.isDefault) return false
  return true
})

function deleteAndClose() {
  if (isCreating.value) {
    closeConfigModal()
    return
  }
  const id = editingConfigId.value
  if (id && canDeleteConfig.value) {
    closeConfigModal()
    removeTranslateConfig(id)
  }
}

// ── 列表项 ─────────────────────────────────────────────────

interface ShortcutItem {
  type: 'shortcut'
  group: string
}

interface LangItem {
  type: 'lang'
  group: string
}

interface ProviderItem {
  type: 'provider'
  group: string
  config: TranslateApiConfig
}

type TranslateSettingsItem = ShortcutItem | LangItem | ProviderItem

const allItems = computed<TranslateSettingsItem[]>(() => [
  { type: 'shortcut', group: '通用' },
  { type: 'lang', group: '通用' },
  ...translateConfig.configs.map((c) => ({
    type: 'provider' as const,
    group: '翻译服务',
    config: c,
  })),
])

const selectedIndex = ref(0)

function onExecute(item: TranslateSettingsItem) {
  if (item.type === 'provider') {
    openConfigModal(item.config)
  }
}

/** 当前编辑的配置类型（弹窗中判断表单布局） */
const editingType = computed(() => {
  if (isCreating.value) return 'ai'
  const config = translateConfig.configs.find((c) => c.id === editingConfigId.value)
  return config?.type || 'ai'
})
</script>
