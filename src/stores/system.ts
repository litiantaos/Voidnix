import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { t } from '@/runtime/i18n'
import { isTauri } from '@/utils/tauri'

/// 系统级运行时状态（权限 + 开机自启）：启动预查缓存 + 窗口获焦刷新。
/// 设置页只读缓存值（零 IPC、零首帧跳变）。获焦刷新覆盖用户从系统设置改完权限返回的场景
/// （权限变更唯一入口是系统设置，返回必经窗口获焦；Rust 侧 preflight 纳秒级，开销可忽略）。
export const useSystemStore = defineStore('system', () => {
  const permScreenRecording = ref<boolean | null>(null)
  const permAccessibility = ref<boolean | null>(null)
  const permFullDiskAccess = ref<boolean | null>(null)
  const autostartEnabled = ref<boolean>(false)
  // 公证状态（Rust 侧一次性缓存，恒定）：未公证时辅助功能/屏幕录制的 API 请求路径
  // 写入的 TCC 条目无效，授权入口据此分流
  const appNotarized = ref<boolean | null>(null)

  // in-flight 去重：启动预查与首次获焦刷新可能重叠，复用同一 Promise 避免重复 IPC（5 路→不翻倍）。
  let refreshing: Promise<void> | null = null

  /// 并行查询五项系统状态，各自独立容错（单项失败不牵连其余）。
  /// screen_recording 走 CGPreflightScreenCaptureAccess 不截屏、纳秒级；notarized
  /// 首次为 xcrun 子进程检测（命令 async + spawn_blocking，不占主线程），此后走缓存。
  function refresh(): Promise<void> {
    if (!isTauri) return Promise.resolve()
    if (refreshing) return refreshing
    refreshing = Promise.all([
      invoke<boolean>(CMD.checkAppNotarized)
        .then((v) => (appNotarized.value = v))
        .catch(() => {}),
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

  // 授权会话激活 kind（Rust open_privacy_settings 起，perm-flow 事件终）：
  // 会话期间主窗钉住——失焦/点击外部/前台切换均不隐藏，直到授权完成或超时
  const permGrantKind = ref<string | null>(null)

  /// 发起授权会话：钉住主窗 + 打开系统设置对应面板 + 显示拖拽指引浮窗（三个权限
  /// 通用：列表中找 Voidnix 开开关，或拖左侧图标进列表）。钉住须先于设置激活
  /// 置位（防首个失焦抢先藏窗）；浮窗由 perm-flow（会话结束）统一收起。
  function startPermGrant(kind: string) {
    if (!isTauri) return
    permGrantKind.value = kind
    invoke(CMD.showPermDragHint, { text: t('common.permDragHint') }).catch(() => {})
    invoke(CMD.openPrivacySettings, { kind }).catch(() => {})
  }

  return {
    permScreenRecording,
    permAccessibility,
    permFullDiskAccess,
    appNotarized,
    autostartEnabled,
    refresh,
    permGrantKind,
    startPermGrant,
  }
})
