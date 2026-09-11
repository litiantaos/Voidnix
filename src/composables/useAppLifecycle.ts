import { onMounted, onUnmounted, watch, type WatchStopHandle } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { CMD } from '@/commands'
import { listen } from '@tauri-apps/api/event'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { useSystemStore } from '@/stores/system'
import { UPDATE } from '@/runtime/constants'
import { whenConfigReady } from '@/runtime/storage'
import { t, resolveLocalized } from '@/runtime/i18n'
import { formatShortcutKeys } from '@/utils/format'
import { isTauri, hideWindow, showWindow } from '@/utils/tauri'
import { getAllExtensions, getExtension } from '@/runtime/extension-registry'

type Win = ReturnType<typeof import('@tauri-apps/api/window').getCurrentWindow> | null

/// 主窗口生命周期：全局快捷键注册/注销、窗口显隐、失焦防抖隐藏、扩展/子视图事件监听。
export function useAppLifecycle(win: Win) {
  const settings = useSettingsStore()
  const appStore = useAppStore()
  const updateStore = useUpdateStore()

  function effectiveShortcut(id: string, fallback?: string): string {
    return settings.getShortcutOverride(id) || fallback || ''
  }

  let lastShortcutTime = 0
  function markSkip() {
    lastShortcutTime = Date.now()
  }

  async function toggleWindow() {
    if (!win) return
    const visible = await win.isVisible()
    if (visible) {
      hideWindow()
    } else {
      await win.show()
      await win.setFocus()
    }
  }

  function onLocalShortcut(e: KeyboardEvent) {
    if (appStore.shortcutRecording) return
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === 'Space') {
      e.preventDefault()
      markSkip()
      toggleWindow()
    }
  }

  // 统一管理事件订阅的清理函数，避免新增监听时漏接 unlisten（曾导致 open-extension-subview 泄漏）
  const unlistenList: Array<() => void> = []
  const track = (fn: () => void): void => {
    unlistenList.push(fn)
  }
  // M-fe1：watch stop handle + update 检查 timer 一并纳入清理，避免 HMR/测试场景累积
  const watchStops: WatchStopHandle[] = []
  let updateTimer: ReturnType<typeof setTimeout> | null = null
  let lastCheckAt = 0
  let allGlobalShortcuts: {
    id: string
    default?: string
    onExecute: (wasVisible: boolean) => void
  }[] = []

  async function setupGlobalShortcut(id: string, shortcut: string) {
    if (!isTauri) return
    try {
      await invoke(CMD.registerGlobalShortcut, { id, shortcut })
      appStore.clearShortcutError(id)
    } catch (e) {
      const msg = String(e)
      appStore.setShortcutError(id, msg)
    }
  }

  /// 启动注册失败（快捷键被 Raycast 等占用）改键引导：列出失败项，一键直达设置。
  /// 任何失败均 show 窗口承载引导——扩展快捷键冲突同样静默失效（用户无感知），
  /// 且隐藏窗口中的弹窗会被「切扩展按取消收束」路径（如扩展快捷键唤起）静默吞掉。
  async function guideShortcutConflicts() {
    const entries = Object.entries(appStore.shortcutErrors)
    if (entries.length === 0) return
    void showWindow()
    const lines = entries.map(([id]) => {
      if (id === 'main') {
        return `- ${formatShortcutKeys(settings.globalShortcut).join(' ')} — ${t('settings.shortcut')}`
      }
      const owner = getAllExtensions().find((e) => e.globalShortcuts?.some((s) => s.id === id))
      const binding = owner?.globalShortcuts?.find((s) => s.id === id)
      const keys = formatShortcutKeys(effectiveShortcut(id, binding?.default)).join(' ')
      const name = owner ? resolveLocalized(owner.meta.name) : id
      return `- ${keys || id} — ${name}`
    })
    const ok = await appStore.showConfirm({
      title: t('shortcut.conflictTitle'),
      message: `${t('shortcut.conflictBody')}\n\n${lines.join('\n')}`,
      markdown: true,
      size: 'md',
      okLabel: t('shortcut.conflictOpenSettings'),
      cancelLabel: t('shortcut.conflictLater'),
    })
    if (ok) appStore.setActiveExtension('settings')
  }

  // 唤起节流：窗口获焦触发，冷却期内秒退。lastCheckAt 先记后查防并发重入。
  // 仅静默检查（发现更新后显示搜索栏入口按钮，下载由设置页弹窗驱动）；
  // 已知有更新则不再重复检查。check() 内部已 catch，外层仅兜底。失败也计入冷却。
  async function maybeCheckUpdate() {
    if (updateStore.info) return
    if (Date.now() - lastCheckAt < UPDATE.checkIntervalMs) return
    lastCheckAt = Date.now()
    try {
      await updateStore.check()
    } catch (e) {
      console.error('Update check failed:', e)
    }
  }

  onMounted(async () => {
    // settings store 走 defineConfig，启动时自动异步加载（无需显式 loadSettings）

    if (isTauri) {
      // 等配置回填完成再注册快捷键：注册一次即为最终值（避免「默认值注册 → 回填再注册」抖动），
      // 注册失败检测（guideShortcutConflicts）也因此读到确定结果
      await whenConfigReady('config/settings')

      updateTimer = setTimeout(() => {
        void maybeCheckUpdate()
      }, 3000)

      // 首启引导：未完成 onboarding 时自动唤出主窗口展示引导卡（新用户不知晓呼出快捷键）。
      // 自测门控读 appStore.selfTestMode：is_self_test_mode 是一次性命令，首次 invoke 已被
      // main.ts 消费（再 invoke 恒 false）；置位经单次 IPC，先于多轮 IPC 的配置回填落定
      if (!appStore.selfTestMode && !settings.onboarded) void showWindow()

      await setupGlobalShortcut('main', settings.globalShortcut)

      allGlobalShortcuts = getAllExtensions()
        .flatMap((e) => e.globalShortcuts || [])
        .filter((s) => s.id !== 'main')

      for (const sc of allGlobalShortcuts) {
        await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
      }

      // 关键快捷键注册失败 → 改键引导（自测模式下窗口由测试脚本驱动，跳过）
      if (!appStore.selfTestMode) void guideShortcutConflicts()

      watchStops.push(
        watch(
          () => settings.globalShortcut,
          async (newVal) => {
            await setupGlobalShortcut('main', newVal)
          },
        ),
      )

      watchStops.push(
        watch(
          () => settings.shortcutOverrides,
          async () => {
            for (const sc of allGlobalShortcuts) {
              await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
            }
          },
          { deep: true },
        ),
      )

      const unlistenShortcut = await listen<{ id: string; wasVisible: boolean }>(
        'shortcut-pressed',
        async (event) => {
          if (appStore.shortcutRecording) return
          markSkip()
          const shortcutId = event.payload.id
          const wasVisible = event.payload.wasVisible

          if (shortcutId === 'main') {
            if (wasVisible) {
              hideWindow()
              return
            }
            // 主快捷键从隐藏唤起：通知搜索层检查剪贴板填充
            window.dispatchEvent(new CustomEvent('window-invoked'))
          } else {
            // 扩展快捷键动态分发
            for (const ext of getAllExtensions()) {
              if (ext.globalShortcuts) {
                const sc = ext.globalShortcuts.find((s) => s.id === shortcutId)
                if (sc) {
                  sc.onExecute(wasVisible)
                  return
                }
              }
            }
          }
        },
      )
      track(unlistenShortcut)

      const unlistenOpenExtension = await listen<{ id: string; wasVisible: boolean }>(
        'open-extension',
        (event) => {
          markSkip()
          const { id: extId, wasVisible } = event.payload
          if (extId) {
            appStore.setActiveExtension(extId)
            appStore.setSearchQuery('')
            // 窗口隐藏时（菜单栏点击）：先切视图再 show，避免渲染旧视图闪现
            if (!wasVisible) void showWindow()
          }
        },
      )
      track(unlistenOpenExtension)

      const unlistenClickOutside = await listen('click-outside', () => {
        hideWindow(true)
      })
      track(unlistenClickOutside)

      // 系统弹窗关闭后用户切到其他 app（frontmost ≠ 原前台 app）→ dismiss
      const unlistenFrontmostChanged = await listen('frontmost-changed', () => {
        hideWindow(true)
      })
      track(unlistenFrontmostChanged)

      // 通用扩展子视图事件：任何扩展都可以通过 Rust `open_extension_subview` 触发
      const unlistenSubview = await listen<{
        extId: string
        subviewId: string
        payload: unknown
        wasVisible: boolean
      }>('open-extension-subview', (e) => {
        markSkip()
        const { extId, subviewId, payload, wasVisible } = e.payload
        appStore.setActiveExtension(extId)
        appStore.setSearchQuery('')
        appStore.openSubview(subviewId, true)
        const ext = getExtension(extId)
        if (ext?.onOpenSubview) {
          ext.onOpenSubview(subviewId, payload)
        }
        // 窗口隐藏时：先切视图再 show，避免渲染旧视图闪现
        if (!wasVisible) void showWindow()
      })
      track(unlistenSubview)

      const unlistenFocus = await win!.onFocusChanged(
        ({ payload: focused }: { payload: boolean }) => {
          if (focused) {
            window.dispatchEvent(new CustomEvent('window-focused'))
            // 主动接完 responder 链（wry: makeFirstResponder(webview)，不 activate_app）：
            // 无激活唤起下窗口 key 聚焦后 WKWebView 页面焦点状态要迟些才自然翻转，
            // 显式落位使其当帧翻转——输入框聚焦/唤起全选（selectAllWhenPageFocused 双通道
            // 探测）随即就位，消除「窗口已显示、选中/输入稍后才到」的滞后
            void getCurrentWebview()
              .setFocus()
              .catch(() => {})
            // 系统对话框（文件选择器 / 授权弹窗）返回时解除抑制
            appStore.suppressBlur = false
            // 刷新系统状态（权限/自启）：覆盖用户从系统设置改完权限返回的场景。
            // 权限变更唯一入口是系统设置，返回必经窗口获焦；Rust 侧 preflight 纳秒级，单次开销可忽略。
            useSystemStore().refresh()
            // 唤起节流：冷却期内秒退，无网络开销
            void maybeCheckUpdate()
          } else if (
            Date.now() - lastShortcutTime > 200 &&
            Date.now() - appStore.lastDialogCloseTime > 300 &&
            !appStore.isDialogOpen &&
            !appStore.suppressBlur
          ) {
            invoke<boolean>(CMD.isAppActive)
              .then((active) => {
                if (active) return
                hideWindow(true)
              })
              .catch(() => {
                hideWindow(true)
              })
          }
        },
      )
      track(unlistenFocus)
    }

    document.addEventListener('keydown', onLocalShortcut)
  })

  onUnmounted(() => {
    document.removeEventListener('keydown', onLocalShortcut)
    // M-fe1：清理 watch + timer，与 unlisten 一并回收
    watchStops.forEach((stop) => stop())
    watchStops.length = 0
    if (updateTimer) {
      clearTimeout(updateTimer)
      updateTimer = null
    }
    if (isTauri) {
      unlistenList.forEach((fn) => {
        try {
          fn()
        } catch (e) {
          console.error('unlisten failed:', e)
        }
      })
      unlistenList.length = 0

      // 同步触发注销（fire-and-forget）：Tauri app 退出前会等待当前 task tick
      void invoke(CMD.registerGlobalShortcut, {
        id: 'main',
        shortcut: '',
      }).catch(() => {})

      for (const sc of allGlobalShortcuts) {
        void invoke(CMD.registerGlobalShortcut, {
          id: sc.id,
          shortcut: '',
        }).catch(() => {})
      }
    }
  })
}
