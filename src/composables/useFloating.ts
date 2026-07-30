import { watch, onUnmounted, nextTick, type Ref } from 'vue'
import {
  computePosition,
  autoUpdate,
  offset as offsetMiddleware,
  flip,
  shift,
  size,
  type Placement,
} from '@floating-ui/dom'

interface UseFloatingOptions {
  isOpen: Ref<boolean>
  placement?: Placement
  offset?: number
  padding?: number
  matchWidth?: boolean
}

/**
 * 定位 + 进出场动画时序内聚。
 *
 * 跳变根因：Transition 的进场动画（rAF 驱动）与 computePosition 的 resolve（microtask）
 * 竞态——元素可能在位置精确前就开始变可见。治本：让进场动画等到定位完成之后才开始。
 *
 * - 位置直接写入 floating DOM（不经响应式 :style，避开 Vue patch 与 paint 的二次竞态）
 * - onEnter：挂载即 opacity:0 → await 定位（含 flip）→ 淡入；位移方向随 dropUp
 * - 消费者：v-if + <Transition :css="false" @enter @leave>，无需 :style / dropUp
 */
export function useFloating(
  referenceRef: Ref<HTMLElement | null>,
  floatingRef: Ref<HTMLElement | null>,
  options: UseFloatingOptions,
) {
  let cleanup: (() => void) | null = null
  // 进场方向（dropUp）由 flip 决定，update 写入；hooks 在 update 之后读，时序保证
  let dropUp = false

  function update(): Promise<void> {
    const reference = referenceRef.value
    const floating = floatingRef.value
    if (!reference || !floating) return Promise.resolve()
    const padding = options.padding ?? 12
    return computePosition(reference, floating, {
      placement: options.placement ?? 'bottom-start',
      middleware: [
        offsetMiddleware(options.offset ?? 4),
        flip({ padding }),
        shift({ padding }),
        size({
          padding,
          apply({ availableHeight, elements }) {
            Object.assign(elements.floating.style, {
              maxHeight: `${Math.max(availableHeight, 0)}px`,
              overflowY: 'auto',
            })
          },
        }),
      ],
    }).then(({ x, y, placement: p }) => {
      dropUp = p.startsWith('top')
      floating.style.position = 'fixed'
      floating.style.top = `${y}px`
      floating.style.left = `${x}px`
      floating.style.zIndex = '9999'
      if (options.matchWidth) floating.style.minWidth = `${reference.offsetWidth}px`
    })
  }

  // 进场：元素挂载即不可见，定位（含 flip）完成后再淡入——首帧可见即精确位置、方向正确
  function onEnter(el: Element, done: () => void) {
    const node = el as HTMLElement
    node.style.opacity = '0'
    nextTick(async () => {
      const reference = referenceRef.value
      if (!reference) {
        settle(node, done)
        return
      }
      cleanup?.()
      cleanup = autoUpdate(reference, node, update)
      await update()
      // 定位完成，淡入：设初始偏移 → 强制 reflow 生效 → 设终态触发过渡
      node.style.transform = `translateY(${dropUp ? '4px' : '-4px'}) scale(.95)`
      void node.offsetHeight
      node.style.transition =
        'opacity var(--duration-fast) var(--ease-out), transform var(--duration-fast) var(--ease-out)'
      node.style.opacity = '1'
      node.style.transform = ''
      settle(node, done)
    })
  }

  function onLeave(el: Element, done: () => void) {
    const node = el as HTMLElement
    cleanup?.()
    cleanup = null
    node.style.transition = 'opacity 100ms var(--ease-in), transform 100ms var(--ease-in)'
    node.style.opacity = '0'
    node.style.transform = `translateY(${dropUp ? '4px' : '-4px'}) scale(.95)`
    settle(node, done)
  }

  // transitionend 取首次 + 超时兜底（防 transition 被中断致 done 永不触发）
  function settle(node: HTMLElement, done: () => void) {
    let called = false
    const finish = () => {
      if (called) return
      called = true
      done()
    }
    node.addEventListener('transitionend', finish, { once: true })
    setTimeout(finish, 220)
  }

  watch(options.isOpen, (open) => {
    if (!open) {
      cleanup?.()
      cleanup = null
    }
  })

  onUnmounted(() => {
    cleanup?.()
    cleanup = null
  })

  return { onEnter, onLeave }
}
