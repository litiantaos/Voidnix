<template>
  <div class="flex-col-full-pb">
    <!-- M-ag2：BOUNDS 数值项越界警告（用户手改 config.json 时提示；Rust 端 clamp 兜底） -->
    <div
      v-if="outOfBoundsItems.length"
      p="x-3 y-2"
      m="b-2"
      rounded="md"
      bg="red-50"
      text="xs red-500"
    >
      <div font="medium">以下配置超出安全边界，Rust 端会自动 clamp：</div>
      <div v-for="it in outOfBoundsItems" :key="it.key" m="t-0.5">
        · {{ it.label }}：当前 {{ it.value }}（允许 {{ it.floor }}–{{ it.cap }}）
      </div>
    </div>
    <BaseList
      :items="allItems"
      v-model:selected-index="selectedIndex"
      :group-field="(item: ChatSettingsItem) => item.group"
      :group-title="(g: string) => g"
      @execute="(item: ChatSettingsItem) => onExecute(item)"
    >
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
              :model-value="agentShortcutValue"
              shortcut-id="agent"
              @update:model-value="handleAgentShortcutChange"
            />
          </template>
        </BaseListItem>

        <!-- 模型提供商 -->
        <BaseListItem
          v-else-if="item.type === 'provider'"
          :ref="setRef"
          :title="providerLabelFromUrl(item.config.endpoint, '默认提供商')"
          :subtitle="item.config.models.filter(Boolean).join('、') || '未配置'"
          :selected="selected"
        />

        <!-- 搜索提供商 -->
        <BaseListItem
          v-else-if="item.type === 'searchProvider'"
          :ref="setRef"
          :title="searchProviderLabel(item.config)"
          :subtitle="item.config.apiKey ? '已配置 Key' : '无 Key'"
          :selected="selected"
        />

        <!-- 命令白名单 -->
        <BaseListItem
          v-else-if="item.type === 'whitelist'"
          :ref="setRef"
          title="命令白名单"
          :subtitle="`${agentConfig.trustedCommands.length} 个命令免审批`"
          :selected="selected"
        />

        <!-- 系统提示词 -->
        <BaseListItem
          v-else
          :ref="setRef"
          title="系统提示词"
          :subtitle="agentConfig.systemPrompt ? '已配置' : '默认 harness'"
          :selected="selected"
        />
      </template>
    </BaseList>

    <!-- 编辑弹窗 -->
    <BaseDialog
      v-if="showConfigModal"
      :title="isCreating ? '添加提供商' : '编辑提供商'"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveConfigModal"
      @cancel="closeConfigModal"
    >
      <div flex="~ col" gap="4">
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

        <!-- 模型 -->
        <div class="form-field">
          <span class="form-label">模型</span>
          <div class="form-field">
            <div v-for="(_, index) in modalForm.models" :key="index" flex gap="1.5" items="center">
              <BaseInput v-model="modalForm.models[index]" placeholder="gpt-5" class="flex-1" />
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
      </div>

      <template #footer-start>
        <BaseButton
          v-if="!isCreating && settings.aiProviders.length > 1"
          class="text-red-500 hover:text-red-600"
          @click="deleteAndClose"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>

    <!-- 搜索提供商编辑弹窗 -->
    <BaseDialog
      v-if="showSearchDialog"
      title="搜索提供商"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveSearchModal"
      @cancel="closeSearchModal"
    >
      <div flex="~ col" gap="4">
        <div class="form-field">
          <span class="form-label">Tavily API Key</span>
          <BaseInput
            v-model="searchForm.apiKey"
            :type="searchKeyVisible ? 'text' : 'password'"
            placeholder="tvly-..."
          >
            <template #suffix>
              <button
                class="i-ri-eye-line hover:text-black/60"
                text="black/35"
                shrink="0"
                :class="{ 'i-ri-eye-off-line': searchKeyVisible }"
                @click.stop="searchKeyVisible = !searchKeyVisible"
              />
            </template>
          </BaseInput>
          <p text="xs tx-subtle" m="t-1">免费注册：tavily.com，1000 次/月，无需信用卡。</p>
        </div>
      </div>
    </BaseDialog>

    <!-- 命令白名单弹窗 -->
    <BaseDialog
      v-if="showWhitelistDialog"
      title="命令白名单"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveWhitelist"
      @cancel="showWhitelistDialog = false"
    >
      <div class="form-field">
        <span class="form-label">免审批命令（每行一个，basename 形式如 `git`）</span>
        <BaseTextarea
          v-model="whitelistText"
          :rows="10"
          :max-height="0"
          :submit-on-enter="false"
          placeholder="ls&#10;cat&#10;git"
        />
        <p text="xs tx-subtle" m="t-1">
          列在此处的命令直接执行无需审批。点击审批弹窗的「执行并信任」也会自动追加。
        </p>
        <!-- trusted ∩ forbidden floor 冲突警告（B2）：被底线覆盖，加进 trusted 无效 -->
        <p v-if="conflictTrusted.length" text="xs text-red-500" m="t-1">
          以下命令在硬禁底线中，加进白名单无效（Rust 端并集兜底）：
          {{ conflictTrusted.join('、') }}
        </p>
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
        <span class="form-label">追加到默认 harness 之后（用户自定义指令）</span>
        <BaseTextarea
          v-model="systemPromptText"
          :rows="8"
          :max-height="0"
          :submit-on-enter="false"
          placeholder="例如：始终用英文回答；优先使用 ripgrep 而非 grep；当前工作目录是 ~/Projects/myapp，使用 pnpm 而非 npm"
        />
        <p text="xs tx-subtle" m="t-1">
          留空使用纯默认 harness（描述工具规则、安全约束、输出风格）。
        </p>
      </div>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSettingsStore, type AiProviderConfig } from '@/stores/settings'
import {
  config as agentConfig,
  BOUNDS,
  type SearchProviderConfig,
  updateSearchProvider,
} from './config'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { providerLabelFromUrl } from '@/utils/format'
import { useSettingsInput } from '@/composables/useSettingsInput'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const settings = useSettingsStore()
useSettingsInput()

const SHORTCUT_ITEM_ID = 'agent-shortcut'

const { value: agentShortcutValue, update: handleAgentShortcutChange } = useShortcutConfig(
  'agent',
  'CommandOrControl+Shift+A',
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

async function saveConfigModal() {
  const models = modalForm.value.models.filter((m) => m.trim())
  const endpoint = modalForm.value.endpoint.trim()

  if (isCreating.value) {
    const id = await settings.addAiProvider()
    await settings.updateAiProvider(id, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
    if (models.length > 0) {
      await settings.setActiveProviderModelKey(`${id}::${models[0]}`)
    }
  } else {
    if (!editingConfigId.value) return
    await settings.updateAiProvider(editingConfigId.value, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
    if (
      models.length > 0 &&
      !settings.activeProviderModelKey.startsWith(`${editingConfigId.value}::`)
    ) {
      await settings.setActiveProviderModelKey(`${editingConfigId.value}::${models[0]}`)
    } else if (models.length > 0) {
      const currentModel = settings.activeProviderModelKey.split('::').slice(1).join('::')
      if (!models.includes(currentModel)) {
        await settings.setActiveProviderModelKey(`${editingConfigId.value}::${models[0]}`)
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
  if (id && settings.aiProviders.length > 1) {
    closeConfigModal()
    settings.removeAiProvider(id)
  }
}

function addAndEdit() {
  openCreateModal()
}

interface ShortcutItem {
  type: 'shortcut'
  group: string
}

interface ProviderItem {
  type: 'provider'
  group: string
  config: AiProviderConfig
}

interface SearchProviderItem {
  type: 'searchProvider'
  group: string
  config: SearchProviderConfig
}

interface AgentBehaviorItem {
  type: 'whitelist' | 'systemPrompt'
  group: string
}

type ChatSettingsItem = ShortcutItem | ProviderItem | SearchProviderItem | AgentBehaviorItem

const allItems = computed<ChatSettingsItem[]>(() => [
  { type: 'shortcut', group: '通用' },
  ...settings.aiProviders.map((c) => ({
    type: 'provider' as const,
    group: '模型提供商',
    config: c,
  })),
  ...agentConfig.searchProviders.map((c) => ({
    type: 'searchProvider' as const,
    group: '搜索提供商',
    config: c,
  })),
  { type: 'whitelist', group: 'Agent 配置' },
  { type: 'systemPrompt', group: 'Agent 配置' },
])

const selectedIndex = ref(0)

// ─── 搜索提供商编辑（默认固定一项 Tavily，不可新增/删除）───
const showSearchDialog = ref(false)
const editingSearchId = ref('')
const searchKeyVisible = ref(false)
const searchForm = ref<{ apiKey: string }>({ apiKey: '' })

function searchProviderLabel(_c: SearchProviderConfig): string {
  return 'Tavily'
}

function openSearchModal(config: SearchProviderConfig) {
  editingSearchId.value = config.id
  searchForm.value = { apiKey: config.apiKey }
  searchKeyVisible.value = false
  showSearchDialog.value = true
}

function closeSearchModal() {
  showSearchDialog.value = false
  editingSearchId.value = ''
}

async function saveSearchModal() {
  if (!editingSearchId.value) return
  await updateSearchProvider(editingSearchId.value, {
    apiKey: searchForm.value.apiKey.trim(),
  })
  closeSearchModal()
}

function onExecute(item: ChatSettingsItem) {
  if (item.type === 'provider') {
    openConfigModal(item.config)
  } else if (item.type === 'searchProvider') {
    openSearchModal(item.config)
  } else if (item.type === 'whitelist') {
    openWhitelistDialog()
  } else if (item.type === 'systemPrompt') {
    openSystemPromptDialog()
  }
}

// ─── 命令白名单 ───
const showWhitelistDialog = ref(false)
const whitelistText = ref('')

function openWhitelistDialog() {
  whitelistText.value = agentConfig.trustedCommands.join('\n')
  showWhitelistDialog.value = true
}

// trusted ∩ forbidden floor 冲突（B2）：编辑中的白名单与硬禁底线交集
const conflictTrusted = computed(() => {
  const floor: readonly string[] = BOUNDS.forbiddenCommands.floor
  return whitelistText.value
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s && floor.includes(s))
})

// M-ag2：BOUNDS 数值项越界检测（消费 BOUNDS.max* tuple，用于 UI 警告；Rust 端 clamp 兜底）
const outOfBoundsItems = computed(() => {
  const checks: Array<{
    key: string
    label: string
    value: number
    floor: number
    cap: number
  }> = [
    {
      key: 'maxTurns',
      label: '最大轮次',
      value: agentConfig.maxTurns,
      floor: BOUNDS.maxTurns.floor,
      cap: BOUNDS.maxTurns.cap,
    },
    {
      key: 'maxCpuSeconds',
      label: 'CPU 上限',
      value: agentConfig.maxCpuSeconds,
      floor: BOUNDS.maxCpuSeconds.floor,
      cap: BOUNDS.maxCpuSeconds.cap,
    },
    {
      key: 'maxMemoryMb',
      label: '内存上限',
      value: agentConfig.maxMemoryMb,
      floor: BOUNDS.maxMemoryMb.floor,
      cap: BOUNDS.maxMemoryMb.cap,
    },
    {
      key: 'maxOpenFiles',
      label: '文件描述符',
      value: agentConfig.maxOpenFiles,
      floor: BOUNDS.maxOpenFiles.floor,
      cap: BOUNDS.maxOpenFiles.cap,
    },
    {
      key: 'executionTimeout',
      label: '执行超时',
      value: agentConfig.executionTimeout,
      floor: BOUNDS.executionTimeout.floor,
      cap: BOUNDS.executionTimeout.cap,
    },
    {
      key: 'maxOutputBytes',
      label: '输出上限',
      value: agentConfig.maxOutputBytes,
      floor: BOUNDS.maxOutputBytes.floor,
      cap: BOUNDS.maxOutputBytes.cap,
    },
  ]
  return checks.filter((c) => c.value < c.floor || c.value > c.cap)
})

async function saveWhitelist() {
  const list = whitelistText.value
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean)
  agentConfig.trustedCommands = list
  showWhitelistDialog.value = false
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
