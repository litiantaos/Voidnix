<template>
  <div
    class="radius-panel mica-tint"
    h="screen"
    w="screen"
    p="3"
    flex
    items="center"
    gap="3"
    overflow="hidden"
    style="-webkit-app-region: no-drag"
  >
    <div v-for="group in groups" :key="group.id" p="1" h="14" w="14">
      <template v-if="group.nested">
        <div h="full" w="full" relative>
          <div
            class="snap-zone radius-ctrl fill-ctrl"
            inset="0"
            absolute
            :class="{ 'ui-active snap-hover': hoveredLayout === group.zones[0].layout }"
            :data-layout="group.zones[0].layout"
            @click="onZone(group.zones[0].layout)"
          />
          <div
            class="snap-zone radius-ctrl fill-ctrl"
            h="40%"
            w="40%"
            left="30%"
            top="30%"
            absolute
            z="1"
            :class="{ 'ui-active snap-hover': hoveredLayout === group.zones[1].layout }"
            :data-layout="group.zones[1].layout"
            @click.stop="onZone(group.zones[1].layout)"
          />
        </div>
      </template>
      <template v-else>
        <div :class="['w-full h-full grid gap-0.5', group.gridClass]">
          <template v-for="zone in group.zones" :key="zone.layout">
            <div
              v-if="zone.layout === 'custom'"
              class="snap-zone custom-zone radius-ctrl fill-ctrl flex-center"
              text="muted"
              :class="{ 'ui-active snap-hover': hoveredLayout === zone.layout }"
              :data-layout="zone.layout"
              @click="onZone(zone.layout)"
            >
              <svg
                viewBox="0 0 20 20"
                width="20"
                height="20"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <path
                  d="M2 7 L2 4 A2 2 0 0 1 4 2 L7 2 M13 2 L16 2 A2 2 0 0 1 18 4 L18 7 M18 13 L18 16 A2 2 0 0 1 16 18 L13 18 M7 18 L4 18 A2 2 0 0 1 2 16 L2 13"
                />
              </svg>
            </div>
            <div
              v-else
              class="snap-zone radius-ctrl fill-ctrl"
              :class="{ 'ui-active snap-hover': hoveredLayout === zone.layout }"
              :data-layout="zone.layout"
              @click="onZone(zone.layout)"
            />
          </template>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'

interface SnapData {
  w: number
  h: number
}

const snapData = ref<SnapData>({ w: 800, h: 600 })
const hoveredLayout = ref<string | null>(null)

interface ZoneDef {
  layout: string
}

interface GroupDef {
  id: string
  gridClass?: string
  nested?: boolean
  zones: ZoneDef[]
}

const groups: GroupDef[] = [
  {
    id: 'quarters',
    gridClass: 'grid-cols-2 grid-rows-2',
    zones: [
      { layout: 'top-left' },
      { layout: 'top-right' },
      { layout: 'bottom-left' },
      { layout: 'bottom-right' },
    ],
  },
  {
    id: 'halves-v',
    gridClass: 'grid-rows-2',
    zones: [{ layout: 'top' }, { layout: 'bottom' }],
  },
  {
    id: 'halves-h',
    gridClass: 'grid-cols-2',
    zones: [{ layout: 'left' }, { layout: 'right' }],
  },
  {
    id: 'full-center',
    nested: true,
    zones: [{ layout: 'fullscreen' }, { layout: 'center' }],
  },
  {
    id: 'custom',
    gridClass: '',
    zones: [{ layout: 'custom' }],
  },
]

async function onZone(layout: string) {
  try {
    await invoke(CMD.setFrontmostWindowLayout, {
      layout,
      customWidth: snapData.value.w,
      customHeight: snapData.value.h,
      prevPid: null,
    })
  } catch (e) {
    console.warn('[window-manager] 布局执行失败:', e)
  }
}

function handleSnapMouse() {
  const data = (window as unknown as { __snapMouse?: { x: number; y: number } }).__snapMouse
  if (!data) return
  const el = document.elementFromPoint(data.x, data.y)
  const zone = (el as HTMLElement | null)?.closest('[data-layout]') as HTMLElement | null
  hoveredLayout.value = zone?.dataset.layout ?? null
}

async function handleShow() {
  const data = (window as unknown as { __snapPanelData?: SnapData }).__snapPanelData
  if (data) snapData.value = data
  await invoke(CMD.showSnapPanel)
}

function handleHide() {
  hoveredLayout.value = null
  invoke(CMD.hideSnapPanel).catch(() => {})
}

onMounted(() => {
  window.addEventListener('__snap_panel_show', handleShow)
  window.addEventListener('__snap_panel_hide', handleHide)
  window.addEventListener('__snap_mouse', handleSnapMouse)
})

onUnmounted(() => {
  window.removeEventListener('__snap_panel_show', handleShow)
  window.removeEventListener('__snap_panel_hide', handleHide)
  window.removeEventListener('__snap_mouse', handleSnapMouse)
})
</script>

<style scoped>
/* hover 材质走 ui-active；custom 图标色略加深 */
.snap-zone.custom-zone.snap-hover {
  color: var(--color-text-secondary);
}
</style>
