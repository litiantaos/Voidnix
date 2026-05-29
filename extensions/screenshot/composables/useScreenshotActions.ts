import type { Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Sel, Shape } from './useTypes'
import { useSettingsStore } from '@/stores/settings'

export function useScreenshotActions(options: {
  sel: Ref<Sel>
  dpr: Ref<number>
  shapes: Ref<Shape[]>
  annotateCanvas: Ref<HTMLCanvasElement | undefined>
  bgImage: Ref<HTMLImageElement | null>
  rootEl: Ref<HTMLElement | undefined>
  emit: (e: 'close', noRestoreFocus?: boolean) => void
}) {
  async function getAnnotationPng(): Promise<string> {
    if (!options.annotateCanvas.value || options.shapes.value.length === 0) return ''
    return options.annotateCanvas.value.toDataURL('image/png')
  }

  async function doCopy() {
    const ann = await getAnnotationPng()
    await invoke('copy_screenshot_to_clipboard', {
      selX: options.sel.value.x,
      selY: options.sel.value.y,
      selW: options.sel.value.w,
      selH: options.sel.value.h,
      scale: options.dpr.value,
      annotationPng: ann,
    })
    doCancel()
  }

  async function doSave() {
    const ann = await getAnnotationPng()
    const settings = useSettingsStore()
    const savePath = settings.screenshotSavePath || '~/Downloads'
    const path = savePath.startsWith('~/')
      ? savePath.replace(
          '~',
          await invoke<string>('get_home_dir').catch(() => ''),
        )
      : savePath
    await invoke('save_screenshot', {
      selX: options.sel.value.x,
      selY: options.sel.value.y,
      selW: options.sel.value.w,
      selH: options.sel.value.h,
      scale: options.dpr.value,
      annotationPng: ann,
      path,
    })
    doCancel()
  }

  async function doOcr() {
    const ann = await getAnnotationPng()

    // 前端合成预览图（底图 + 标注层）
    let previewPng = ''
    const bg = options.bgImage.value
    const ac = options.annotateCanvas.value
    if (bg && ac) {
      const canvas = document.createElement('canvas')
      const dpr = options.dpr.value
      const { x, y, w, h } = options.sel.value
      const cw = Math.round(w * dpr)
      const ch = Math.round(h * dpr)
      canvas.width = cw
      canvas.height = ch
      const ctx = canvas.getContext('2d')!
      ctx.drawImage(bg, x * dpr, y * dpr, cw, ch, 0, 0, cw, ch)
      ctx.drawImage(ac, 0, 0, cw, ch, 0, 0, cw, ch)
      previewPng = canvas.toDataURL('image/png')
    }

    await invoke('open_module_panel', {
      moduleId: 'screenshot',
      payload: {
        selX: options.sel.value.x,
        selY: options.sel.value.y,
        selW: options.sel.value.w,
        selH: options.sel.value.h,
        scale: options.dpr.value,
        annotationPng: ann,
        previewPng,
      },
    })
    doCancel(true)
  }

  async function doPin() {
    const ann = await getAnnotationPng()
    // 钉图窗口创建走主线程 webview build，耗时约 100ms；
    // 这里不 await，立刻 doCancel(true) 让截屏窗口先开始 fade out，
    // 钉图窗口在 fade 期间出现，整体观感更连贯。
    // noRestoreFocus=true：exit_impl 不重新激活上一个 app，焦点能留在钉图窗口。
    invoke('pin_image', {
      selX: options.sel.value.x,
      selY: options.sel.value.y,
      selW: options.sel.value.w,
      selH: options.sel.value.h,
      scale: options.dpr.value,
      annotationPng: ann,
    }).catch((err) => {
      console.error('pin_image failed:', err)
    })
    doCancel(true)
  }

  /// 滚动截屏：把 finish_scroll_capture 返回的 dataURL 复制到剪贴板，然后退出。
  async function doScrollCopy(dataUrl: string) {
    if (!dataUrl) {
      doCancel()
      return
    }
    await invoke('copy_scroll_result_to_clipboard', { resultDataUrl: dataUrl })
    doCancel()
  }

  /// 滚动截屏：保存到设置目录。
  async function doScrollSave(dataUrl: string) {
    if (!dataUrl) {
      doCancel()
      return
    }
    const settings = useSettingsStore()
    const savePath = settings.screenshotSavePath || '~/Downloads'
    const path = savePath.startsWith('~/')
      ? savePath.replace('~', await invoke<string>('get_home_dir').catch(() => ''))
      : savePath
    await invoke('save_scroll_result', { resultDataUrl: dataUrl, path })
    doCancel()
  }

  function doCancel(noRestoreFocus = false) {
    if (options.rootEl.value) options.rootEl.value.style.cursor = 'default'
    document.body.style.cursor = 'default'
    requestAnimationFrame(() => {
      document.body.style.cursor = ''
      options.emit('close', noRestoreFocus)
    })
  }

  return {
    getAnnotationPng,
    doCopy,
    doSave,
    doOcr,
    doPin,
    doScrollCopy,
    doScrollSave,
    doCancel,
  }
}
