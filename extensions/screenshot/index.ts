import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { defineExtension } from '@/runtime/extension-registry'
import { useAppStore } from '@/stores/app'
import { showToast } from '@/composables/useToast'
import { t } from '@/runtime/i18n'
import { isTauri } from '@/utils/tauri'
import { config } from './config'
import type { RecordPrefs } from './composables/useTypes'
import './locales'
import ScreenshotView from './View.vue'
import ScreenshotOcr from './OcrView.vue'

// 快捷键截屏失败（权限被拒等）：Rust emit 错误文案，外部 toast 展示（主窗
// 隐藏态照常可见）——此前仅 eprintln，按键无反应无从排查。模块级常驻监听
listen<string>('screenshot-capture-failed', (e) => {
  showToast(e.payload, { kind: 'error', duration: 4000 })
}).catch(() => {})

/** 录屏时长（ms）→ 反馈文案：短录屏显示秒，超过一分钟用 m:ss。 */
function formatRecordDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000))
  const minutes = Math.floor(total / 60)
  const seconds = total % 60
  return minutes > 0 ? `${minutes}:${String(seconds).padStart(2, '0')}` : `${seconds}s`
}

// 录屏结束（Rust 侧 ScreenCaptureKit 收尾后 emit；主窗隐藏态照常收到）
listen<{ path: string; durationMs: number; warning?: string }>(
  'screenshot-recording-saved',
  (e) => {
    if (e.payload.warning) {
      // 收尾异常但文件非空：保留文件，提示风险后仍进剪辑台（可能可播）
      void showToast(t('screenshot.recordingSavedIncomplete', { error: e.payload.warning }), {
        kind: 'error',
        duration: 5000,
      })
    } else {
      void showToast(
        t('screenshot.recordingSaved', { duration: formatRecordDuration(e.payload.durationMs) }),
        { duration: 3000 },
      )
    }
    if (!config.openClipAfterRecord) return
    // 跨扩展交接：video 扩展 setup 监听同名 CustomEvent 并打开剪辑台（同页同步投递，
    // 不引入扩展间 import；剪辑台打开前主窗保持隐藏，由 video 侧负责焦点时序）
    window.dispatchEvent(new CustomEvent('video-clip-open', { detail: [e.payload.path] }))
  },
).catch(() => {})

listen<string>('screenshot-recording-failed', (e) => {
  showToast(e.payload, { kind: 'error', duration: 4000 })
}).catch(() => {})

// 收尾期间再按截屏快捷键（Rust setup 钩子 emit）：按键有反馈，不被误判卡死
listen('screenshot-recording-stopping', () => {
  void showToast(t('screenshot.recordingStopping'), { duration: 1500 })
}).catch(() => {})

// overlay 录屏弹出菜单的选项变更：子窗口无 store 权限，由 main 窗口落 config 持久化
listen<RecordPrefs>('screenshot-record-prefs', (e) => {
  config.recordSystemAudio = e.payload.systemAudio
  config.recordMicrophone = e.payload.microphone
  config.recordFps = e.payload.fps
  config.openClipAfterRecord = e.payload.openClip
}).catch(() => {})

// OCR 待识别数据（由截屏标注界面通过 open_extension_subview 触发时注入）
export const pendingOcrData = ref<{
  selX: number
  selY: number
  selW: number
  selH: number
  scale: number
  annotationPng: string
  previewPng: string
} | null>(null)

/// OCR 会话状态（模块级单例）：KeepAlive 缓存超限 LRU 驱逐 / navigate 重载都会
/// 销毁 OcrView 组件，局部状态随之丢失（卸载后重进内容空白）——提升到模块级，
/// 组件重挂载直接恢复现场；识别进行中隐藏亦然（invoke 回调写模块级 ref）。
export const ocrSession = ref({
  imageUrl: '',
  ocrText: '',
  error: '',
  loading: false,
})

export default defineExtension({
  meta: {
    id: 'screenshot',
    name: { 'zh-CN': '截屏', en: 'Screenshot' },
    description: {
      'zh-CN': '截屏、录屏、标注、识别文字',
      en: 'Screenshot, record, annotate, recognize text',
    },
    icon: 'i-ri-screenshot-line',
    keywords: [
      'screenshot',
      '截屏',
      '截图',
      'jietu',
      '录屏',
      '录制',
      'screen record',
      'recording',
      'luping',
      'ocr',
      '识别',
      '文字识别',
      'shibie',
      'qr',
      '二维码',
      'erweima',
      'barcode',
    ],
    order: 110,
  },

  mainView: () => ScreenshotView,
  subviews: { ocr: () => ScreenshotOcr },
  subviewHeights: { ocr: 'auto' },
  disableSearchInput: true,
  setup: () => {
    // 录屏参数权威在 Rust（overlay 是子窗口，无 store 权限、config 为纯内存默认值）。
    // setup 只在 main 窗口执行：把持久化配置（含启动回填）推给 Rust，overlay 只发选区。
    // 纯浏览器预览（Playwright / bun run dev）没有 Rust 侧，跳过 IPC 同步。
    if (!isTauri) return
    watch(
      () =>
        [
          config.savePath,
          config.recordFps,
          config.recordSystemAudio,
          config.recordMicrophone,
          config.openClipAfterRecord,
        ] as const,
      ([savePath, fps, systemAudio, microphone, openClip]) => {
        void invoke(CMD.setScreenshotRecordConfig, {
          savePath,
          fps,
          systemAudio,
          microphone,
          openClip,
        }).catch((e) => console.error('sync screenshot record config failed:', e))
      },
      { immediate: true },
    )
  },
  globalShortcuts: [
    {
      id: 'screenshot',
      default: 'Alt+S',
      onExecute: () => {
        // Rust 端 hook 已在 shortcut.rs 中处理截屏全流程（capture + enter 模式），
        // 前端 onExecute 此处为占位，确保扩展声明让 App.vue 能注册快捷键。
      },
    },
  ],
  onOpenSubview: (_subviewId: string, payload: unknown) => {
    const d = payload as {
      selX: number
      selY: number
      selW: number
      selH: number
      scale: number
      annotationPng: string
      previewPng?: string
    }
    pendingOcrData.value = {
      selX: d.selX,
      selY: d.selY,
      selW: d.selW,
      selH: d.selH,
      scale: d.scale,
      annotationPng: d.annotationPng,
      previewPng: d.previewPng ?? '',
    }
  },
  onExecute: async (result) => {
    if (result.data?.openSubview) {
      const appStore = useAppStore()
      appStore.setActiveExtension('screenshot')
      appStore.setSearchQuery('')
      appStore.openSubview('ocr')
    }
  },
})
