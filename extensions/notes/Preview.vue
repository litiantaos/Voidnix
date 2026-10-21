<template>
  <!-- 预览层：沿用 agent 回复正文的 markdown 排版（全局 src/styles/markdown.css +
       同一组 text/leading 工具类）。固定窗高下自身滚动，与编辑层同构。
       外壳沿用输入区面（ui-field），故此处复位两项被其继承污染的阅读态属性：
       select-text 允许拖选复制；font-normal 抵消 ui-field 的 font-medium，
       正文与 agent 回复同为正文字重 -->
  <div
    class="notes-preview hide-scrollbar radius-panel"
    p="3"
    flex="1"
    min-h="0"
    overflow="y-auto"
    font="normal"
    select="text"
    @click="onClick"
  >
    <div v-if="text" class="markdown-body" text="sm primary" leading="relaxed" v-html="html" />
    <span v-else text="sm muted">{{ t('notes.previewEmpty') }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { renderMarkdown } from '@/utils/markdown'
import { writeText } from '@/utils/clipboard'
import { showToast } from '@/composables/useToast'
import { t } from '@/runtime/i18n'

const props = defineProps<{ text: string }>()

/** 预览 HTML：computed 惰性求值，仅预览态挂载时解析——编辑态零渲染开销 */
const html = computed(() => renderMarkdown(props.text))

/** 复制成功反馈时长（按钮图标翻转 check，与 agent 回复正文一致） */
const COPY_FLIP_MS = 1500

/// markdown 交互委托（renderMarkdown 输出的契约）：
/// - .md-code-copy：复制所在代码块，按钮图标翻转反馈
/// - a[href^=http]：系统浏览器打开，webview 内不导航
async function onClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null

  const copyBtn = target?.closest?.('.md-code-copy') as HTMLElement | null
  if (copyBtn) {
    e.preventDefault()
    e.stopPropagation()
    const code = copyBtn.closest('.md-code')?.querySelector('pre code')?.textContent ?? ''
    if (!code) return
    try {
      await writeText(code)
      flipCopied(copyBtn)
    } catch {
      showToast(t('common.operationFailed'), { kind: 'error' })
    }
    return
  }

  const anchor = target?.closest?.('a') as HTMLAnchorElement | null
  if (!anchor) return
  const href = anchor.getAttribute('href') || ''
  // 仅拦截 http/https（mailto/tel 等让浏览器默认处理）
  if (!/^https?:\/\//i.test(href)) return
  e.preventDefault()
  e.stopPropagation()
  try {
    await open(href)
  } catch (err) {
    console.warn('Failed to open URL:', err)
  }
}

function flipCopied(btn: HTMLElement) {
  btn.classList.add('is-copied')
  const icon = btn.querySelector('i')
  if (icon) icon.className = 'i-ri-check-line'
  window.setTimeout(() => {
    btn.classList.remove('is-copied')
    if (icon) icon.className = 'i-ri-file-copy-line'
  }, COPY_FLIP_MS)
}
</script>
