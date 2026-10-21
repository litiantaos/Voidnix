<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items">
      <template #trailing-token>
        <BaseInput
          :model-value="config.briaToken"
          :type="tokenVisible ? 'text' : 'password'"
          :placeholder="t('image.briaTokenPlaceholder')"
          class="w-64"
          @update:model-value="config.briaToken = $event"
        >
          <template #suffix>
            <BaseButton
              variant="ghost"
              :icon="tokenVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
              class="!text-muted !px-1 !shrink-0 !h-auto"
              @click.stop="tokenVisible = !tokenVisible"
            />
          </template>
        </BaseInput>
      </template>
    </BaseSettingsList>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import type { SettingItem } from '@/types/settings'
import { config, type RemoveBgEngine } from './config'

const tokenVisible = ref(false)

const items = computed<SettingItem[]>(() => {
  const group = t('image.group.removeBg')
  const list: SettingItem[] = [
    {
      id: 'engine',
      title: t('image.engine'),
      subtitle: t('image.engineHint'),
      type: 'select',
      value: config.removeBgEngine,
      options: [
        { label: t('image.engine.vision'), value: 'vision' },
        { label: t('image.engine.bria'), value: 'bria' },
      ],
      update: (v) => {
        config.removeBgEngine = v as RemoveBgEngine
      },
      group,
    },
  ]

  if (config.removeBgEngine === 'bria') {
    list.push(
      {
        id: 'maxEdge',
        title: t('image.cloudMaxEdge'),
        subtitle: t('image.cloudMaxEdgeHint'),
        type: 'select',
        value: config.briaMaxEdge,
        options: [
          { label: t('image.cloudMaxEdge.original'), value: 0 },
          { label: '4096', value: 4096 },
          { label: '2048', value: 2048 },
          { label: '1024', value: 1024 },
        ],
        update: (v) => {
          config.briaMaxEdge = Number(v)
        },
        group,
      },
      {
        id: 'token',
        title: t('image.briaToken'),
        subtitle: t('image.briaTokenHint'),
        type: 'custom',
        group,
      },
      {
        id: 'console',
        title: t('image.briaConsole'),
        subtitle: t('image.briaFreeHint'),
        type: 'button',
        label: t('image.open'),
        action: () => {
          open('https://platform.bria.ai/organization-management/api-keys').catch(() => {
            /* 打开失败静默：配置可手动访问官网 */
          })
        },
        group,
      },
    )
  }

  return list
})
</script>
