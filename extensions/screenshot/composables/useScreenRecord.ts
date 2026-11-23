import { onScopeDispose, ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { showToast } from '@/composables/useToast'
import { t } from '@/runtime/i18n'
import type { RecordPrefs, Sel } from './useTypes'

/** 工具栏「录屏」：3 秒倒计时后启动 ScreenCaptureKit 采集当前选区并关闭 overlay。
 * 倒计时期间 Esc 关 overlay（组件卸载）即取消，不会启动。 */
export function useScreenRecord(options: { sel: Ref<Sel>; dpr: Ref<number>; onClose: () => void }) {
  const starting = ref(false)
  const countdown = ref(0)
  let timer: ReturnType<typeof setTimeout> | undefined
  let resolveCountdown: (() => void) | undefined
  let generation = 0

  function stopCountdown() {
    generation += 1
    if (timer) {
      clearTimeout(timer)
      timer = undefined
    }
    countdown.value = 0
    const resolve = resolveCountdown
    resolveCountdown = undefined
    resolve?.()
  }

  function beginCountdown(): Promise<void> {
    return new Promise((resolve) => {
      const my = ++generation
      countdown.value = 3
      resolveCountdown = resolve
      const tick = () => {
        if (my !== generation) return
        if (countdown.value > 1) {
          countdown.value -= 1
          timer = setTimeout(tick, 1000)
          return
        }
        countdown.value = 0
        timer = undefined
        resolveCountdown = undefined
        resolve()
      }
      timer = setTimeout(tick, 1000)
    })
  }

  async function start(prefs: RecordPrefs) {
    if (starting.value) return
    const { x, y, w, h } = options.sel.value
    if (w < 8 || h < 8) return
    starting.value = true
    await beginCountdown()
    if (!starting.value) return
    try {
      await invoke(CMD.screenRecordStart, {
        selX: x,
        selY: y,
        selW: w,
        selH: h,
        scale: options.dpr.value,
        prefs,
      })
      // 与截屏共用「上次选区」：下次截屏/录屏 R 键可恢复
      void invoke(CMD.saveLastSelection, { selX: x, selY: y, selW: w, selH: h }).catch(() => {})
      void showToast(t('screenshot.recordingStarted'), { duration: 3000 })
      options.onClose()
    } catch (e) {
      starting.value = false
      void showToast(String(e), { kind: 'error', duration: 4000 })
    }
  }

  onScopeDispose(() => {
    starting.value = false
    stopCountdown()
  })

  return { starting, countdown, start }
}
