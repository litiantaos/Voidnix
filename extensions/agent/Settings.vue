<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" shortcut-id="agent" />

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
import { config as agentConfig, type SearchProviderConfig, updateSearchProvider } from './config'
import { useAppStore } from '@/stores/app'
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
  {
    id: 'ai-providers-link',
    title: 'AI',
    subtitle: '在「AI 提供商」中配置',
    type: 'action',
    group: '提供商',
    action: () => appStore.setActiveModule('ai-providers'),
  },
  {
    id: 'search-provider',
    title: 'Tavily',
    subtitle: agentConfig.searchProvider.apiKey ? '已配置' : '未配置',
    type: 'action',
    group: '提供商',
    action: () => openSearchModal(agentConfig.searchProvider),
  },
  {
    id: 'system-prompt',
    title: '系统提示词',
    subtitle: systemPromptPreview.value,
    type: 'action',
    group: '高级',
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
</script>
