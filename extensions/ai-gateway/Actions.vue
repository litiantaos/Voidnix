<template>
  <BaseButton
    icon="i-ri-information-line"
    :title="t('ai-gateway.configGuide')"
    :aria-label="t('ai-gateway.configGuide')"
    @click="showHelp = true"
  />
  <BaseDialog
    v-if="showHelp"
    :title="t('ai-gateway.usageGuide')"
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
import { GATEWAY_PORT } from './logic'

const showHelp = ref(false)

// KeepAlive 切走扩展时关闭说明弹窗,避免再次进入时残留
onDeactivated(() => {
  showHelp.value = false
})

const helpMarkdown = computed(() =>
  renderMarkdown(t('ai-gateway.helpMarkdown', { port: GATEWAY_PORT })),
)
</script>
