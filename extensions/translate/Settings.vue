<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" shortcut-id="translate">
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
    </BaseSettingsList>

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
                <BaseButton
                  variant="ghost"
                  :icon="passwordVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
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
import {
  type TranslateApiConfig,
  config as translateConfig,
  addTranslateConfig,
  updateTranslateConfig,
  removeTranslateConfig,
} from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { providerLabelFromUrl } from '@/utils/format'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import type { SettingItem } from '@/types/settings'

const { value: translateShortcutValue, update: handleTranslateShortcutChange } = useShortcutConfig(
  'translate',
  'Alt+T',
)

const handleTargetLangChange = async (val: string | number) => {
  translateConfig.targetLang = String(val)
}

function providerLabel(config: TranslateApiConfig): string {
  if (config.type === 'youdao') return '有道翻译'
  return providerLabelFromUrl(config.endpoint, '翻译')
}

function providerSubtitle(c: TranslateApiConfig): string {
  if (c.type === 'ai') return c.models.filter(Boolean).join('、') || '未配置模型'
  return c.appKey ? '已配置' : '未配置'
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
  return translateConfig.configs.length > 1
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

const allItems = computed<SettingItem[]>(() => [
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
  ...translateConfig.configs.map((c) => ({
    id: `provider-${c.id}`,
    title: providerLabel(c),
    subtitle: providerSubtitle(c),
    type: 'action' as const,
    group: '翻译服务',
    action: () => openConfigModal(c),
  })),
])

/** 当前编辑的配置类型（弹窗中判断表单布局） */
const editingType = computed(() => {
  if (isCreating.value) return 'ai'
  const config = translateConfig.configs.find((c) => c.id === editingConfigId.value)
  return config?.type || 'ai'
})
</script>
