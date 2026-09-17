<template>
  <!-- 顶距交给 scrollContainer CHROME_HEIGHT（已含栏底 gap），勿再 p-t 叠双层 -->
  <div flex="~ col" gap="3" :class="{ 'pb-3': !session.ocrText && !session.error }">
    <!-- 截图预览：cover 缩放铺满容器，长边溢出可上下/左右滚动。
         滚动层与遮罩分层：absolute 遮罩若在滚动容器内会随内容滚走（初始仅覆盖视口）。
         overflow-hidden 在外层裁圆角：图片与遮罩四角不溢出 radius-panel 边框范围 -->
    <div
      v-if="session.imageUrl"
      m="x-3"
      relative
      overflow="hidden"
      class="border border-divider radius-panel border-solid fill-ctrl"
      h="44"
      shrink="0"
    >
      <div ref="previewRef" absolute inset="0" overflow="auto" class="hide-scrollbar">
        <img
          :src="session.imageUrl"
          block
          max-w="none"
          w="full"
          h="full"
          object="cover left-top"
          :alt="t('screenshot.previewAlt')"
          @load="onPreviewLoad"
        />
      </div>
      <!-- 识别中加载遮罩：磨砂 + 居中空态（在滚动层外，钉住视口不随滚动） -->
      <Transition
        enter-active-class="transition duration-[var(--duration-fast)] ease-out"
        enter-from-class="opacity-0"
        enter-to-class="opacity-100"
        leave-active-class="transition duration-[var(--duration-fastest)] ease-in"
        leave-from-class="opacity-100"
        leave-to-class="opacity-0"
      >
        <div
          v-if="session.loading"
          class="bg-[var(--mica-shell-fill)] flex inset-0 absolute backdrop-blur-xs"
        >
          <BaseEmptyState loading />
        </div>
      </Transition>
    </div>

    <!-- 错误 -->
    <div v-if="session.error" p="x-3 b-3" shrink="0">
      <div text="sm danger" p="3" class="radius-ctrl bg-danger-soft">
        {{ session.error }}
      </div>
    </div>

    <!-- 结果 -->
    <template v-else-if="session.ocrText">
      <div p="x-3">
        <BaseTextarea
          v-model="session.ocrText"
          rounded="panel"
          :rows="4"
          :max-height="textMaxHeight"
          :submit-on-enter="false"
          :placeholder="t('screenshot.ocrResult')"
        />
      </div>

      <!-- 操作按钮行（横排）：左右键切换选中，回车触发；点击即选中并执行 -->
      <div p="x-3 b-3" flex gap="2">
        <BaseButton
          v-for="(action, i) in ocrActions"
          :key="action.id"
          :active="i === actionIndex"
          @click="onAction(i)"
        >
          {{ action.label }}
        </BaseButton>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onActivated, onDeactivated } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { t } from '@/runtime/i18n'
import { isTauri } from '@/utils/tauri'
import { copyAndHide, useAppStore } from '@/stores/app'
import { onKeyStroke } from '@/composables/events'
import { isComposing, isModalDialogOpen, wrapIndex } from '@/utils/dom'
import { pendingOcrData, ocrSession } from './index'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

interface OcrResult {
  text: string
  qr: string[]
}

interface OcrAction {
  id: string
  label: string
  run: () => void | Promise<void>
}

const appStore = useAppStore()
// 会话状态提升至模块级 ocrSession（见 index.ts）：组件销毁（KeepAlive LRU 驱逐 /
// navigate 重载）后重挂载恢复现场；此处仅 previewRef / actionIndex 留组件局部
const session = ocrSession
const previewRef = ref<HTMLElement>()
const actionIndex = ref(0)

// 输入框高度弹性上限：内容自然撑高、超限框内滚动。上限从 get_window_max_height
// 命令推导（placement/光标屏 visibleFrame × 0.9，与 set_main_frame 的 Rust clamp
// 同源）− 固定部分（chrome 76 + 预览 176 + 间距 24 + 边框 2 + 操作按钮行 40 = 318）
// − 12px 余量，内容恒不超窗（窗口级零滚动）。必须同源——按整屏高（monitor 尺寸
// ÷ scale，含菜单栏/Dock）推导会撑过 clamp 重引窗口级滚动。非 Tauri / 查询失败
// 回退 224（10 行），下限 144（6 行）防短屏算出退化值。每次唤起都是新挂载
// （隐藏清 KeepAlive），无需监听屏变化；上限晚到由 BaseTextarea watch maxHeight 重测补齐
const textMaxHeight = ref(224)

onMounted(async () => {
  if (!isTauri) return
  const maxWin = await invoke<number | null>(CMD.getWindowMaxHeight).catch(() => null)
  if (!maxWin) return
  textMaxHeight.value = Math.max(144, Math.round(maxWin) - 318 - 12)
})

// 截图 cover 缩放：短边撑满容器、长边溢出（容器 overflow-auto 可上下/左右滚动）
// 双阶段：加载前用 CSS object-fit:cover（object-position:left top 对齐 scroll 0,0）
// 零滞后且无跳变；@load 后按 natural 尺寸手算精确 cover 尺寸写入内联宽高，
// 切换为可滚动模式——切换前后短边撑满方式与左上对齐完全一致，视觉连续。
function onPreviewLoad(e: Event) {
  const img = e.target as HTMLImageElement
  const box = previewRef.value
  if (!img || !box) return
  const cw = box.clientWidth
  const ch = box.clientHeight
  const nw = img.naturalWidth
  const nh = img.naturalHeight
  if (!cw || !ch || !nw || !nh) return
  const scale = Math.max(cw / nw, ch / nh)
  img.style.width = `${Math.round(nw * scale)}px`
  img.style.height = `${Math.round(nh * scale)}px`
}

async function runOcr(data: NonNullable<typeof pendingOcrData.value>) {
  session.value.loading = true
  session.value.error = ''
  session.value.ocrText = ''
  try {
    const result = await invoke<OcrResult>(CMD.ocrImage, {
      selX: data.selX,
      selY: data.selY,
      selW: data.selW,
      selH: data.selH,
      scale: data.scale,
      annotationPng: data.annotationPng,
    })
    if (result.qr?.length) {
      session.value.ocrText = result.qr.join('\n')
    } else {
      session.value.ocrText = result.text || t('screenshot.noContent')
    }
  } catch (e) {
    session.value.error = String(e)
  } finally {
    session.value.loading = false
    // 按钮行接管键盘：默认选中首项（复制），回车直接复制；点击 textarea 可编辑
    actionIndex.value = 0
  }
}

watch(
  pendingOcrData,
  (data) => {
    if (!data) return
    session.value.imageUrl = data.previewPng || ''
    runOcr(data)
    pendingOcrData.value = null
  },
  { immediate: true },
)

async function handleCopy() {
  if (!session.value.ocrText.trim()) return
  await copyAndHide(session.value.ocrText)
}

async function handleTranslate() {
  if (!session.value.ocrText.trim()) return
  // 跨扩展同页投递（C9）：screenshot 不直依赖 translate 内部状态。
  // translate 扩展 setup 监听 'translate-pending-text'（window CustomEvent，同步达），
  // 写入自身 pendingText——跳转首帧即进入翻译中状态。
  window.dispatchEvent(new CustomEvent('translate-pending-text', { detail: session.value.ocrText }))
  appStore.setActiveExtension('translate')
}

function trimSpaces() {
  session.value.ocrText = session.value.ocrText.replace(/[ \t\u3000]+/g, '')
}

function trimNewlines() {
  session.value.ocrText = session.value.ocrText.replace(/[\r\n]+/g, '')
}

function trimEmptyLines() {
  session.value.ocrText = session.value.ocrText
    .split('\n')
    .filter((line) => line.trim() !== '')
    .join('\n')
}

// 操作按钮行（原列表项改回横排）：识别完成后默认选中首项（复制），回车直接复制
const ocrActions = computed<OcrAction[]>(() => {
  if (!session.value.ocrText.trim()) return []
  return [
    { id: 'copy', label: t('screenshot.copy'), run: handleCopy },
    { id: 'translate', label: t('screenshot.translate'), run: handleTranslate },
    { id: 'trimSpaces', label: t('screenshot.trimSpaces'), run: trimSpaces },
    { id: 'trimNewlines', label: t('screenshot.trimNewlines'), run: trimNewlines },
    { id: 'trimEmptyLines', label: t('screenshot.trimEmptyLines'), run: trimEmptyLines },
  ]
})

// ── 按钮行键盘：左右键环形切换，回车触发（让位语义与 BaseList 对齐）──
// KeepAlive 软禁用：deactivate 后监听仍在，isActive 抑制响应
const isActive = ref(true)
onActivated(() => {
  isActive.value = true
})
onDeactivated(() => {
  isActive.value = false
})

function canNavigate(e: KeyboardEvent): boolean {
  if (!isActive.value) return false
  if (ocrActions.value.length === 0) return false
  if (appStore.fullscreenView) return false
  if (isComposing(e)) return false
  if (isModalDialogOpen()) return false
  return true
}

onKeyStroke(
  ['ArrowLeft', 'ArrowRight'],
  (e) => {
    if (!canNavigate(e)) return
    e.preventDefault()
    actionIndex.value = wrapIndex(
      actionIndex.value,
      ocrActions.value.length,
      e.key === 'ArrowRight' ? 'down' : 'up',
    )
  },
  // 焦点在 textarea（编辑识别结果）时让出：左右键移动光标
  { ignoreFormControls: true },
)

onKeyStroke(
  'Enter',
  (e) => {
    if (!canNavigate(e)) return
    // 按钮聚焦时 Enter 由按钮自身 click 处理
    if (document.activeElement?.tagName === 'BUTTON') return
    // 按住回车 auto-repeat 不重复执行（跨扩展跳转等场景）
    if (e.repeat) return
    e.preventDefault()
    // 执行即消费：一次回车至多触发一个动作（防 KeepAlive 并存监听 / 全局导航重复响应）
    e.stopImmediatePropagation()
    void ocrActions.value[actionIndex.value]?.run()
  },
  // textarea 聚焦时回车换行（submit-on-enter=false）
  { ignoreFormControls: true },
)

// 跨扩展转移归首项（与 BaseList 自管列表同语义）；subview 往返与窗口唤起保留
watch(
  () => appStore.activeExtId,
  () => {
    actionIndex.value = 0
  },
)

function onAction(index: number) {
  actionIndex.value = index
  void ocrActions.value[index]?.run()
}
</script>
