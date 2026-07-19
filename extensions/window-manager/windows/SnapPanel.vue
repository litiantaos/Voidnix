<template>
  <!-- 铺满固定尺寸窗口；进出场由原生 alpha + 位移动画，前端不参与 -->
  <div
    class="snap-panel p-3 radius-panel flex gap-3 h-screen w-screen mica-tint items-center overflow-hidden"
    style="-webkit-app-region: no-drag"
  >
    <div v-for="group in groups" :key="group.id" class="snap-group p-1 shrink-0 h-14 w-14">
      <template v-if="group.nested">
        <div class="snap-nested" h="full" w="full" relative>
          <!-- 全屏：中心镂空环；居中：cell×0.78 压小，同色 fill-8 -->
          <div
            class="snap-zone snap-ring"
            inset="0"
            absolute
            :class="{ 'snap-hover': hoveredLayout === group.zones[0].layout }"
            :data-layout="group.zones[0].layout"
            @click="onZone(group.zones[0].layout)"
          />
          <div
            class="snap-zone snap-inset"
            absolute
            z="1"
            :class="{ 'snap-hover': hoveredLayout === group.zones[1].layout }"
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
              class="snap-zone custom-zone flex-center"
              :class="{ 'snap-hover': hoveredLayout === zone.layout }"
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
              v-else-if="zone.layout === 'prev-display' || zone.layout === 'next-display'"
              class="snap-zone custom-zone flex-center"
              :class="{ 'snap-hover': hoveredLayout === zone.layout }"
              :data-layout="zone.layout"
              @click="onZone(zone.layout)"
            >
              <svg
                viewBox="0 0 20 20"
                width="16"
                height="16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path v-if="zone.layout === 'prev-display'" d="M12 4 L6 10 L12 16" />
                <path v-else d="M8 4 L14 10 L8 16" />
              </svg>
            </div>
            <div
              v-else
              class="snap-zone"
              :class="{ 'snap-hover': hoveredLayout === zone.layout }"
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
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'

interface SnapData {
  w: number
  h: number
  screens: number
}

const snapData = ref<SnapData>({ w: 800, h: 600, screens: 1 })
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

const baseGroups: GroupDef[] = [
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

const displayGroup: GroupDef = {
  id: 'displays',
  gridClass: 'grid-cols-2',
  zones: [{ layout: 'prev-display' }, { layout: 'next-display' }],
}

const groups = computed<GroupDef[]>(() =>
  snapData.value.screens > 1 ? [...baseGroups, displayGroup] : baseGroups,
)

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
  const data = (window as unknown as { __snapPanelData?: Partial<SnapData> }).__snapPanelData
  if (data) {
    snapData.value = {
      w: data.w ?? snapData.value.w,
      h: data.h ?? snapData.value.h,
      screens: data.screens ?? 1,
    }
  }
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
/*
 * 矩形默认 fill-8 / 悬浮 fill-18（前三组一致）。
 *
 * 全屏/居中组：
 * - 居中压小为 cell×0.78（给环与间隙留权重），同色 fill-8
 * - 镂空 = 居中 + 两侧 gap
 * - 外环 / 镂空内缘 = --snap-r；居中 r = 镂空 r − gap/2（略收，避免同 r 偏鼓、全减过方）
 * - 环仅靠 ::after box-shadow 上色，本体背景必须始终透明（否则 hover 会填满间隙）
 */
.snap-nested {
  --snap-gap: 0.125rem; /* = gap-0.5，与前三组小矩形间距一致 */
  --snap-cell: calc((100% - var(--snap-gap)) / 2);
  --snap-center: calc(var(--snap-cell) * 0.78);
  --snap-hollow: calc(var(--snap-center) + 2 * var(--snap-gap));
  --snap-r: 4px;
  /* 同绝对 r 在更小块上会显得更圆；居中略收半档 gap（全减过方） */
  --snap-r-center: max(0px, calc(var(--snap-r) - var(--snap-gap) / 2));
}
.snap-zone {
  /* 介于微圆与 radius-ctrl(6) 之间，微格略圆即可 */
  border-radius: var(--snap-r, 4px);
  background-color: var(--color-fill-8);
}
/*
 * 中心镂空环：伪元定位镂空区 + 扩散 box-shadow 填环；
 * 内缘与外环同 --snap-r（对齐大矩形），居中用 --snap-r-center
 */
.snap-zone.snap-ring {
  background-color: transparent;
  border-radius: var(--snap-r);
  overflow: hidden;
}
.snap-zone.snap-ring::after {
  content: '';
  position: absolute;
  width: var(--snap-hollow);
  height: var(--snap-hollow);
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  border-radius: var(--snap-r);
  box-shadow: 0 0 0 999px var(--color-fill-8);
  pointer-events: none;
}
.snap-zone.snap-inset {
  width: var(--snap-center);
  height: var(--snap-center);
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  border-radius: var(--snap-r-center);
  /* 默认色走 .snap-zone fill-8，与其它格一致 */
}
.snap-zone.snap-hover {
  background-color: var(--color-fill-18);
}
/* 环本体禁止铺底：间隙保持透空，仅 ::after 阴影变色 */
.snap-zone.snap-ring.snap-hover {
  background-color: transparent;
}
.snap-zone.snap-ring.snap-hover::after {
  box-shadow: 0 0 0 999px var(--color-fill-18);
}
/* 自定义格：底 fill-8/18 与他格相同；图标默认深一档 fill-12，悬浮再深到 muted */
.snap-zone.custom-zone {
  color: var(--color-fill-12);
}
.snap-zone.custom-zone.snap-hover {
  color: var(--color-text-muted);
}
</style>
