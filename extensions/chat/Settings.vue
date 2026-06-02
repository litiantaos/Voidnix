<template>
  <div class="pb-4 flex flex-col h-full">
    <BaseList
      :items="allItems"
      v-model:selected-index="selectedIndex"
      :group-field="(item: ChatSettingsItem) => item.group"
      :group-title="(g: string) => g"
      @execute="(item: ChatSettingsItem) => onExecute(item)"
    >
      <template #group-title="{ group }">
        <div class="flex items-center">
          <span>{{ group }}</span>
          <BaseButton
            v-if="group === '提供商'"
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
              :model-value="chatShortcutValue"
              shortcut-id="chat"
              @update:model-value="handleChatShortcutChange"
            />
          </template>
        </BaseListItem>

        <!-- 提供商 -->
        <BaseListItem
          v-else
          :ref="setRef"
          :title="providerLabelFromUrl(item.config.endpoint, '默认提供商')"
          :subtitle="item.config.models.filter(Boolean).join('、') || '未配置'"
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
      <div class="flex flex-col gap-4">
        <!-- API URL -->
        <div class="flex flex-col gap-1.5">
          <span class="text-xs text-tx-faint font-medium">API URL</span>
          <BaseInput v-model="modalForm.endpoint" placeholder="https://api.openai.com/v1" />
        </div>

        <!-- API KEY -->
        <div class="flex flex-col gap-1.5">
          <span class="text-xs text-tx-faint font-medium">API KEY</span>
          <BaseInput
            v-model="modalForm.apiKey"
            :type="passwordVisible ? 'text' : 'password'"
            placeholder="sk-..."
          >
            <template #suffix>
              <button
                class="i-ri-eye-line text-black/35 shrink-0 hover:text-black/60"
                :class="{ 'i-ri-eye-off-line': passwordVisible }"
                @click.stop="passwordVisible = !passwordVisible"
              />
            </template>
          </BaseInput>
        </div>

        <!-- 模型 -->
        <div class="flex flex-col gap-1.5">
          <span class="text-xs text-tx-faint font-medium">模型</span>
          <div class="flex flex-col gap-1.5">
            <div
              v-for="(_, index) in modalForm.models"
              :key="index"
              class="flex gap-1.5 items-center"
            >
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
          v-if="!isCreating && settings.chatConfigs.length > 1"
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
import { useSettingsStore, type ChatApiConfig } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { providerLabelFromUrl } from '@/utils/provider'
import { useSettingsInput } from '@/composables/useSettingsInput'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const settings = useSettingsStore()
useSettingsInput()

const SHORTCUT_ITEM_ID = 'chat-shortcut'

const { value: chatShortcutValue, update: handleChatShortcutChange } = useShortcutConfig(
  'chat',
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

function openConfigModal(config: ChatApiConfig) {
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
    const id = await settings.addChatConfig()
    await settings.updateChatConfig(id, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
    if (models.length > 0) {
      await settings.setActiveModelKey(`${id}::${models[0]}`)
    }
  } else {
    if (!editingConfigId.value) return
    await settings.updateChatConfig(editingConfigId.value, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
    if (models.length > 0 && !settings.activeModelKey.startsWith(`${editingConfigId.value}::`)) {
      await settings.setActiveModelKey(`${editingConfigId.value}::${models[0]}`)
    } else if (models.length > 0) {
      const currentModel = settings.activeModelKey.split('::').slice(1).join('::')
      if (!models.includes(currentModel)) {
        await settings.setActiveModelKey(`${editingConfigId.value}::${models[0]}`)
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
  if (id && settings.chatConfigs.length > 1) {
    closeConfigModal()
    settings.removeChatConfig(id)
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
  config: ChatApiConfig
}

type ChatSettingsItem = ShortcutItem | ProviderItem

const allItems = computed<ChatSettingsItem[]>(() => [
  { type: 'shortcut', group: '通用' },
  ...settings.chatConfigs.map((c) => ({
    type: 'provider' as const,
    group: '提供商',
    config: c,
  })),
])

const selectedIndex = ref(0)

function onExecute(item: ChatSettingsItem) {
  if (item.type === 'provider') {
    openConfigModal(item.config)
  }
}
</script>
