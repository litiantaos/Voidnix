import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { defineConfig } from '@/runtime/storage'

export type AwakeScreenPolicy = 'sleep' | 'dim'

/// awake 扩展自管配置（持久化至 extensions/awake/config.json）。
/// enabled 是意图状态：Rust 侧 watchdog 随 flag 文件异步对齐系统真实状态。
export const config = defineConfig('extensions/awake/config', {
  enabled: false,
  /// 菜单栏快捷开关常驻显示（不随 enabled 显隐）
  menubarToggleVisible: false,
  /// 合盖熄屏策略：dim=零亮度（背光灭、framebuffer 活跃，远程可用）；
  /// sleep=显示睡眠（更省电，屏幕捕获流冻结）
  screenPolicy: 'dim' as AwakeScreenPolicy,
})

/// Rust 状态同步：enabled 走 watch(immediate: true) 推送到 Rust。
/// immediate 关键：启动期磁盘值回填触发 watch，实现「开机后自动恢复持有」
/// （watchdog 生命周期绑定 app 进程，重启后需重新授权一次并重建持有）。
/// Rust 端 set_awake_enabled 幂等，回声与重复触发零副作用；
/// 失败（含授权取消）回写 false 纠偏，防止「配置说开、系统实际没开」的漂移
/// 一直留到下次启动反复弹窗。
watch(
  () => config.enabled,
  async (enabled) => {
    try {
      await invoke(CMD.setAwakeEnabled, { enabled })
    } catch (e: unknown) {
      console.error('[awake] setAwakeEnabled failed:', e)
      if (enabled) config.enabled = false
    }
  },
  { immediate: true },
)

/// menubarToggleVisible 同步（Config 字段型：View 仅改 config，Rust 侧开关段显隐）
watch(
  () => config.menubarToggleVisible,
  (visible) => {
    invoke(CMD.setAwakeMenubarVisible, { visible }).catch((e: unknown) => {
      console.error('[awake] setAwakeMenubarVisible failed:', e)
    })
  },
  { immediate: true },
)

/// screenPolicy 同步（Config 字段型：View 仅改 config，Rust 侧熄屏策略切换）
watch(
  () => config.screenPolicy,
  (policy) => {
    invoke(CMD.setAwakeScreenPolicy, { policy }).catch((e: unknown) => {
      console.error('[awake] setAwakeScreenPolicy failed:', e)
    })
  },
  { immediate: true },
)

/// Rust 侧发起的关闭（菜单栏开关、电池护栏解除）同步回写 config：
/// 模块级 listener 不依赖 View 挂载，保证事件不因视图未激活而丢失；
/// 漏写会使重启后 watch immediate 误重新持有（弹授权）。
/// 菜单栏切换的熄屏策略同款回写（防 View select 与实际脱钩）。
if (isTauri) {
  listen<boolean>('awake-enabled', (e) => {
    if (!e.payload && config.enabled) config.enabled = false
  }).catch((e: unknown) => {
    console.error('[awake] listen awake-enabled failed:', e)
  })
  listen<string>('awake-policy', (e) => {
    if (e.payload === 'sleep' || e.payload === 'dim') {
      if (config.screenPolicy !== e.payload) config.screenPolicy = e.payload
    }
  }).catch((e: unknown) => {
    console.error('[awake] listen awake-policy failed:', e)
  })
}
