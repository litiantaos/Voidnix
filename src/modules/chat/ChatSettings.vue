<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSettingsStore, type ChatApiConfig } from '@/stores/settings'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import { useSettingsInput } from '@/composables/useSettingsInput'

const settings = useSettingsStore()
const { setShortcutRef, shortcutRefs } = useSettingsInput()

const SHORTCUT_ITEM_ID = 'chat-shortcut'

/** 从 URL 中提取倒数第一个点和倒数第二个点之间的文本，转大写 */
function providerLabel(url: string, fallback: string): string {
  if (!url) return fallback
  try {
    const parts = new URL(url).hostname.split('.')
    if (parts.length >= 2) return parts[parts.length - 2].toUpperCase()
    return parts[0].toUpperCase()
  } catch {
    return fallback
  }
}

const handleChatShortcutChange = async (val: string) => {
  await settings.setChatShortcut(val)
}

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
  } else {
    if (!editingConfigId.value) return
    await settings.updateChatConfig(editingConfigId.value, {
      endpoint,
      apiKey: modalForm.value.apiKey,
      models,
    })
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
  } else {
    // 委托 useSettingsInput 处理快捷键聚焦
    const ref = shortcutRefs.value[`si-${SHORTCUT_ITEM_ID}`]
    if (ref) {
      ref.focus()
      ref.startRecording()
    }
  }
}
</script>

<template>
  <div class="pb-4 flex flex-col h-full">
    <BaseList
      :items="allItems"
      v-model:selected-index="selectedIndex"
      keyboard-navigation
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
            @click.stop="addAndEdit"
          >
            <div class="i-ri-add-line text-sm" />
          </BaseButton>
        </div>
      </template>

      <template #item="{ item, selected, hoverable: h, setRef, select }">
        <!-- 快捷键 -->
        <BaseListItem
          v-if="item.type === 'shortcut'"
          :ref="setRef"
          :id="`si-${SHORTCUT_ITEM_ID}`"
          title="启动快捷键"
          :selected="selected"
          :hoverable="h"
          @click="select"
        >
          <template #trailing>
            <ShortcutInput
              :ref="(el: any) => setShortcutRef(`si-${SHORTCUT_ITEM_ID}`, el)"
              :model-value="settings.chatShortcut"
              @update:model-value="handleChatShortcutChange"
            />
          </template>
        </BaseListItem>

        <!-- 提供商 -->
        <BaseListItem
          v-else
          :ref="setRef"
          :title="providerLabel(item.config.endpoint, 'API')"
          :subtitle="
            item.config.models.filter(Boolean).join('、') || '未配置模型'
          "
          :selected="selected"
          :hoverable="h"
          @click="select"
          @dblclick="openConfigModal(item.config)"
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
          <BaseInput
            v-model="modalForm.endpoint"
            placeholder="https://api.openai.com/v1"
          />
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
                class="i-ri-eye-line text-black/35 shrink-0 cursor-pointer hover:text-black/60"
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
              <BaseInput
                v-model="modalForm.models[index]"
                placeholder="gpt-5"
                class="flex-1"
              />
              <BaseButton
                v-if="index > 0"
                size="icon"
                class="text-red-500"
                @click="removeModel(index)"
              >
                <div class="i-ri-close-line" />
              </BaseButton>
              <BaseButton v-else size="icon" @click="addModel">
                <div class="i-ri-add-line" />
              </BaseButton>
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
