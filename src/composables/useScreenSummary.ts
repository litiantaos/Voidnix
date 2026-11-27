import { inject, onMounted, onUnmounted, type ComputedRef, type InjectionKey } from 'vue'
import { useAppStore } from '@/stores/app'

/** 当前渲染屏的身份：与 ContentView 的动态组件 key 同源（main = subview null）。 */
export interface ScreenContext {
  extId: string
  subview: string | null
}

/** ContentView 在动态视图子树根部 provide；View 侧 useScreenSummary 消费。
 *  用 computed 提供，View 挂载时读到的是本次渲染对应的屏（main / subview）。 */
export const SCREEN_CONTEXT_KEY: InjectionKey<ComputedRef<ScreenContext>> =
  Symbol('voidnix.screen-context')

/**
 * 注册当前屏的最近使用副标题供给（挂载注册 / 卸载注销；KeepAlive 停用不注销——现场仍在，
 * 摘要读的是当前状态）。屏身份由 ContentView 注入，View 不再手写 extId + subview 字符串。
 *
 * 仅供 ContentView 动态视图内的 mainView / subviews 使用；脱离该子树（独立窗口等）调用
 * 不注册（inject 缺省），零副作用。
 */
export function useScreenSummary(provider: () => string | null | undefined): void {
  const ctx = inject(SCREEN_CONTEXT_KEY, null)
  if (!ctx) return
  // setup 时快照屏身份：组件按 ContentView 的动态 key 创建，注册/注销必须用同一 key。
  // 不在 onMounted 读取 ctx.value——post-flush 阶段 activeSubview 若已变化，会注册到错误屏。
  const screen = ctx.value
  if (!screen.extId) return
  const appStore = useAppStore()
  onMounted(() => appStore.registerScreenSummary(screen.extId, screen.subview, provider))
  onUnmounted(() => {
    appStore.unregisterScreenSummary(screen.extId, screen.subview, provider)
  })
}
