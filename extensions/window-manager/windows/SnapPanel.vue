<template>
  <div
    class="pt-12 flex h-screen w-screen items-start justify-center"
    style="-webkit-app-region: no-drag"
  >
    <div
      ref="panel"
      class="panel p-3 border border-gray-300 rounded-xl bg-[#f7f7fa] flex gap-2.5 shadow-2xl"
    >
      <div v-for="group in groups" :key="group.id" class="p-1 h-14 w-14">
        <template v-if="group.nested">
          <div class="h-full w-full relative">
            <div
              class="snap-zone rounded bg-black/7 inset-0 absolute hover:bg-black/18"
              @click="onZone(group.zones[0].layout)"
            />
            <div
              class="snap-zone rounded bg-black/7 h-40% w-40% left-30% top-30% absolute z-1 hover:bg-black/18"
              @click.stop="onZone(group.zones[1].layout)"
            />
          </div>
        </template>
        <template v-else>
          <div :class="['w-full h-full grid gap-[3px]', group.gridClass]">
            <template v-for="zone in group.zones" :key="zone.layout">
              <div
                v-if="zone.layout === 'custom'"
                class="snap-zone text-black/25 rounded bg-black/7 flex items-center justify-center hover:text-black/45 hover:bg-black/18"
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
                class="snap-zone rounded bg-black/7 hover:bg-black/18"
                @click="onZone(zone.layout)"
              />
            </template>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { commands } from '@/bindings'

interface SnapData {
  w: number
  h: number
}

const snapData = ref<SnapData>({ w: 800, h: 600 })
const panel = ref<HTMLElement>()

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
    await commands.setFrontmostWindowLayout(layout, snapData.value.w, snapData.value.h, null)
  } catch (e) {
    console.warn('[window-manager] 布局执行失败:', e)
  }
}

async function handleShow() {
  const data = (window as unknown as { __snapPanelData?: SnapData }).__snapPanelData
  if (data) snapData.value = data
  await commands.showSnapPanel()
  requestAnimationFrame(() => panel.value?.classList.add('show'))
}

function handleHide() {
  if (!panel.value) return
  const el = panel.value
  el.addEventListener('transitionend', () => commands.hideSnapPanel().catch(() => {}), {
    once: true,
  })
  el.classList.remove('show')
}

onMounted(() => {
  document.body.style.overflow = 'visible'
  window.addEventListener('__snap_panel_show', handleShow)
  window.addEventListener('__snap_panel_hide', handleHide)
})

onUnmounted(() => {
  document.body.style.overflow = ''
  window.removeEventListener('__snap_panel_show', handleShow)
  window.removeEventListener('__snap_panel_hide', handleHide)
})
</script>

<style>
.panel {
  opacity: 0;
  transform: scale(0.8);
  transition: all 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.panel.show {
  opacity: 1;
  transform: scale(1);
}
</style>
