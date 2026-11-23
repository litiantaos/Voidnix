<template>
  <div
    ref="rootEl"
    p="1"
    flex
    gap="1.5"
    items="center"
    absolute
    z="50"
    class="palette-surface"
    :style="style"
    @mousedown.stop
  >
    <!-- 毛玻璃背景独立一层：acrylic-bar 的 backdrop-filter 会成为 fixed 子元素的
         包含块，把弹出面板的屏幕坐标限制在工具栏本地坐标里（面板被推到屏幕外）。
         背景层是兄弟节点，弹出面板的 fixed 仍相对视口。 -->
    <div
      class="palette-surface acrylic-bar pointer-events-none inset-0 absolute -z-1"
      aria-hidden="true"
    />
    <!-- 滚动截屏模式：仅保留保存（关闭/确定由统一尾部提供） -->
    <template v-if="mode === 'scroll'">
      <BaseButton
        variant="ghost"
        :title="t('screenshot.save')"
        icon="i-ri-save-line"
        @click="$emit('scroll-save')"
      />
    </template>

    <!-- 标注模式：完整工具栏 -->
    <template v-else>
      <!-- 工具按钮 -->
      <BaseButton
        v-for="t in tools"
        :key="t.icon"
        variant="ghost"
        :active="activeTool === t.id"
        :title="t.label"
        :icon="t.icon"
        @click="$emit('tool', t.id)"
      />

      <div m="x-0.5" class="fill-active" shrink="0" h="5" w="px" />

      <!-- 文字样式切换（仅 text 工具时显示，在颜色选择器之前）：纯文本 / 底色模式 -->
      <div v-if="activeTool === 'text'" flex gap="1.5">
        <BaseButton
          variant="ghost"
          :active="!textBg"
          :title="t('screenshot.textPlain')"
          icon="i-ri-input-method-line"
          @click="emit('text-bg', false)"
        />
        <BaseButton
          variant="ghost"
          :active="textBg"
          :title="t('screenshot.textBg')"
          icon="i-ri-input-method-fill"
          @click="emit('text-bg', true)"
        />
      </div>

      <!-- 颜色选择器（模糊工具时不显示） -->
      <div v-if="activeTool !== 'blur'" ref="colorTriggerEl" flex h="7" items="center" relative>
        <button
          border="2"
          rounded="full"
          shrink="0"
          h="4"
          w="4"
          shadow="sm"
          transition="transform"
          class="active:scale-95"
          :style="{
            background: color,
            borderColor: color === '#ffffff' ? 'var(--color-border)' : 'white',
          }"
          :title="t('screenshot.color')"
          @click="toggleColors"
        />

        <Transition name="palette-popup">
          <div
            ref="colorPanelEl"
            v-if="showColors"
            p="1"
            z="100"
            class="acrylic-bar"
            :style="colorPopupStyle"
            @mousedown.stop
            @click.stop
          >
            <div flex h="7" gap="2" items="center">
              <button
                v-for="c in colors"
                :key="c"
                border="2"
                rounded="full"
                shrink="0"
                h="4"
                w="4"
                shadow="sm"
                transition="transform"
                class="active:scale-95"
                :class="{
                  'ring-2 ring-accent ring-offset-1': c === color,
                }"
                :style="{
                  background: c,
                  borderColor: c === '#ffffff' ? 'var(--color-border)' : 'white',
                }"
                :title="c"
                @click="selectColor(c)"
              />
            </div>
          </div>
        </Transition>
      </div>

      <!-- 字号（文本工具） -->
      <BaseSlider
        v-if="activeTool === 'text'"
        :model-value="fontSize"
        :min="12"
        :max="64"
        :title="t('screenshot.fontSize')"
        @update:model-value="$emit('font-size', $event)"
      />

      <!-- 线宽（矩形/直线/箭头） -->
      <BaseSlider
        v-if="activeTool === 'rect' || activeTool === 'line' || activeTool === 'arrow'"
        :model-value="lineWidth"
        :min="1"
        :max="12"
        :title="t('screenshot.lineWidth')"
        @update:model-value="$emit('line-width', $event)"
      />

      <!-- 模糊模式切换（仅 blur 工具时显示） -->
      <div v-if="activeTool === 'blur'" flex gap="1.5">
        <BaseButton
          variant="ghost"
          :active="blurMode === 'selection'"
          :title="t('screenshot.blurSelection')"
          icon="i-ri-checkbox-blank-line"
          @click="emit('blur-mode', 'selection')"
        />
        <BaseButton
          variant="ghost"
          :active="blurMode === 'text'"
          :title="t('screenshot.blurText')"
          icon="i-ri-t-box-line"
          @click="emit('blur-mode', 'text')"
        />
      </div>

      <!-- 模糊度（仅 blur 工具时显示） -->
      <BaseSlider
        v-if="activeTool === 'blur'"
        :model-value="blurAmount"
        :min="5"
        :max="50"
        :title="t('screenshot.blurAmount')"
        @update:model-value="$emit('blur-amount', $event)"
      />

      <div class="mx-0.5 fill-active shrink-0 h-5 w-px" />

      <!-- 操作按钮 -->
      <BaseButton
        variant="ghost"
        :title="t('screenshot.ocr')"
        icon="i-ri-qr-scan-2-line"
        @click="$emit('ocr')"
      />
      <BaseButton
        variant="ghost"
        :title="t('screenshot.scrollCapture')"
        icon="i-ri-arrow-down-double-line"
        @click="$emit('scroll-start')"
      />
      <div ref="recordTriggerEl" relative>
        <BaseButton
          variant="ghost"
          :title="t('screenshot.record')"
          icon="i-ri-record-circle-line"
          :loading="recordLoading"
          @click="toggleRecordMenu"
        />
        <Transition name="palette-popup">
          <div
            ref="recordPanelEl"
            v-if="recordMenu"
            class="palette-surface acrylic-bar w-max z-100"
            p="1"
            flex
            gap="1.5"
            items="center"
            :style="recordPopupStyle"
            @mousedown.stop
            @click.stop
          >
            <BaseButton
              variant="ghost"
              :active="prefs.systemAudio"
              :title="t('screenshot.recordSystemAudio')"
              :icon="prefs.systemAudio ? 'i-ri-volume-up-line' : 'i-ri-volume-mute-line'"
              @click="toggleRecordPref('systemAudio')"
            />
            <BaseButton
              variant="ghost"
              :active="prefs.microphone"
              :title="t('screenshot.recordMicrophone')"
              :icon="prefs.microphone ? 'i-ri-mic-line' : 'i-ri-mic-off-line'"
              @click="toggleRecordPref('microphone')"
            />
            <BaseButton
              variant="ghost"
              class="font-mono !px-2"
              :title="t('screenshot.recordFps')"
              @click="cycleRecordFps"
            >
              {{ prefs.fps }} fps
            </BaseButton>
            <BaseButton
              variant="ghost"
              :active="prefs.openClip"
              :title="t('screenshot.openClipAfterRecord')"
              icon="i-ri-film-line"
              @click="toggleRecordPref('openClip')"
            />
            <div class="mx-0.5 fill-active shrink-0 h-5 w-px" />
            <BaseButton
              variant="primary"
              icon="i-ri-record-circle-line"
              :title="t('screenshot.startRecording')"
              @click="startRecording"
            />
          </div>
        </Transition>
      </div>
      <BaseButton
        variant="ghost"
        :title="t('screenshot.pin')"
        icon="i-ri-pushpin-line"
        @click="$emit('pin')"
      />
      <BaseButton
        variant="ghost"
        :title="t('screenshot.save')"
        icon="i-ri-save-line"
        @click="$emit('save')"
      />
    </template>

    <!-- 统一尾部：关闭 + 确定（复制并关闭），始终位于最右侧 -->
    <div class="mx-0.5 fill-active shrink-0 h-5 w-px" />
    <BaseButton
      variant="ghost"
      :title="t('screenshot.closeEsc')"
      icon="i-ri-close-line"
      @click="onClose"
    />
    <BaseButton
      variant="ghost"
      :title="t('screenshot.copyAndClose')"
      icon="i-ri-check-line"
      @click="onConfirm"
    />
  </div>
</template>

<script setup lang="ts">
import {
  ref,
  reactive,
  computed,
  onMounted,
  onBeforeUnmount,
  nextTick,
  watch,
  type CSSProperties,
} from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { t } from '@/runtime/i18n'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSlider from '@/components/ui/BaseSlider.vue'
import type { RecordPrefs } from '../composables/useTypes'

type Tool = 'rect' | 'line' | 'arrow' | 'text' | 'blur' | null
type BlurMode = 'selection' | 'text'
type PaletteMode = 'annotate' | 'scroll'

const props = defineProps<{
  sel: { x: number; y: number; w: number; h: number }
  activeTool: Tool
  color: string
  lineWidth: number
  fontSize: number
  textBg: boolean
  blurAmount: number
  blurMode: BlurMode
  screenHeight: number
  screenWidth: number
  mode?: PaletteMode
  /** 录屏启动在途（倒计时 + SCK start）：按钮转 spinner 防重复点击 */
  recordLoading?: boolean
  /** 录屏弹出菜单初始选项（Rust 当前权威值） */
  recordPrefs?: RecordPrefs
}>()

const emit = defineEmits<{
  tool: [t: Tool]
  color: [c: string]
  'line-width': [n: number]
  'font-size': [n: number]
  'text-bg': [b: boolean]
  'blur-amount': [n: number]
  'blur-mode': [m: BlurMode]
  ocr: []
  'record-prefs': [prefs: RecordPrefs]
  'record-start': [prefs: RecordPrefs]
  pin: []
  copy: []
  save: []
  cancel: []
  'scroll-start': []
  'scroll-finish': []
  'scroll-save': []
  'scroll-cancel': []
}>()

const showColors = ref(false)
const rootEl = ref<HTMLElement>()
const colorTriggerEl = ref<HTMLElement>()
const recordTriggerEl = ref<HTMLElement>()
const colorPanelEl = ref<HTMLElement>()
const recordPanelEl = ref<HTMLElement>()

// 子面板用 fixed 坐标：触发器在工具栏里的位置 + 面板实测尺寸 → 屏幕边缘 clamp。
// （绝对定位会以 28px 触发器为包含块收缩宽度，且无法跨出工具栏做边界避让）
const POPUP_GAP = 12
const hiddenPopupStyle: CSSProperties = {
  position: 'fixed',
  left: '0px',
  top: '0px',
  visibility: 'hidden',
}
const colorPopupStyle = ref<CSSProperties>({ ...hiddenPopupStyle })
const recordPopupStyle = ref<CSSProperties>({ ...hiddenPopupStyle })

// ── 录屏弹出菜单 ──────────────────────────────────────────
type RecordToggleKey = 'systemAudio' | 'microphone' | 'openClip'
const recordMenu = ref(false)
const prefs = reactive<RecordPrefs>({
  systemAudio: props.recordPrefs?.systemAudio ?? true,
  microphone: props.recordPrefs?.microphone ?? false,
  fps: props.recordPrefs?.fps ?? 30,
  openClip: props.recordPrefs?.openClip ?? true,
})

function popupStyleFor(
  panel: HTMLElement | undefined,
  trigger: HTMLElement | undefined,
  align: 'center' | 'right',
): CSSProperties | null {
  if (!panel || !trigger) return null
  const triggerRect = trigger.getBoundingClientRect()
  const panelRect = panel.getBoundingClientRect()
  if (panelRect.width <= 0 || panelRect.height <= 0) return null
  const maxLeft = Math.max(EDGE_PAD, props.screenWidth - panelRect.width - EDGE_PAD)
  const anchor =
    align === 'center'
      ? triggerRect.left + triggerRect.width / 2 - panelRect.width / 2
      : triggerRect.right - panelRect.width
  const left = Math.max(EDGE_PAD, Math.min(anchor, maxLeft))
  let top = triggerRect.bottom + POPUP_GAP
  if (top + panelRect.height > props.screenHeight - EDGE_PAD) {
    const above = triggerRect.top - POPUP_GAP - panelRect.height
    top =
      above >= EDGE_PAD
        ? above
        : Math.max(EDGE_PAD, props.screenHeight - panelRect.height - EDGE_PAD)
  }
  return { position: 'fixed', left: `${left}px`, top: `${top}px` }
}

function positionColorPopup() {
  const next = popupStyleFor(colorPanelEl.value, colorTriggerEl.value, 'center')
  if (next) colorPopupStyle.value = next
}

function positionRecordPopup() {
  const next = popupStyleFor(recordPanelEl.value, recordTriggerEl.value, 'right')
  if (next) recordPopupStyle.value = next
}

function toggleRecordMenu() {
  showColors.value = false
  if (recordMenu.value) {
    recordMenu.value = false
    return
  }
  recordPopupStyle.value = { ...hiddenPopupStyle }
  recordMenu.value = true
  void nextTick(positionRecordPopup)
}

function toggleColors() {
  recordMenu.value = false
  if (showColors.value) {
    showColors.value = false
    return
  }
  colorPopupStyle.value = { ...hiddenPopupStyle }
  showColors.value = true
  void nextTick(positionColorPopup)
}

function toggleRecordPref(key: RecordToggleKey) {
  prefs[key] = !prefs[key]
  emit('record-prefs', { ...prefs })
}

function setRecordFps(fps: number) {
  prefs.fps = fps
  emit('record-prefs', { ...prefs })
}

function cycleRecordFps() {
  setRecordFps(prefs.fps === 30 ? 60 : 30)
}

function startRecording() {
  recordMenu.value = false
  emit('record-start', { ...prefs })
}

function selectColor(c: string) {
  emit('color', c)
  showColors.value = false
}

// 统一尾部按钮：根据 mode 分发到对应事件
function onConfirm() {
  if (props.mode === 'scroll') emit('scroll-finish')
  else emit('copy')
}
function onClose() {
  if (props.mode === 'scroll') emit('scroll-cancel')
  else emit('cancel')
}

// 尺寸单一真相源：DOM 实测（ResizeObserver），不维护估算值。位置 = f(选区, 实测尺寸)，
// 尺寸未就绪时只渲染隐藏占位——首次可见帧就是正确位置，工具切换/字体变化也不会先错位再跳。
const paletteSize = ref<{ w: number; h: number } | null>(null)
let paletteObserver: ResizeObserver | undefined

const tools = computed(() => [
  { id: 'rect' as Tool, label: t('screenshot.tool.rect'), icon: 'i-ri-checkbox-blank-line' },
  { id: 'line' as Tool, label: t('screenshot.tool.line'), icon: 'i-ri-subtract-line' },
  { id: 'arrow' as Tool, label: t('screenshot.tool.arrow'), icon: 'i-ri-arrow-right-line' },
  { id: 'text' as Tool, label: t('screenshot.tool.text'), icon: 'i-ri-text' },
  { id: 'blur' as Tool, label: t('screenshot.tool.blur'), icon: 'i-ri-drop-line' },
])

const colors = [
  '#ff3b30',
  '#ff9500',
  '#ffcc00',
  '#34c759',
  '#007aff',
  '#5856d6',
  '#af52de',
  '#ffffff',
  '#000000',
]

const PALETTE_GAP = 8
const EDGE_PAD = 8

function measure() {
  const el = rootEl.value
  if (!el) return
  const rect = el.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return
  const cur = paletteSize.value
  if (cur && cur.w === rect.width && cur.h === rect.height) return
  paletteSize.value = { w: rect.width, h: rect.height }
}

// scroll 模式:把工具栏屏幕矩形同步给 Rust(mouse_monitor 排除该区域,避免穿透)
async function reportToolbarRect() {
  if (props.mode !== 'scroll') {
    clearToolbarRect()
    return
  }
  if (!rootEl.value) return
  await nextTick()
  const el = rootEl.value
  if (!el) return
  const rect = el.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return
  invoke(CMD.setScrollToolbarRect, {
    x: rect.left,
    y: rect.top,
    w: rect.width,
    h: rect.height,
  }).catch(() => {})
}

function clearToolbarRect() {
  invoke(CMD.setScrollToolbarRect, { x: 0, y: 0, w: 0, h: 0 }).catch(() => {})
}

onMounted(() => {
  measure()
  if (typeof ResizeObserver !== 'undefined' && rootEl.value) {
    paletteObserver = new ResizeObserver(() => measure())
    paletteObserver.observe(rootEl.value)
  }
})

onBeforeUnmount(() => {
  paletteObserver?.disconnect()
  if (props.mode === 'scroll') clearToolbarRect()
})

// 尺寸/选区/模式变化后：同步 scroll 工具栏矩形，并重算已打开的子面板锚点。
// 尺寸本身由 ResizeObserver 更新，这里只消费结果。
watch(
  () =>
    [
      props.activeTool,
      paletteSize.value,
      props.sel.x,
      props.sel.y,
      props.sel.w,
      props.sel.h,
      props.mode,
    ] as const,
  () =>
    nextTick(() => {
      reportToolbarRect()
      if (showColors.value) positionColorPopup()
      if (recordMenu.value) positionRecordPopup()
    }),
)

// 帧率文案宽度变化时重算弹出面板位置（右对齐依赖面板实测宽度）
watch(
  () => prefs.fps,
  () => {
    if (recordMenu.value) void nextTick(positionRecordPopup)
  },
)

const style = computed<CSSProperties>(() => {
  const size = paletteSize.value
  // 尺寸未实测前只占位不显示：避免用估算宽高算出错误 above/inside 位置（初始 gap 偏小）
  if (!size) return { visibility: 'hidden' }
  const { x, y, w, h } = props.sel
  const pw = size.w
  const ph = size.h

  // 水平：优先与选区左对齐；超出右边界则向左偏移；选区本身贴右时退回到 sel.right - pw。
  let left: number
  if (x + pw <= props.screenWidth - EDGE_PAD) {
    left = Math.max(EDGE_PAD, x)
  } else {
    left = Math.max(EDGE_PAD, Math.min(x + w - pw, props.screenWidth - pw - EDGE_PAD))
  }

  const base: CSSProperties = { left: `${left}px`, pointerEvents: 'auto' }

  // 垂直：优先下方；不够则上方；都不够则贴在选区内底部。
  const belowTop = y + h + PALETTE_GAP
  if (belowTop + ph <= props.screenHeight - EDGE_PAD) {
    return { top: `${belowTop}px`, ...base }
  }
  if (y - ph - PALETTE_GAP >= EDGE_PAD) {
    return { top: `${y - ph - PALETTE_GAP}px`, ...base }
  }
  const insideBottom = props.screenHeight - (y + h) + PALETTE_GAP
  return {
    bottom: `${Math.max(insideBottom, EDGE_PAD)}px`,
    ...base,
  }
})

// 工具切换时关闭颜色弹窗，避免 blur 工具下隐藏 + 残留状态
watch(
  () => props.activeTool,
  () => {
    showColors.value = false
    recordMenu.value = false
  },
)

defineExpose({ reportToolbarRect })
</script>

<style scoped>
/*
 * 工具栏 / 色板叠在花色截图上，需比主搜索框（0.72）更实以保证按钮可读；
 * 仅局部覆盖 soft-surface-fill 基元，blur/saturate 玻璃感保留
 */
.palette-surface {
  --soft-surface-fill: rgb(246 246 248 / 0.92);
  /* 工具栏激活态比全局列表选中更深，保证叠在截图上可读（= fill-12 档） */
  --ui-active-fill: var(--color-fill-12);
}

.palette-popup-enter-active,
.palette-popup-leave-active {
  transition:
    opacity var(--duration-normal) var(--ease-out),
    transform var(--duration-normal) var(--ease-out);
}
.palette-popup-enter-from,
.palette-popup-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
