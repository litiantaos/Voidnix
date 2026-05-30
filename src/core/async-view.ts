import { defineAsyncComponent, type AsyncComponentLoader, type Component } from 'vue'

const pending = new Set<AsyncComponentLoader>()

/**
 * 注册一个异步视图组件。与 `defineAsyncComponent` 等价，但额外把 loader 收集起来，
 * 由 `preloadAllViews()` 在应用空闲时一次性并行预热，消除首次激活时的 chunk 拉取卡顿。
 */
export function asyncView<T extends Component = Component>(loader: AsyncComponentLoader<T>) {
  pending.add(loader as AsyncComponentLoader)
  return defineAsyncComponent(loader)
}

/** 并行触发所有注册过的 loader；只在启动时调用一次。 */
export function preloadAllViews(): void {
  for (const loader of pending) {
    void loader().catch((e: unknown) => console.error('[async-view] preload failed', e))
  }
  pending.clear()
}
