<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" shortcut-id="agent">
      <template #group-title="{ group }">
        <div flex items="center">
          <span>{{ group }}</span>
          <BaseButton
            v-if="group === '模型提供商'"
            class="ml-auto"
            icon="i-ri-add-line"
            @click.stop="addAndEdit"
          />
        </div>
      </template>
    </BaseSettingsList>

    <!-- 编辑弹窗 -->
    <BaseDialog
      v-if="showConfigModal"
      :title="isCreating ? '添加模型提供商' : '编辑模型提供商'"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveConfigModal"
      @cancel="closeConfigModal"
    >
      <div flex="~ col" gap="3">
        <!-- API URL -->
        <div class="form-field">
          <span class="form-label">API URL</span>
          <BaseInput v-model="modalForm.endpoint" placeholder="https://api.openai.com/v1" />
        </div>

        <!-- API KEY -->
        <div class="form-field">
          <span class="form-label">API KEY</span>
          <BaseInput
            v-model="modalForm.apiKey"
            :type="passwordVisible ? 'text' : 'password'"
            placeholder="sk-..."
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

        <!-- 模型 -->
        <div class="form-field">
          <span class="form-label">模型</span>
          <div class="form-field">
            <div v-for="(_, index) in modalForm.models" :key="index" flex gap="1.5" items="center">
              <BaseInput v-model="modalForm.models[index]" placeholder="gpt-5" class="flex-1" />
              <BaseButton
                v-if="index > 0"
                class="text-danger"
                icon="i-ri-close-line"
                @click="removeModel(index)"
              />
              <BaseButton v-else icon="i-ri-add-line" @click="addModel" />
            </div>
          </div>
        </div>
      </div>

      <template #footer-start>
        <BaseButton
          v-if="!isCreating && agentConfig.aiProviders.length > 1"
          class="text-danger hover:text-danger/80"
          @click="deleteAndClose"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>

    <!-- 搜索提供商编辑弹窗 -->
    <BaseDialog
      v-if="showSearchDialog"
      title="编辑搜索提供商"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveSearchModal"
      @cancel="closeSearchModal"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">Tavily API Key</span>
          <BaseInput
            v-model="searchForm.apiKey"
            :type="searchKeyVisible ? 'text' : 'password'"
            placeholder="tvly-..."
          >
            <template #suffix>
              <BaseButton
                variant="ghost"
                :icon="searchKeyVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
                class="!text-muted !px-1 !shrink-0 !h-auto"
                @click.stop="searchKeyVisible = !searchKeyVisible"
              />
            </template>
          </BaseInput>
        </div>
      </div>
    </BaseDialog>

    <!-- 系统提示词弹窗 -->
    <BaseDialog
      v-if="showSystemPromptDialog"
      title="系统提示词"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveSystemPrompt"
      @cancel="showSystemPromptDialog = false"
    >
      <div class="form-field">
        <BaseTextarea
          v-model="systemPromptText"
          :rows="12"
          :max-height="0"
          :auto-resize="false"
          :submit-on-enter="false"
          placeholder="定义 Agent 角色、能力边界、工具使用规则、安全约束与输出风格"
        />
      </div>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  config as agentConfig,
  type AiProviderConfig,
  type SearchProviderConfig,
  addAiProvider,
  removeAiProvider,
  updateAiProvider,
  setActiveProviderModelKey,
  updateSearchProvider,
} from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { providerLabelFromUrl } from '@/utils/format'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import type { SettingItem } from '@/types/settings'

const { value: agentShortcutValue, update: handleAgentShortcutChange } = useShortcutConfig(
  'agent',
  'Alt+A',
)

interface ModalForm {
  endpoint: string
  apiKey: string
  models: string[]
}

const editingConfigId = ref('')
const isCreating = ref(false)
const showConfigModal = ref(false)
const modalForm = ref<ModalForm>({
  endpoint: '',
  apiKey: '',
  models: [],
})
const passwordVisible = ref(false)

function openConfigModal(config: AiProviderConfig) {
  editingConfigId.value = config.id
  isCreating.value = false
  modalForm.value = {
    endpoint: config.endpoint,
    apiKey: config.apiKey,
    models: config.models.length > 0 ? [...config.models] : [''],
  }
  passwordVisible.value = false
  showConfigModal.value = true
}

function openCreateModal() {
  editingConfigId.value = ''
  isCreating.value = true
  modalForm.value = {
    endpoint: '',
    apiKey: '',
    models: [''],
  }
  passwordVisible.value = false
  showConfigModal.value = true
}

function closeConfigModal() {
  showConfigModal.value = false
  editingConfigId.value = ''
  isCreating.value = false
}

function saveConfigModal() {
  const models = modalForm.value.models.filter((m) => m.trim())
  const endpoint = modalForm.value.endpoint.trim()

  if (isCreating.value) {
    const id = addAiProvider()
    updateAiProvider(id, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
    if (models.length > 0) {
      setActiveProviderModelKey(`${id}::${models[0]}`)
    }
  } else {
    if (!editingConfigId.value) return
    updateAiProvider(editingConfigId.value, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
    if (
      models.length > 0 &&
      !agentConfig.activeProviderModelKey.startsWith(`${editingConfigId.value}::`)
    ) {
      setActiveProviderModelKey(`${editingConfigId.value}::${models[0]}`)
    } else if (models.length > 0) {
      const currentModel = agentConfig.activeProviderModelKey.split('::').slice(1).join('::')
      if (!models.includes(currentModel)) {
        setActiveProviderModelKey(`${editingConfigId.value}::${models[0]}`)
      }
    }
  }
  closeConfigModal()
}

function addModel() {
  modalForm.value.models.push('')
}

function removeModel(index: number) {
  modalForm.value.models.splice(index, 1)
  if (modalForm.value.models.length === 0) {
    modalForm.value.models.push('')
  }
}

function deleteAndClose() {
  if (isCreating.value) {
    closeConfigModal()
    return
  }
  const id = editingConfigId.value
  if (id && agentConfig.aiProviders.length > 1) {
    closeConfigModal()
    removeAiProvider(id)
  }
}

function addAndEdit() {
  openCreateModal()
}

/// 系统提示词预览（折叠换行），空则「未设置」（截断交给 BaseListItem 的 truncate）
const systemPromptPreview = computed(() => {
  const text = agentConfig.systemPrompt.trim()
  if (!text) return '未设置'
  return text.replace(/\s+/g, ' ').trim()
})

const allItems = computed<SettingItem[]>(() => [
  {
    id: 'agent-shortcut',
    title: '启动快捷键',
    type: 'shortcut',
    group: '通用',
    value: agentShortcutValue.value,
    update: handleAgentShortcutChange,
  },
  ...agentConfig.aiProviders.map((c) => ({
    id: `provider-${c.id}`,
    title: providerLabelFromUrl(c.endpoint, '默认提供商'),
    subtitle: c.models.filter(Boolean).join('、') || '未配置',
    type: 'action' as const,
    group: '模型提供商',
    action: () => openConfigModal(c),
  })),
  {
    id: 'search-provider',
    title: 'Tavily',
    subtitle: agentConfig.searchProvider.apiKey ? '已配置 Key' : '无 Key',
    type: 'action',
    group: '搜索提供商',
    action: () => openSearchModal(agentConfig.searchProvider),
  },
  {
    id: 'system-prompt',
    title: '系统提示词',
    subtitle: systemPromptPreview.value,
    type: 'action',
    group: 'Agent 配置',
    action: openSystemPromptDialog,
  },
])

// ─── 搜索提供商编辑（固定 Tavily 单项，不可新增/删除）───
const showSearchDialog = ref(false)
const searchKeyVisible = ref(false)
const searchForm = ref<{ apiKey: string }>({ apiKey: '' })

function openSearchModal(config: SearchProviderConfig) {
  searchForm.value = { apiKey: config.apiKey }
  searchKeyVisible.value = false
  showSearchDialog.value = true
}

function closeSearchModal() {
  showSearchDialog.value = false
}

async function saveSearchModal() {
  await updateSearchProvider({ apiKey: searchForm.value.apiKey.trim() })
  closeSearchModal()
}

// ─── 系统提示词 ───
const showSystemPromptDialog = ref(false)
const systemPromptText = ref('')

function openSystemPromptDialog() {
  systemPromptText.value = agentConfig.systemPrompt
  showSystemPromptDialog.value = true
}

async function saveSystemPrompt() {
  agentConfig.systemPrompt = systemPromptText.value.trim()
  showSystemPromptDialog.value = false
}
</script>
