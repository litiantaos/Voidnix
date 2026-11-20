<template>
  <BaseDialog
    :title="t('settings.checkUpdate')"
    :ok-label="okLabel"
    :cancel-label="cancelLabel"
    :show-cancel="showCancel"
    :close-on-confirm="closesOnConfirm"
    size="sm"
    @confirm="onConfirm"
    @cancel="emit('close')"
  >
    <div flex="~ col" gap="3">
      <!-- 检查中：文案 + 不定进度条（统一 BaseProgress，无分母不伪造百分比） -->
      <div v-if="updateStore.checking" flex="~ col" gap="1.5">
        <div text="xs secondary">{{ t('updateDialog.checking') }}</div>
        <BaseProgress :value="null" />
      </div>

      <!-- 检查完成、无更新：失败错误 / 已是最新版本 -->
      <template v-else-if="!updateStore.info">
        <p v-if="updateStore.error" text="xs danger">{{ updateStore.error }}</p>
        <p v-else text="xs secondary">
          {{
            updateStore.currentVersion
              ? t('settings.upToDate', { version: updateStore.currentVersion })
              : t('updateDialog.upToDate')
          }}
        </p>
      </template>

      <!-- 发现新版本 -->
      <template v-else>
        <div text="xs secondary" flex gap="2" items="center">
          <span>{{ t('updateDialog.newVersionFound') }}</span>
          <span font="mono">v{{ updateStore.info.currentVersion }}</span>
          <span class="i-ri-arrow-right-line" text="muted"></span>
          <span text="accent" font="medium mono">v{{ updateStore.info.newVersion }}</span>
        </div>

        <p v-if="updateStore.info.body" text="xs secondary" leading="relaxed" line-clamp="4">
          {{ updateStore.info.body }}
        </p>

        <!-- 下载进度：统一 BaseProgress；百分比紧贴条尾（与任务行同构），无分母时走不定进度 -->
        <div v-if="updateStore.downloading || updateStore.downloaded" flex items="center" gap="2">
          <div flex-1 min-w="0">
            <BaseProgress :value="pct > 0 ? pct : null" />
          </div>
          <span v-if="pct > 0" text="xs muted" font="mono" shrink="0">{{ pct }}%</span>
        </div>

        <p v-if="updateStore.error" text="xs danger">{{ updateStore.error }}</p>
      </template>
    </div>
  </BaseDialog>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { relaunch } from '@tauri-apps/plugin-process'
import BaseDialog from './BaseDialog.vue'
import BaseProgress from './BaseProgress.vue'
import { t } from '@/runtime/i18n'
import { useUpdateStore } from '@/stores/update'

const emit = defineEmits<{ close: [] }>()

const updateStore = useUpdateStore()
const installing = ref(false)

const pct = computed(() => Math.round((updateStore.progress ?? 0) * 100))

const hasUpdate = computed(() => !!updateStore.info)
const failed = computed(() => !updateStore.info && !!updateStore.error)

const okLabel = computed(() => {
  if (updateStore.checking) return t('common.cancel')
  if (failed.value) return t('updateDialog.retry')
  if (!updateStore.info) return t('settings.updateOK')
  if (updateStore.downloading) return t('settings.downloading')
  if (updateStore.downloaded)
    return installing.value ? t('updateDialog.installing') : t('updateDialog.installNow')
  return t('settings.downloadAndInstall')
})

const cancelLabel = computed(() => (failed.value ? t('common.cancel') : t('updateDialog.later')))

// 双按钮仅在「有更新（稍后）/ 失败（取消）」形态；检查中（取消）/ 无更新（好的）单按钮
const showCancel = computed(() => hasUpdate.value || failed.value)

// 确认即关窗：检查中取消、无更新好的；重试与下载/安装由 confirm handler 自管（弹窗保持）
const closesOnConfirm = computed(() => updateStore.checking || (!updateStore.info && !failed.value))

async function onConfirm() {
  if (updateStore.checking) {
    // 关窗不打断检查：结果后台落地（发现更新时搜索栏角标出现）
    updateStore.closeDialog()
    return
  }
  if (!updateStore.info) {
    if (failed.value) {
      updateStore.startCheck()
      return
    }
    updateStore.closeDialog()
    return
  }
  if (updateStore.downloaded) {
    installing.value = true
    try {
      await updateStore.install()
      await relaunch()
    } catch {
      installing.value = false
    }
  } else if (!updateStore.downloading) {
    await updateStore.download()
  }
}
</script>
