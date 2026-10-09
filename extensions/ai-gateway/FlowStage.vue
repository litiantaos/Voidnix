<template>
  <div ref="stageEl" class="relative overflow-hidden" h="39">
    <svg class="h-full w-full pointer-events-none inset-0 absolute">
      <path
        v-for="l in links"
        :key="l.key"
        :ref="(el) => setPathEl(l.key, el)"
        class="link"
        :class="{ on: l.on, idle: l.idle }"
        :d="l.d"
      />
    </svg>
    <div class="col col-prov">
      <div
        v-for="m in leftNodes"
        :key="m.key"
        :ref="(el) => setNodeEl(`m:${m.key}`, el)"
        class="node"
        :class="{ idle: m.idle }"
      >
        <span class="chip">
          <i class="i-ri-box-3-line" />
          <span class="m truncate">{{ m.model }}</span>
        </span>
      </div>
    </div>
    <div class="col col-gw">
      <div :ref="(el) => setNodeEl('gw', el)" class="node gw">
        <span class="chip">
          <span class="l1">
            <span class="gw-dot" :class="{ stopped: !running }" />
            <span class="m">{{ t('ai-gateway.nodeGateway') }}</span>
          </span>
          <span class="p">127.0.0.1:{{ port }}</span>
        </span>
      </div>
    </div>
    <div class="col col-cons">
      <div
        v-for="c in activeConsumers"
        :key="c.id"
        :ref="(el) => setNodeEl(`c:${c.id}`, el)"
        class="node"
      >
        <span class="chip">
          <i class="i-ri-computer-line" />
          <span class="m">{{ c.label }}</span>
          <span class="p">· {{ c.sub }}</span>
        </span>
      </div>
    </div>
    <!-- 静息态:右侧虚线锚点承载待命连线的视觉端点(左侧为路由表模型陈列) -->
    <span v-if="isEmpty" class="idle-anchor" style="left: calc(100% - 34px)" />
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  onMounted,
  ref,
  watch,
} from 'vue'
import { t } from '@/runtime/i18n'
import type { UsageEvent } from './sync'

/** 活跃窗口:窗口内有过流量的 (model · provider) / 消费者才生长为节点。
 * 须与 Rust `usage.rs::EVENT_CAP` 的覆盖余量互指(调整任一侧同步另一侧) */
const ACTIVE_MS = 90_000
/** 左列节点上限:舞台固定高,超出截断(活跃集按最近活跃序,截尾的是最久未用) */
const MAX_NODES = 5
/** 事件新鲜度:轮询周期 3s 内到达的才回放粒子;隐藏/切扩展期积压的陈旧事件
 * 只推水位不回放,防一次唤起回放出上百个粒子的风暴 */
const FRESH_MS = 15_000

const props = defineProps<{
  events: UsageEvent[]
  port: number
  /** 网关运行态:状态灯蓝(accent 呼吸)= 运行,红(danger 静止)= 停止 */
  running: boolean
  /** 路由表模型(空态左列待命陈列;活跃时让位给事件派生的活跃集) */
  models: { model: string; provider: string }[]
}>()

const stageEl = ref<HTMLElement>()
const links = ref<{ key: string; d: string; on: boolean; idle: boolean }[]>([])
const nowTick = ref(Date.now())

const nodeEls = new Map<string, HTMLElement>()
const pathEls = new Map<string, Element | null>()

function setNodeEl(key: string, el: unknown) {
  if (el) nodeEls.set(key, el as HTMLElement)
  else nodeEls.delete(key)
}
function setPathEl(key: string, el: unknown) {
  pathEls.set(key, (el as Element | null) ?? null)
}

/** 窗口内活跃的模型节点(model+provider 去重,最近活跃在前) */
const activeModels = computed(() => {
  const deadline = nowTick.value - ACTIVE_MS
  const map = new Map<string, { key: string; model: string; provider: string; last: number }>()
  for (const e of props.events) {
    if (e.ts < deadline) continue
    const key = `${e.model}|${e.provider}`
    const cur = map.get(key)
    if (!cur) map.set(key, { key, model: e.model, provider: e.provider, last: e.ts })
  }
  return [...map.values()].sort((a, b) => b.last - a.last)
})

/** 左列节点:活跃集优先;零活跃时陈列路由表模型(待命态,画静默虚线) */
const leftNodes = computed(() => {
  if (activeModels.value.length > 0) {
    return activeModels.value.slice(0, MAX_NODES).map((m) => ({
      key: m.key,
      model: m.model,
      provider: m.provider,
      idle: false,
    }))
  }
  return props.models.slice(0, MAX_NODES).map((m) => ({
    key: `${m.model}|${m.provider}`,
    model: m.model,
    provider: m.provider,
    idle: true,
  }))
})

/** 窗口内活跃的消费者:登记身份(事件 consumer)优先展示真名,未登记按协议面
 * 兜底(anthropic = Claude Code,其余 = 其它工具) */
const activeConsumers = computed(() => {
  const deadline = nowTick.value - ACTIVE_MS
  const map = new Map<string, { id: string; label: string; sub: string }>()
  for (const e of props.events) {
    if (e.ts < deadline) continue
    const face = e.protocol === 'anthropic' ? 'cc' : 'tools'
    const id = e.consumer ?? face
    if (map.has(id)) continue
    map.set(id, {
      id,
      label: e.consumer ?? (face === 'cc' ? 'Claude Code' : t('ai-gateway.consumerTools')),
      sub: { anthropic: 'Messages', responses: 'Responses', chat: 'Chat' }[e.protocol],
    })
  }
  return [...map.values()]
})

const isEmpty = computed(() => activeModels.value.length === 0)

// ── 连线:节点集/尺寸变化后重测锚点重画(纯 opacity 淡入不参与测量,锚点稳定) ──

function anchor(key: string, side: 'left' | 'right'): { x: number; y: number } | null {
  const el = nodeEls.get(key)
  const stage = stageEl.value
  if (!el || !stage) return null
  const r = el.getBoundingClientRect()
  const sr = stage.getBoundingClientRect()
  return {
    x: (side === 'right' ? r.right : r.left) - sr.left,
    y: r.top - sr.top + r.height / 2,
  }
}

function linkD(a: { x: number; y: number }, b: { x: number; y: number }, padA = 5, padB = 5) {
  const mx = (a.x + b.x) / 2
  return `M ${a.x + padA} ${a.y} C ${mx} ${a.y}, ${mx} ${b.y}, ${b.x - padB} ${b.y}`
}

function rebuildLinks() {
  // KeepAlive 隐藏态 DOM 脱离,锚点测量全零——跳过,激活时 onActivated 重画
  if (stageEl.value?.isConnected === false) return
  const out: { key: string; d: string; on: boolean; idle: boolean }[] = []
  const now = nowTick.value
  const gwL = anchor('gw', 'left')
  const gwR = anchor('gw', 'right')
  if (!gwL || !gwR) {
    links.value = out
    return
  }
  const deadline = now - ACTIVE_MS
  for (const m of leftNodes.value) {
    const a = anchor(`m:${m.key}`, 'right')
    if (!a) continue
    const on = m.idle
      ? false
      : props.events.some(
          (e) => e.model === m.model && e.provider === m.provider && e.ts >= deadline,
        )
    out.push({ key: `m:${m.key}`, d: linkD(a, gwL), on, idle: m.idle })
  }
  for (const c of activeConsumers.value) {
    const a = anchor(`c:${c.id}`, 'left')
    if (!a) continue
    const on = props.events.some((e) => {
      if (e.ts < deadline) return false
      const id = e.consumer ?? (e.protocol === 'anthropic' ? 'cc' : 'tools')
      return id === c.id
    })
    out.push({ key: `c:${c.id}`, d: linkD(gwR, a), on, idle: false })
  }
  if (activeModels.value.length === 0 && activeConsumers.value.length === 0) {
    // 空态:右侧锚点到网关的待命虚线(左侧为路由表模型陈列线)
    const stage = stageEl.value
    if (stage) {
      const sr = stage.getBoundingClientRect()
      const anchors = stage.querySelectorAll<HTMLElement>('.idle-anchor')
      if (anchors.length > 0) {
        const r = anchors[anchors.length - 1]
        const rr = r.getBoundingClientRect()
        out.push({
          key: 'idle-r',
          d: linkD(
            gwR,
            { x: rr.left - sr.left + rr.width / 2, y: rr.top - sr.top + rr.height / 2 },
            5,
            8,
          ),
          on: false,
          idle: true,
        })
      }
    }
  }
  links.value = out
}

let destroyed = false

/** 粒子:主光点 + 尾点沿 path 飞行(easeOut,尾段淡出) */
function fly(key: string) {
  const path = pathEls.get(key) as SVGPathElement | null
  if (!path || destroyed) return
  const svg = path.ownerSVGElement
  if (!svg) return
  const mk = (cls: string, r: string) => {
    const c = document.createElementNS('http://www.w3.org/2000/svg', 'circle')
    c.setAttribute('class', cls)
    c.setAttribute('r', r)
    svg.appendChild(c)
    return c
  }
  const dot = mk('spark', '2.5')
  const tail = mk('tail', '2')
  const len = path.getTotalLength()
  const dur = 380
  const t0 = performance.now()
  const step = (tm: number) => {
    if (destroyed) {
      dot.remove()
      tail.remove()
      return
    }
    const k = Math.min(1, (tm - t0) / dur)
    const e = 1 - Math.pow(1 - k, 3)
    const pt = path.getPointAtLength(e * len)
    const tt = path.getPointAtLength(Math.max(0, e - 0.08) * len)
    const fade = k > 0.85 ? String((1 - k) / 0.15) : '1'
    dot.setAttribute('cx', String(pt.x))
    dot.setAttribute('cy', String(pt.y))
    dot.setAttribute('opacity', fade)
    tail.setAttribute('cx', String(tt.x))
    tail.setAttribute('cy', String(tt.y))
    tail.setAttribute('opacity', String(0.3 * Number(fade)))
    if (k < 1) requestAnimationFrame(step)
    else {
      dot.remove()
      tail.remove()
    }
  }
  requestAnimationFrame(step)
}

/** 新事件驱动粒子:左段(model → 网关)到达后右段(网关 → 消费者)。
 * 某侧首次进入活跃集时其连线尚未生成(连线重建在 nextTick watch 链上),
 * rAF 重试一次——否则「首激活」事件的粒子被静默丢弃 */
function fire(ev: UsageEvent, retried = false) {
  const mKey = `m:${ev.model}|${ev.provider}`
  const cKey = `c:${ev.consumer ?? (ev.protocol === 'anthropic' ? 'cc' : 'tools')}`
  const mLink = links.value.find((l) => l.key === mKey)
  const cLink = links.value.find((l) => l.key === cKey)
  if ((!mLink || !cLink) && !retried) {
    requestAnimationFrame(() => fire(ev, true))
    return
  }
  if (mLink) {
    mLink.on = true
    fly(mKey)
  }
  window.setTimeout(() => {
    if (cLink) {
      cLink.on = true
      fly(cKey)
    }
  }, 240)
}

// 轮询到新事件(seq 判新)逐条驱动;首挂载只记水位不回放历史
let lastSeq = 0
let primed = false
watch(
  () => props.events,
  (evs) => {
    if (!primed) {
      primed = true
      lastSeq = evs[0]?.seq ?? 0
      return
    }
    const fresh = evs.filter((e) => e.seq > lastSeq)
    if (fresh.length > 0) {
      lastSeq = fresh[0].seq
      const now = Date.now()
      for (const ev of fresh) {
        if (now - ev.ts <= FRESH_MS) fire(ev)
      }
    }
    nowTick.value = Date.now()
  },
)

watch([leftNodes, activeConsumers, () => props.port], () => void nextTick(rebuildLinks))

let tickTimer: number | undefined
let resizeHandler: () => void

function startTick() {
  // 窗口推进兜底:无新事件时活跃集也要随 90s 窗口收敛
  if (tickTimer === undefined) {
    tickTimer = window.setInterval(() => {
      nowTick.value = Date.now()
    }, 15_000)
  }
}
function stopTick() {
  if (tickTimer !== undefined) {
    clearInterval(tickTimer)
    tickTimer = undefined
  }
}

onMounted(() => {
  startTick()
  resizeHandler = () => rebuildLinks()
  window.addEventListener('resize', resizeHandler)
  void nextTick(rebuildLinks)
})
// KeepAlive 缓存态(切扩展)不空转:停 tick 防后台重算 + 脱离 DOM 的锚点测量
onActivated(() => {
  startTick()
  void nextTick(rebuildLinks)
})
onDeactivated(stopTick)
onBeforeUnmount(() => {
  destroyed = true
  stopTick()
  window.removeEventListener('resize', resizeHandler)
})
</script>

<style scoped>
.col {
  position: absolute;
  top: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 8px;
}
.col-prov {
  left: 14px;
  align-items: flex-end;
}
.col-cons {
  right: 14px;
  align-items: flex-start;
}
.col-gw {
  left: 50%;
  transform: translateX(-50%);
  align-items: center;
}

.node {
  display: flex;
  flex-direction: column;
}
.col-prov .node {
  align-items: flex-end;
}
.col-cons .node {
  align-items: flex-start;
}

.chip {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border-radius: var(--radius-ctrl);
  border: 1px solid var(--color-border);
  background: var(--soft-chip-fill);
  box-shadow: 0 1px 2px rgb(var(--shadow-ink) / 0.12);
  font-size: 12px;
  line-height: 14px;
  white-space: nowrap;
  animation: node-in var(--duration-slow) var(--ease-out) backwards;
}
.chip .m {
  color: var(--color-text-primary);
  font-weight: 500;
  max-width: 160px;
}
.chip .p {
  color: var(--color-text-secondary);
  flex: none;
}
.chip i {
  font-size: 12px;
  color: var(--color-text-secondary);
  flex: none;
}
/* 待命陈列节点(空态路由表模型):整体降一档存在感 */
.node.idle {
  opacity: 0.72;
}

/* 网关:上下双行(上行呼吸点+名称,下行端口),accent 细边强化中枢锚点感 */
.gw .chip {
  flex-direction: column;
  gap: 1px;
  padding: 7px 16px 6px;
  align-items: center;
  font-weight: 500;
  border-color: color-mix(in srgb, var(--color-accent) 45%, var(--color-border));
  background: linear-gradient(var(--accent-wash), var(--soft-chip-fill));
}
.gw .l1 {
  display: flex;
  align-items: center;
  gap: 6px;
}
.gw .p {
  font-size: 11px;
  line-height: 13px;
}
.gw-dot {
  width: 7px;
  height: 7px;
  border-radius: 9999px;
  background: var(--color-accent);
  animation: breathe var(--duration-normal) var(--ease-out) infinite;
}
/* 停止态:红灯静止(无心跳) */
.gw-dot.stopped {
  background: var(--color-danger);
  animation: none;
}

/* 节点生长 = 纯 opacity 淡入(transform 不参与,连线锚点测量零干扰) */
@keyframes node-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
@keyframes breathe {
  0%,
  100% {
    opacity: 0.35;
  }
  50% {
    opacity: 1;
  }
}

svg :deep(path.link) {
  fill: none;
  stroke: var(--color-border);
  stroke-width: 1.5;
}
svg :deep(path.link.on) {
  stroke: color-mix(in srgb, var(--color-accent) 38%, transparent);
  stroke-dasharray: 3 5;
  animation: dash-flow 1.1s linear infinite;
}
svg :deep(path.link.idle) {
  stroke-dasharray: 2 4;
  opacity: 0.7;
}
svg :deep(circle.spark) {
  fill: var(--color-accent);
}
svg :deep(circle.tail) {
  fill: var(--color-accent);
  opacity: 0.3;
}
@keyframes dash-flow {
  to {
    stroke-dashoffset: -8;
  }
}

/* 静息态锚点与提示 */
.idle-anchor {
  position: absolute;
  top: 50%;
  width: 14px;
  height: 14px;
  transform: translate(-50%, -50%);
  border-radius: 9999px;
  border: 1.5px dashed var(--color-border);
}
</style>
