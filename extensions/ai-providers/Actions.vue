<template>
  <BaseButton
    icon="i-ri-information-line"
    title="配置说明"
    aria-label="配置说明"
    @click="showHelp = true"
  />
  <BaseButton icon="i-ri-add-line" title="添加提供商" aria-label="添加提供商" @click="onAdd" />
  <BaseDialog
    v-if="showHelp"
    title="使用说明"
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
import { ref, onDeactivated } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import { renderMarkdown } from '@/utils/markdown'
import { requestCreateProvider } from './bridge'

const showHelp = ref(false)

// KeepAlive 切走模块时关闭帮助弹窗，避免再次进入时残留
onDeactivated(() => {
  showHelp.value = false
})

function onAdd() {
  requestCreateProvider()
}

const helpMarkdown = renderMarkdown(`
提供商配置保存后写入 \`~/.config/voidnix/ai.env\`，新开终端生效。

- Key 导出为 \`VOIDNIX_*_API_KEY\`，端点导出为 \`VOIDNIX_*_BASE_URL\`
- 智谱、DeepSeek 用固定后缀，如 \`VOIDNIX_ZHIPU_API_KEY\`，其余按名称推导
- 外部工具须显式引用，如 OpenCode \`{env:VOIDNIX_ZHIPU_API_KEY}\`
- 选中 Key 按下 **Cmd+Enter** 可粘贴 Key / 端点 / 模型名
`)
</script>
