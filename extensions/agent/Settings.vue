<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" shortcut-id="agent" />

    <!-- 搜索提供商编辑弹窗 -->
    <BaseDialog
      v-if="showSearchDialog"
      :title="t('agent.editSearchProvider')"
      variant="form"
      size="md"
      show-footer
      :ok-label="t('common.save')"
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
      :title="t('agent.systemPrompt')"
      variant="form"
      size="md"
      show-footer
      :ok-label="t('common.save')"
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
          :placeholder="t('agent.systemPromptPlaceholder')"
        />
      </div>
      <template #footer-start>
        <BaseButton
          v-if="systemPromptText !== DEFAULT_SYSTEM_PROMPT"
          variant="danger"
          @click="resetSystemPrompt"
          >{{ t('agent.reset') }}</BaseButton
        >
      </template>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  config as agentConfig,
  DEFAULT_SYSTEM_PROMPT,
  type SearchProviderConfig,
  updateSearchProvider,
} from './config'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { useShortcutConfig } from '@/composables/useShortcutConfig'
import type { SettingItem } from '@/types/settings'

const appStore = useAppStore()

const { value: agentShortcutValue, update: handleAgentShortcutChange } = useShortcutConfig(
  'agent',
  'Alt+A',
)

/// 系统提示词预览（折叠换行），空则「未设置」
const systemPromptPreview = computed(() => {
  const text = agentConfig.systemPrompt.trim()
  if (!text) return t('agent.notSet')
  return text.replace(/\s+/g, ' ').trim()
})

const allItems = computed<SettingItem[]>(() => [
  {
    id: 'agent-shortcut',
    title: t('settings.shortcut'),
    type: 'shortcut',
    group: t('common.group.general'),
    value: agentShortcutValue.value,
    update: handleAgentShortcutChange,
  },
  {
    id: 'ai-providers-link',
    title: 'AI',
    subtitle: t('agent.configureInAiProviders'),
    type: 'action',
    group: t('agent.group.provider'),
    action: () => appStore.setActiveExtension('ai-providers'),
  },
  {
    id: 'search-provider',
    title: 'Tavily',
    subtitle: agentConfig.searchProvider.apiKey ? t('agent.configured') : t('agent.notConfigured'),
    type: 'action',
    group: t('agent.group.provider'),
    action: () => openSearchModal(agentConfig.searchProvider),
  },
  {
    id: 'system-prompt',
    title: t('agent.systemPrompt'),
    subtitle: systemPromptPreview.value,
    type: 'action',
    group: t('agent.group.advanced'),
    action: openSystemPromptDialog,
  },
])

// ─── 搜索提供商编辑（固定 Tavily 单项，不可新增/删除）───
const showSearchDialog = ref(false)
const searchKeyVisible = ref(false)
const searchForm = ref<{ apiKey: string }>({ apiKey: '' })

function openSearchModal(cfg: SearchProviderConfig) {
  searchForm.value = { apiKey: cfg.apiKey }
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

function resetSystemPrompt() {
  systemPromptText.value = DEFAULT_SYSTEM_PROMPT
}
</script>
