<template>
  <BaseButton
    icon="i-ri-information-line"
    :title="t('ai-providers.configGuide')"
    :aria-label="t('ai-providers.configGuide')"
    @click="showHelp = true"
  />
  <BaseButton
    icon="i-ri-add-line"
    :title="t('ai-providers.addProvider')"
    :aria-label="t('ai-providers.addProvider')"
    @click="onAdd"
  />
  <BaseDialog
    v-if="showHelp"
    :title="t('ai-providers.usageGuide')"
    size="lg"
    :show-cancel="false"
    @confirm="showHelp = false"
    @cancel="showHelp = false"
  >
    <div class="markdown-body" text="sm primary" leading="relaxed">
      <div class="md-full" v-html="helpMarkdown" />
    </div>
  </BaseDialog>
</template>

<script setup lang="ts">
import { ref, computed, onDeactivated } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import { renderMarkdown } from '@/utils/markdown'
import { t } from '@/runtime/i18n'
import { requestCreateProvider } from './bridge'

const showHelp = ref(false)

// KeepAlive 切走扩展时关闭帮助弹窗，避免再次进入时残留
onDeactivated(() => {
  showHelp.value = false
})

function onAdd() {
  requestCreateProvider()
}

const helpMarkdown = computed(() => renderMarkdown(t('ai-providers.helpMarkdown')))
</script>
