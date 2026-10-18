<template>
  <div class="flex-col-full">
    <BaseSettingsList :items="items" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { config as awakeConfig } from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const isEnabled = ref(false)
const isPrivacy = ref(false)
const appStore = useAppStore()
let unlistenEnabled: (() => void) | undefined
let unlistenPrivacy: (() => void) | undefined

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isAwakeEnabled)
  } catch (e) {
    console.error('Failed to check awake status:', e)
  }
}

const checkPrivacy = async () => {
  try {
    isPrivacy.value = await invoke<boolean>(CMD.isAwakePrivacy)
  } catch (e) {
    console.error('Failed to check awake privacy status:', e)
  }
}

const toggleAwake = async (next: boolean) => {
  try {
    // 先 invoke 再写 config：成功路径由 watch 回声幂等处理，
    // 失败（授权取消等）时 config.enabled 未被置位，状态不漂移
    await invoke(CMD.setAwakeEnabled, { enabled: next })
    isEnabled.value = next
    awakeConfig.enabled = next
  } catch (e) {
    appStore.showStatus(`${t('common.operationFailed')}: ${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  }
}

const togglePrivacy = async (next: boolean) => {
  // 保持系统唤醒与屏幕是两个维度：开盖零亮度依赖它，但不自动开启它。
  // 未开启时本地先提示，Rust 侧仍有同样的硬校验（菜单入口也走同一检查）。
  if (next && !isEnabled.value) {
    appStore.showStatus(t('awake.privacyNeedsAwake'), { duration: 4000, kind: 'error' })
    return
  }
  try {
    isPrivacy.value = await invoke<boolean>(CMD.setAwakePrivacy, { enabled: next })
  } catch (e) {
    appStore.showStatus(`${t('common.operationFailed')}: ${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  }
}

onMounted(async () => {
  checkStatus()
  checkPrivacy()
  // 菜单栏/设置页操作改状态后 Rust emit 同步面板显示（与 proxy-enabled 同模式）
  unlistenEnabled = await listen<boolean>('awake-enabled', (e) => {
    isEnabled.value = e.payload
    // disarm 会一并撤销零亮度；Rust 的 awake-privacy 事件也可能先到/后到
    if (!e.payload) isPrivacy.value = false
  })
  unlistenPrivacy = await listen<boolean>('awake-privacy', (e) => {
    isPrivacy.value = e.payload
  })
})

onUnmounted(() => {
  unlistenEnabled?.()
  unlistenPrivacy?.()
})

const items = computed<SettingItem[]>(() => {
  const base: SettingItem[] = [
    {
      id: 'awake',
      title: t('awake.enable'),
      subtitle: t('awake.enableHint'),
      type: 'toggle',
      value: isEnabled.value,
      update: toggleAwake,
      group: t('awake.group.system'),
    },
    {
      id: 'menubar',
      title: t('awake.menubarToggle'),
      type: 'toggle',
      value: awakeConfig.menubarToggleVisible,
      update: (visible: boolean | string | number) => {
        awakeConfig.menubarToggleVisible = visible as boolean
      },
      group: t('awake.group.system'),
    },
  ]
  // 屏幕项依赖保持系统唤醒；未开启时不展示，避免“设置了但无效”的歧义。
  if (!isEnabled.value) return base
  return [
    ...base,
    {
      id: 'screenOpen',
      title: t('awake.screenOpen'),
      subtitle: t('awake.screenOpenHint'),
      type: 'select',
      value: isPrivacy.value ? 'zero' : 'auto',
      options: [
        { label: t('awake.screenOpen.auto'), value: 'auto' },
        { label: t('awake.screenOpen.zero'), value: 'zero' },
      ],
      update: (value: string | number) => {
        void togglePrivacy(value === 'zero')
      },
      group: t('awake.group.screen'),
    },
    {
      id: 'screenClosed',
      title: t('awake.screenClosed'),
      subtitle: t('awake.screenClosedHint'),
      type: 'select',
      value: awakeConfig.screenPolicy,
      options: [
        { label: t('awake.screenClosed.sleep'), value: 'sleep' },
        { label: t('awake.screenClosed.dim'), value: 'dim' },
      ],
      update: (value: string | number) => {
        awakeConfig.screenPolicy = value as typeof awakeConfig.screenPolicy
      },
      group: t('awake.group.screen'),
    },
  ]
})
</script>
