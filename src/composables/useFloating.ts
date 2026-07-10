import { ref, watch, onUnmounted, computed, nextTick, type Ref } from 'vue'
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

export function useFloating(
  referenceRef: Ref<HTMLElement | null>,
  floatingRef: Ref<HTMLElement | null>,
  options: UseFloatingOptions,
) {
  const floatingStyles = ref<Record<string, string>>({
    position: 'fixed',
    top: '-9999px',
    left: '-9999px',
  })
  const resolvedPlacement = ref<Placement>(options.placement ?? 'bottom-start')

  let cleanup: (() => void) | null = null

  function update() {
    const reference = referenceRef.value
    const floating = floatingRef.value
    if (!reference || !floating) return

    const padding = options.padding ?? 12

    computePosition(reference, floating, {
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
      resolvedPlacement.value = p
      floatingStyles.value = {
        position: 'fixed',
        top: `${y}px`,
        left: `${x}px`,
        zIndex: '9999',
        ...(options.matchWidth ? { minWidth: `${reference.offsetWidth}px` } : {}),
      }
    })
  }

  watch(options.isOpen, (open) => {
    if (open) {
      nextTick(() => {
        const reference = referenceRef.value
        const floating = floatingRef.value
        if (!reference || !floating) return
        cleanup?.()
        cleanup = autoUpdate(reference, floating, update)
      })
    } else {
      cleanup?.()
      cleanup = null
    }
  })

  onUnmounted(() => {
    cleanup?.()
    cleanup = null
  })

  return {
    floatingStyles,
    dropUp: computed(() => resolvedPlacement.value.startsWith('top')),
  }
}
