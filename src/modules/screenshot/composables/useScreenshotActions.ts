import type { Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Sel, Shape } from './useTypes'
import { useSettingsStore } from '@/stores/settings'

export function useScreenshotActions(options: {
  sel: Ref<Sel>
  dpr: Ref<number>
  shapes: Ref<Shape[]>
  annotateCanvas: Ref<HTMLCanvasElement | undefined>
  rootEl: Ref<HTMLElement | undefined>
  emit: (e: 'close', forOcr?: boolean) => void
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
    await invoke('open_ocr_in_main_window', {
      selX: options.sel.value.x,
      selY: options.sel.value.y,
      selW: options.sel.value.w,
      selH: options.sel.value.h,
      scale: options.dpr.value,
      annotationPng: ann,
    })
    doCancel(true)
  }

  async function doPin() {
    const ann = await getAnnotationPng()
    await invoke('pin_image', {
      selX: options.sel.value.x,
      selY: options.sel.value.y,
      selW: options.sel.value.w,
      selH: options.sel.value.h,
      scale: options.dpr.value,
      annotationPng: ann,
    })
    doCancel()
  }

  function doCancel(forOcr = false) {
    if (options.rootEl.value) options.rootEl.value.style.cursor = 'default'
    document.body.style.cursor = 'default'
    requestAnimationFrame(() => {
      document.body.style.cursor = ''
      options.emit('close', forOcr)
    })
  }

  return {
    getAnnotationPng,
    doCopy,
    doSave,
    doOcr,
    doPin,
    doCancel,
  }
}
