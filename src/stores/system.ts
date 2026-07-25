import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'

/// 系统级运行时状态（权限 + 开机自启）：启动预查缓存 + 窗口获焦刷新。
/// 设置页只读缓存值（零 IPC、零首帧跳变）。获焦刷新覆盖用户从系统设置改完权限返回的场景
/// （权限变更唯一入口是系统设置，返回必经窗口获焦；Rust 侧 preflight 纳秒级，开销可忽略）。
export const useSystemStore = defineStore('system', () => {
  const permScreenRecording = ref<boolean | null>(null)
  const permAccessibility = ref<boolean | null>(null)
  const permFullDiskAccess = ref<boolean | null>(null)
  const autostartEnabled = ref<boolean>(false)

  // in-flight 去重：启动预查与首次获焦刷新可能重叠，复用同一 Promise 避免重复 IPC（4 路→不翻倍）。
  let refreshing: Promise<void> | null = null

  /// 并行查询四项系统状态，各自独立容错（单项失败不牵连其余）。
  /// Rust 侧均同步纳秒/微秒级（screen_recording 走 CGPreflightScreenCaptureAccess 不截屏）。
  function refresh(): Promise<void> {
    if (!isTauri) return Promise.resolve()
    if (refreshing) return refreshing
    refreshing = Promise.all([
      invoke<boolean>(CMD.checkScreenRecordingPermission)
        .then((v) => (permScreenRecording.value = v))
        .catch(() => {}),
      invoke<boolean>(CMD.checkAccessibilityPermission)
        .then((v) => (permAccessibility.value = v))
        .catch(() => {}),
      invoke<boolean>(CMD.checkFullDiskAccessPermission)
        .then((v) => (permFullDiskAccess.value = v))
        .catch(() => {}),
      invoke<boolean>(CMD.isAutostartEnabled)
        .then((v) => (autostartEnabled.value = v))
        .catch(() => {}),
    ]).then(() => {
      refreshing = null
    })
    return refreshing
  }

  return {
    permScreenRecording,
    permAccessibility,
    permFullDiskAccess,
    autostartEnabled,
    refresh,
  }
})
