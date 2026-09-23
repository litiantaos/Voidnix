<template>
  <div p="x-3" pb="3" role="listbox" :aria-label="t('search.resultsLabel')">
    <div ref="listBodyRef" class="list-body relative" flex="~ col" gap="1.5">
      <!-- 选中滑层（色块）：脱流承载聚焦行色块，绘制序在行文本之下（见 placeIndicator） -->
      <div ref="indicatorRef" class="selection-indicator" aria-hidden="true" />
      <template v-for="(item, i) in items" :key="itemKey(item, i)">
        <div
          v-if="
            groupField &&
            (i === 0 || getGroupValue(item) !== getGroupValue(items[i - 1])) &&
            (groupTitle ? groupTitle(getGroupValue(item)) : getGroupValue(item))
          "
          class="group-header"
        >
          <slot name="group-title" :group="getGroupValue(item)" :item="item" :index="i">
            {{ groupTitle ? groupTitle(getGroupValue(item)) : getGroupValue(item) }}
          </slot>
        </div>

        <div
          :ref="(el: unknown) => setItemRef(el, i)"
          role="option"
          :aria-selected="isItemSelected(i)"
          class="radius-panel relative"
          :class="{ 'ui-active': isItemSelected(i), 'list-focus-row': localIndex === i }"
          @click="onItemClick(i, $event)"
          @dblclick="onItemDblClick(i)"
          @contextmenu="onItemContextMenu(i, $event)"
        >
          <slot name="item" :item="item" :index="i" />
        </div>
      </template>
      <!-- 选中滑层（徽标）：与色块层同参数移动，DOM 尾部使其盖行尾内容之上 -->
      <div v-if="actionHint" ref="hintLayerRef" class="selection-hint" aria-hidden="true">
        <ActionMenuHint :visible="focusHasHint" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts" generic="T">
import {
  ref,
  computed,
  watch,
  nextTick,
  onMounted,
  onActivated,
  onBeforeUnmount,
  getCurrentInstance,
} from 'vue'
import { onKeyStroke } from '@/composables/events'
import { t } from '@/runtime/i18n'
import { useAppStore } from '@/stores/app'
import ActionMenuHint from '@/components/ui/ActionMenuHint.vue'
import {
  isComposing as isComposingCheck,
  isFormControl,
  isModalDialogOpen,
  wrapIndex,
} from '@/utils/dom'

// KeepAlive 软禁用：deactivate 后监听仍在，按键时实时判定是否处于停用子树（见 inDeactivatedTree）。
// inKeepAliveTree：首次 activated 即标记（KeepAlive 树内组件挂载即触发 activated；
// 标准列表在 ContentView 的 KeepAlive 外，永不触发）——用于区分自管/受控列表。
const inKeepAliveTree = ref(false)
onActivated(() => {
  inKeepAliveTree.value = true
})

const instance = getCurrentInstance()

/// 已停用 KeepAlive 子树内的列表不响应键盘。不能用 onDeactivated 置位的 isActive 镜像：
/// 停用视图的响应式 watcher 仍活跃，其内部 v-if/v-else-if 分支翻转会使列表在停用树内
/// 卸载后重挂载（如 homebrew status 就绪前后空态与列表互切），重挂载不触发
/// activated/deactivated 钩子对，镜像停在初值 true 会成为后台仍消费 ↑↓/Enter 的
/// 「僵尸列表」（用户在其它扩展回车误触的根因）。按键时沿父链查 isDeactivated——
/// 状态由框架在 KeepAlive 激活/停用转换时维护，无镜像配对假设（与 Vue
/// registerKeepAliveHook 的钩子守卫同源语义）
function inDeactivatedTree(): boolean {
  for (let anc = instance; anc; anc = anc.parent) {
    if (anc.isDeactivated) return true
  }
  return false
}

// 整窗视图让位在列表层强制执行（不依赖调用方 keyboardActive 巧合为 false）：
// 同 isModalDialogOpen 一样属全局模态让位，集中防御优于散布到全部调用点。
// 契约：挂载需 active Pinia（读 appStore；子窗口入口不经 BaseList）
const appStore = useAppStore()

const props = withDefaults(
  defineProps<{
    items: T[]
    selectedIndex?: number
    groupField?: keyof T | ((item: T) => string)
    groupTitle?: (group: string) => string
    multiSelect?: boolean
    selectedIds?: Set<string>
    idField?: string
    /** 是否激活键盘导航（由父组件根据扩展状态控制） */
    keyboardActive?: boolean
    /** IME 输入法合成状态（由父组件传入） */
    composing?: boolean
    /** ArrowUp/Down 在自定义输入框聚焦时是否仍导航（如翻译框）。
     *  全局搜索框由 data-list-execute 属性统一放行，无需此 prop。Enter 一律让出
     *  除非控件标记 data-list-execute。默认 false 保护设置页 input 编辑。 */
    navigateOnInput?: boolean
    /** 行内右键动作菜单（Cmd+Enter 同入口）快捷键提示徽标：true = 全部行显示；
     *  谓词 = 按焦点行 item 条件显示（如全局结果仅 application/file/folder 且有
     *  path）。徽标由选中滑层承载、随焦点行平滑移动，仅谓词翻转时淡入淡出。 */
    actionHint?: boolean | ((item: T) => boolean)
  }>(),
  {
    selectedIndex: 0,
    multiSelect: false,
    idField: 'id',
    keyboardActive: true,
    composing: false,
    navigateOnInput: false,
    actionHint: false,
  },
)

const emit = defineEmits<{
  'update:selectedIndex': [index: number]
  'update:selectedIds': [ids: Set<string>]
  select: [index: number]
  execute: [item: T, index: number, event?: KeyboardEvent]
  contextmenu: [item: T, index: number, event: MouseEvent]
}>()

const localIndex = ref(props.selectedIndex)
watch(
  () => props.selectedIndex,
  (val) => {
    localIndex.value = val
  },
)

function setSelectedIndex(index: number) {
  localIndex.value = index
  emit('update:selectedIndex', index)
  emit('select', index)
}

// 跨会话转移（进入/退出/切换扩展）统一归首项；同扩展内 subview 往返（activeExtId
// 不变）与窗口唤起（focus 不动 activeExtId）保留导航位置。仅作用于 KeepAlive 树内的
// 自管列表（emit 同步父级镜像，deactivate 不销毁 watch——离开扩展时归零，回来首帧
// 即首项）；标准列表受控于 MainView 不参与：其 selectedIndex 转移链（exitExtension
// 的 savedToolIndex 恢复 / 进入扩展 watch 的捕获）自洽，子组件归零回写会覆盖同步
// 恢复值，破坏受控单向数据流。
watch(
  () => appStore.activeExtId,
  () => {
    if (inKeepAliveTree.value) setSelectedIndex(0)
  },
)

// ── Refs ──
// 函数 ref 在节点卸载时会收到 null；必须同步释放旧 DOM 引用，否则搜索结果
// 缩短/替换后，itemRefs 会把整棵已脱离文档的节点树继续挂在 JS 堆上。
const itemRefs = ref<Array<HTMLElement | null>>([])
function setItemRef(el: unknown, index: number) {
  if (el) {
    itemRefs.value[index] = (el as { $el: HTMLElement }).$el || (el as HTMLElement)
    return
  }

  itemRefs.value[index] = null
  // 结果列表缩短时裁掉尾部空槽，避免数组本身随历史最大列表长度保留。
  while (itemRefs.value.length > 0 && itemRefs.value[itemRefs.value.length - 1] === null) {
    itemRefs.value.pop()
  }
}
onBeforeUnmount(() => {
  // 原地清空：比赋新数组（itemRefs.value = []）更利于 GC——
  // 旧数组引用链立即断开，而非等 ref 替换后旧数组被间接持有期间 DOM 节点仍可达
  itemRefs.value.length = 0
})
defineExpose({ selectedIndex: localIndex, setSelectedIndex, reset, reveal })

// ── 选中高亮滑层 ──
// 聚焦行色块与快捷键徽标由两层脱流 overlay 承载：色块层绘制序在行文本之下、徽标层
// 置于 DOM 尾部盖行尾内容之上（滑块含 transform 形成 stacking context，徽标无法在
// 单元素内同时满足两种层级，故拆两层同参数驱动）；选中切换经 transform 过渡滑动
// （仅 GPU 合成属性），行自身 ui-active 保留（文本色 / aria），背景由 .list-focus-row
// 规则让位滑块；多选（selectedIds）行不受影响，保留自身静态色块
const listBodyRef = ref<HTMLElement | null>(null)
const indicatorRef = ref<HTMLElement | null>(null)
const hintLayerRef = ref<HTMLElement | null>(null)
let bodyObserver: ResizeObserver | null = null

/// 徽标显隐（actionHint 谓词按焦点行求值）：运动已由滑层承载（随色块平滑移动），
/// 显隐只表达「焦点行有无 Cmd+Enter 动作」的数据语义；未传 actionHint 时徽标层零渲染
const focusHasHint = computed(() => {
  const item = props.items[localIndex.value]
  if (!item || !props.actionHint) return false
  return typeof props.actionHint === 'function' ? props.actionHint(item) : true
})

/// 滑层落位（色块 + 徽标两层同参数）。animated=false 瞬时（挂载 / items 替换 /
/// 尺寸变化——不从过期位置滑来）；true 走 CSS transform/height 过渡（高度与位移同
/// 曲线渐变，滑层脱流无 reflow 传播），follow=true 时滚动先行、滑层以同曲线
/// （--duration-normal + --ease-inout）滞后跟随（滞后量 --selection-follow-lag，
/// theme.css 单一定义）——位移与高度进度恒 ≤ 滚动揭示量，构造上不会滑入未揭示的裁剪区
function placeIndicator(animated: boolean, follow = false) {
  const layers = [indicatorRef.value, hintLayerRef.value].filter((l): l is HTMLElement => l != null)
  const el = itemRefs.value[localIndex.value]
  for (const layer of layers) layer.classList.toggle('follow', follow)
  if (!el) {
    for (const layer of layers) {
      layer.style.display = 'none'
      layer.style.transition = ''
    }
    return
  }
  const height = `${el.offsetHeight}px`
  const transform = `translateY(${el.offsetTop}px)`
  if (!animated) {
    for (const layer of layers) {
      layer.style.transition = 'none'
      layer.style.display = ''
      layer.style.height = height
      layer.style.transform = transform
    }
    // 强制同步 layout 锁定两层落位帧后恢复过渡，后续移动从新位置起滑
    for (const layer of layers) void layer.offsetHeight
    for (const layer of layers) layer.style.transition = ''
    return
  }
  for (const layer of layers) {
    layer.style.display = ''
    layer.style.height = height
    layer.style.transform = transform
  }
}

onMounted(() => {
  placeIndicator(false)
  // 行高随内容 / 容器宽度变化（副标题出现、换行）重落位；happy-dom 无 RO 跳过
  if (typeof ResizeObserver !== 'undefined' && listBodyRef.value) {
    bodyObserver = new ResizeObserver(() => placeIndicator(false))
    bodyObserver.observe(listBodyRef.value)
  }
})

// KeepAlive 重挂 DOM 后按当前几何重落位（停用期间后台刷新可能改写列表布局）
onActivated(() => placeIndicator(false))

onBeforeUnmount(() => {
  bodyObserver?.disconnect()
  bodyObserver = null
})

// ── Multi-select ──
let anchorIndex = -1

function getId(item: T): string {
  return (item as Record<string, unknown>)[props.idField] as string
}

/// 列表 key：优先 idField，缺省回退 index（避免重排时 DOM 错位复用）
function itemKey(item: T, index: number): string | number {
  const id = (item as Record<string, unknown>)?.[props.idField]
  if (id != null && id !== '') return String(id)
  return index
}

function isMultiSelected(index: number): boolean {
  if (!props.multiSelect || !props.selectedIds) return false
  return props.selectedIds.has(getId(props.items[index]))
}

/// wrapper 是否高亮：单选焦点项 或 多选项
function isItemSelected(index: number): boolean {
  return localIndex.value === index || isMultiSelected(index)
}

function emitIds(ids: Set<string>) {
  emit('update:selectedIds', ids)
}

/// shift 范围选择：anchor → index 区间全选（列表缩短后 anchor 可能越界，跳过缺席项）
function selectRangeTo(index: number) {
  if (anchorIndex < 0) anchorIndex = localIndex.value
  const [start, end] = [Math.min(anchorIndex, index), Math.max(anchorIndex, index)]
  const ids = new Set<string>()
  for (let i = start; i <= end; i++) {
    const it = props.items[i]
    if (it) ids.add(getId(it))
  }
  setSelectedIndex(index)
  emitIds(ids)
}

function onItemClick(index: number, e: MouseEvent) {
  if (props.multiSelect && (e.metaKey || e.ctrlKey || e.shiftKey)) {
    if (e.shiftKey) {
      selectRangeTo(index)
      return
    }
    const ids = new Set(props.selectedIds ?? [])
    if (ids.size === 0) {
      const focused = props.items[localIndex.value]
      if (focused) ids.add(getId(focused))
      if (anchorIndex < 0) anchorIndex = localIndex.value
    }
    const id = getId(props.items[index])
    if (ids.has(id)) ids.delete(id)
    else ids.add(id)
    setSelectedIndex(index)
    emitIds(ids)
  } else {
    anchorIndex = index
    setSelectedIndex(index)
  }
}

function onItemDblClick(index: number) {
  emit('execute', props.items[index], index)
  if (props.multiSelect) emitIds(new Set())
}

/// 右键：选中该项并冒泡（消费者决定是否弹出菜单），抑制原生右键菜单
/// emit 延迟到 nextTick：setSelectedIndex 经 props 传播（flush）后再触发消费者
/// —— 全局 ResultActionPanel 的 canOpen 读 props.selectedIndex，同步 emit时 prop 尚未刷新
function onItemContextMenu(index: number, e: MouseEvent) {
  e.preventDefault()
  anchorIndex = index
  setSelectedIndex(index)
  nextTick(() => emit('contextmenu', props.items[index], index, e))
}

// ── Keyboard 守卫 ──

/// 公共守卫：处于停用 KeepAlive 子树 / 未激活 / 整窗视图接管 / IME 合成中 / 模态弹窗
/// 打开（焦点在 BUTTON 上也不会再抢 ↑↓）不响应
function canNavigate(e: KeyboardEvent): boolean {
  if (inDeactivatedTree() || !props.keyboardActive) return false
  if (appStore.fullscreenView) return false
  if (props.composing || isComposingCheck(e)) return false
  if (isModalDialogOpen()) return false
  return true
}

/// ArrowUp/Down 让出判断：搜索框（data-list-execute）始终放行；其余表单控件按 navigateOnInput
function shouldYieldNavigation(): boolean {
  const active = document.activeElement as Element | null
  if (!isFormControl(active, { settingsControl: true })) return false
  if (active?.hasAttribute('data-list-execute')) return false
  return !props.navigateOnInput || props.items.length === 0
}

/// Enter 让出判断：表单控件聚焦时一律让出，除非控件显式委托（data-list-execute）
function shouldYieldExecution(): boolean {
  const active = document.activeElement as Element | null
  if (!isFormControl(active, { settingsControl: true })) return false
  return !active?.hasAttribute('data-list-execute')
}

// ── Keyboard ──
onKeyStroke(['ArrowDown', 'ArrowUp'], (e) => {
  if (!canNavigate(e)) return
  if (shouldYieldNavigation()) return

  const direction = e.key === 'ArrowDown' ? 'down' : 'up'

  // shift 范围多选
  if (props.multiSelect && e.shiftKey) {
    e.preventDefault()
    const next =
      direction === 'down'
        ? Math.min(localIndex.value + 1, props.items.length - 1)
        : Math.max(localIndex.value - 1, 0)
    if (next !== localIndex.value) selectRangeTo(next)
    return
  }

  e.preventDefault()
  if (props.items.length > 0) {
    const next = wrapIndex(localIndex.value, props.items.length, direction)
    setSelectedIndex(next)
    anchorIndex = next
    if ((props.selectedIds?.size ?? 0) > 0) emitIds(new Set())
  }
})

if (props.multiSelect) {
  onKeyStroke('a', (e) => {
    if (!canNavigate(e)) return
    if (!(e.metaKey || e.ctrlKey)) return
    e.preventDefault()
    emitIds(new Set(props.items.map((item) => getId(item))))
  })

  // 有多选项时 ESC 先清选择（不退出扩展）：捕获相监听先于 useResultNavigation 的
  // bubble 监听（与注册顺序无关）——扩展视图列表晚于 MainView 挂载，注册顺序不保证
  // 「子先于父」，捕获相是唯一确定性先行通道；stopImmediatePropagation 断掉后续
  // document 监听（含退出扩展的分派）
  onKeyStroke(
    'Escape',
    (e) => {
      if (!canNavigate(e)) return
      if ((props.selectedIds?.size ?? 0) === 0) return
      e.preventDefault()
      e.stopImmediatePropagation()
      emitIds(new Set())
    },
    { capture: true },
  )
}

onKeyStroke('Enter', (e) => {
  if (!canNavigate(e)) return
  if (shouldYieldExecution()) return
  // 按钮聚焦时 Enter 由按钮自身 click 处理
  if (document.activeElement?.tagName === 'BUTTON') return
  // 按住回车的 auto-repeat 不执行行项：跨扩展跳转的回车若被按住，repeat 会落到
  // 目标视图首行执行其动作（曾致 video 首行「选择文件」直接弹系统面板）
  if (e.repeat) return
  // H7 同族守卫：过滤/删除使列表缩短后 localIndex 可能越界（无高亮、消费者收到
  // undefined 会读属性崩溃）——无有效目标时不消费按键，方向键 wrapIndex 自愈
  const item = props.items[localIndex.value]
  if (!item) return
  e.preventDefault()
  // 执行即消费：一次回车至多执行一个列表的项（与 useResultNavigation 全局模式对齐），
  // 防 KeepAlive 并存监听重复响应同一按键
  e.stopImmediatePropagation()
  emit('execute', item, localIndex.value, e)
  if (props.multiSelect) emitIds(new Set())
})

// ── Scroll ──
function findScrollContainer(el: HTMLElement): HTMLElement | null {
  return el.closest('.overflow-y-auto, .overflow-auto') as HTMLElement | null
}

/// 读 scroll-padding-*（ContentView：top = chrome；bottom = CONTENT_INSET；无则 0）
function scrollPadding(container: HTMLElement, edge: 'Top' | 'Bottom'): number {
  const n = parseFloat(getComputedStyle(container)[`scrollPadding${edge}`])
  return Number.isFinite(n) ? n : 0
}

/// 抑制 watch 的视口跟随滚动，reveal 定位时由居中滚动接管
let suppressScroll = false

/// 三次贝塞尔缓动求值（与 CSS cubic-bezier 同式）：对时间比例 x 解 X(t)=x，返回 Y(t)。
/// Newton-Raphson 为主、二分兜底（导数过小或未收敛）
function cubicBezier(x1: number, y1: number, x2: number, y2: number): (x: number) => number {
  const cx = 3 * x1
  const bx = 3 * (x2 - x1) - cx
  const ax = 1 - cx - bx
  const cy = 3 * y1
  const by = 3 * (y2 - y1) - cy
  const ay = 1 - cy - by
  const sampleX = (t: number) => ((ax * t + bx) * t + cx) * t
  const sampleY = (t: number) => ((ay * t + by) * t + cy) * t
  const sampleDx = (t: number) => (3 * ax * t + 2 * bx) * t + cx
  return (x: number) => {
    let t = x
    for (let i = 0; i < 8; i++) {
      const err = sampleX(t) - x
      if (Math.abs(err) < 1e-6) return sampleY(t)
      const d = sampleDx(t)
      if (Math.abs(d) < 1e-7) break
      t -= err / d
    }
    let lo = 0
    let hi = 1
    t = x
    while (hi - lo > 1e-7) {
      const cur = sampleX(t)
      if (Math.abs(cur - x) < 1e-7) break
      if (cur < x) lo = t
      else hi = t
      t = (lo + hi) / 2
    }
    return sampleY(t)
  }
}

const EASE_INOUT = cubicBezier(0.45, 0, 0.2, 1)

/// 读 :root 自定义属性（时长/缓动单一真相在 theme.css token，与 .selection-indicator.follow
/// 同源；测试环境未加载 theme.css 时得空串，经解析回退同值常量）
function rootToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name)
}

/// 时长 token 解析（"150ms" / "0.2s"），失败回退 fallback
function parseDuration(value: string, fallback: number): number {
  const m = /([\d.]+)(ms|s)/.exec(value)
  return m ? parseFloat(m[1]!) * (m[2] === 's' ? 1000 : 1) : fallback
}

/// 滚动动画参数（时长/缓动）：滑块跟随曲线与滚动曲线恒一致，进度约束（滑块 ≤ 滚动）
/// 不因 token 调参失配
function scrollMotion(): { duration: number; ease: (x: number) => number } {
  const bm = /cubic-bezier\(([^)]+)\)/.exec(rootToken('--ease-inout'))
  const params = bm?.[1]?.split(',').map(Number)
  const ease =
    params && params.length === 4 && params.every(Number.isFinite)
      ? cubicBezier(params[0]!, params[1]!, params[2]!, params[3]!)
      : EASE_INOUT
  return { duration: parseDuration(rootToken('--duration-normal'), 200), ease }
}

/// 在飞滚动动画取消句柄（单列表单容器，实例级即可）
let scrollAnim: (() => void) | null = null

/// 容器滚动到 target：animated 时按滚动曲线（--duration-normal + --ease-inout，软起步）
/// rAF 逐帧推进。每帧回读 scrollTop，非本动画写入（用户滚轮/拖拽/滚动锚定）即中断让权
function animateScroll(container: HTMLElement, target: number, animated: boolean) {
  scrollAnim?.()
  const start = container.scrollTop
  const delta = target - start
  if (!animated || Math.abs(delta) < 1) {
    container.scrollTop = target
    return
  }
  const { duration, ease } = scrollMotion()
  if (duration <= 0) {
    container.scrollTop = target
    return
  }
  let raf = 0
  let last = start
  let t0 = -1
  const cancel = () => {
    if (raf) cancelAnimationFrame(raf)
    raf = 0
    scrollAnim = null
  }
  scrollAnim = cancel
  const step = (now: number) => {
    if (container.scrollTop !== last) {
      cancel()
      return
    }
    if (t0 < 0) t0 = now
    const p = Math.min((now - t0) / duration, 1)
    container.scrollTop = start + delta * ease(p)
    last = container.scrollTop
    if (p < 1) raf = requestAnimationFrame(step)
    else cancel()
  }
  raf = requestAnimationFrame(step)
}

onBeforeUnmount(() => scrollAnim?.())

/// 视口跟随目标：行不在视野内时返回容器与目标 scrollTop（分组首项连同标题一并滚入），
/// 在视野内返回 null。既有触发面：仅选中移动时调用
function viewportFollowTarget(el: HTMLElement): { container: HTMLElement; target: number } | null {
  const container = findScrollContainer(el)
  if (!container) return null

  // 分组首项连同标题滚入：仅当前邻确为分组标题（.group-header）才作滚动锚——
  // 不能盲取 previousElementSibling：list-body 首子元素是选中滑块，index 0 的前邻
  // 是滑块而非标题
  const prev = el.previousElementSibling
  const topElement = prev?.classList.contains('group-header') ? prev : el

  const topInset = scrollPadding(container, 'Top')
  // bottom 无声明时回退 12（与列表 pb-3 / 全局 p-3 一致），避免贴底时下边距小于两侧
  const bottomInset = scrollPadding(container, 'Bottom') || 12
  const elRectTop = topElement.getBoundingClientRect().top
  const elRectBottom = el.getBoundingClientRect().bottom
  const containerRect = container.getBoundingClientRect()
  // 顶部可见区 = 容器顶 + chrome；底部 = 容器底 − scroll-padding-bottom
  const visibleTop = containerRect.top + topInset
  const visibleBottom = containerRect.bottom - bottomInset

  if (elRectBottom > visibleBottom) {
    return { container, target: container.scrollTop + (elRectBottom - visibleBottom) }
  }
  if (elRectTop < visibleTop) {
    return { container, target: container.scrollTop - (visibleTop - elRectTop) }
  }
  return null
}

/// 减动效偏好：导航滑层与视口跟随滚动退化为瞬时（复用连击 snap 同路径），逐次读取
function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/// 连击步进窗口 = 本步动画包络 × 0.8（同源读 token 推导，调参随动）：视口内滑动 =
/// --duration-fast；跨界跟随 = --selection-follow-lag + --duration-normal。重定目标
/// 发生在动画完成 80% 之后时，滑块残余滞后不超过一步的 20%（读感为连续滑行而非
/// 追帧漂浮），保留动画；快于此的输入（按住 repeat / 极速连点）瞬时步进
function rapidSnapWindow(follow: boolean): number {
  const envelope = follow
    ? parseDuration(rootToken('--selection-follow-lag'), 60) +
      parseDuration(rootToken('--duration-normal'), 200)
    : parseDuration(rootToken('--duration-fast'), 150)
  return envelope * 0.8
}
let lastNavAt = -Infinity
/// reset() 进行中标志：联动 watch 本轮跳过动画判定（长距离移向 0 会落入动画
/// 路径，与 reset 的瞬时语义冲突），归位由 reset 续段显式执行
let resetting = false

// 选中移动主编排（滑块 + 视口跟随，双源单 watcher）：滚动先行、滑块滞后同曲线跟随
// （follow 模式）；行在视野内时滑块直接常规滑动（--duration-fast ease-out）。items
// 引用替换（含同 flush「替换 + 归零」）与 wrap 首↔末项两者一致瞬时跳变；仅 items
// 变化只重落滑块不滚动。滚动将启动是 follow 的唯一判定，保证滑块进度恒 ≤ 滚动
// 揭示量（不入裁剪区）。prefers-reduced-motion 下退化为瞬时（滑块直落位、直写 scrollTop）
watch([() => props.items, localIndex], async ([items, index], [prevItems, prevIndex]) => {
  const now = performance.now()
  const moved = index !== prevIndex
  const interval = moved ? now - lastNavAt : Infinity
  if (moved) lastNavAt = now
  const itemsChanged = items !== prevItems
  // wrap 首↔末项（末→0 / 0→末，显式判定非距离阈值）：穿行整列表无导航意义，
  // 所有列表一致瞬时跳变（滑块/滚动/徽标同语义）；2 项列表除外——0↔1 即相邻
  // 步进，判定退化会让短列表的选择移动永不滑动
  const wrapped =
    prevItems.length > 2 &&
    ((prevIndex === prevItems.length - 1 && index === 0) ||
      (prevIndex === 0 && index === prevItems.length - 1))
  // 同步段（pre-patch）：同类导航行已存在，滚动计划可按当前几何先行计算（类翻转
  // 不改变行几何），供连击分档选取窗口
  let plan: { container: HTMLElement; target: number } | null = null
  if (moved && !itemsChanged && !wrapped && !suppressScroll) {
    const el = itemRefs.value[index]
    if (el) plan = viewportFollowTarget(el)
  }
  const rapid = interval < rapidSnapWindow(plan != null)
  const animated = !prefersReducedMotion() && !itemsChanged && !wrapped && !rapid && !resetting
  await nextTick()
  if (!moved) {
    placeIndicator(animated)
    return
  }
  // items 替换 / wrap / 连击步进（跳变路径，新行 post-patch 才存在时补算滚动计划）
  if (!animated) {
    const el = itemRefs.value[index]
    plan = el != null && !suppressScroll ? viewportFollowTarget(el) : null
  }
  placeIndicator(animated, animated && plan != null)
  if (plan) animateScroll(plan.container, plan.target, animated)
})

/// 归零并瞬时落位（滑层瞬落 + 视口滚顶）：会话复位 / 列表重过滤等 View 数据语义
/// 的显式入口（动态置顶列表新记录不断插入顶部，保留索引指向已漂移记录——消费者
/// clipboard）。与 setSelectedIndex(0) 的差别：不经联动 watch 的动画判定（长距离
/// 移向 0 落入动画路径），同步归零 + 下一帧瞬时落位滚顶；与 items 是否替换无关
/// （fetch 缓存命中引用不变仍瞬时）。滚动方向恒为滚顶（列表头部 = 最新记录起点）
async function reset() {
  resetting = true
  suppressScroll = true
  setSelectedIndex(0)
  await nextTick()
  resetting = false
  suppressScroll = false
  placeIndicator(false)
  const el = itemRefs.value[0]
  if (el) {
    const plan = viewportFollowTarget(el)
    if (plan) animateScroll(plan.container, plan.target, false)
  }
}

/// 定位到指定项：高亮选中（同步导航索引）+ 居中滚动
function reveal(index: number) {
  suppressScroll = true
  setSelectedIndex(index)
  // watch 的视口跟随在本轮微任务被 suppressScroll 拦截；下一宏任务复位并居中滚动
  setTimeout(() => {
    suppressScroll = false
    void scrollIntoCenter(index)
  })
}

async function scrollIntoCenter(index: number) {
  await nextTick()
  const el = itemRefs.value[index]
  if (!el) return
  const container = findScrollContainer(el)
  if (!container) return
  const topInset = scrollPadding(container, 'Top')
  const bottomInset = scrollPadding(container, 'Bottom') || 12
  const elRect = el.getBoundingClientRect()
  const containerRect = container.getBoundingClientRect()
  // 在扣除 chrome / 底边 inset 后的可视区内垂直居中
  const visibleHeight = containerRect.height - topInset - bottomInset
  const offset = elRect.top - (containerRect.top + topInset) + elRect.height / 2 - visibleHeight / 2
  // 居中滚动与视口跟随同曲线（reveal 高亮常规滑动；减动效偏好退化为瞬时）
  animateScroll(container, container.scrollTop + offset, !prefersReducedMotion())
}

function getGroupValue(item: T): string {
  if (!props.groupField) return ''
  return String(
    typeof props.groupField === 'function' ? props.groupField(item) : item[props.groupField],
  )
}
</script>

<style scoped>
/* 列表容器 containment：限制子项 invalidation 传播范围，减少搜索结果替换时的
   WebKit 重排/重绘面积。LIMITS 已收紧（单组≤12、file≤20，全文≈44 节点），
   全量渲染开销可控，不使用 content-visibility（快速滚动有加载延迟 + 滚动条跳动）。 */
[role='listbox'] {
  contain: layout style;
}

/* 选中滑层：色块层（.selection-indicator）置于行前、绘制序在行文本之下，徽标层
 * （.selection-hint）置于 DOM 尾部、盖行尾内容之上——两层脱流、同参数驱动，仅过渡
 * transform / height（GPU 合成 + 脱流高度，无 IOSurface 常驻；height 属布局属性但
 * 滑层脱流且容器 contain:layout，reflow 不传播到行——同 BaseDialog 内容高 FLIP 的
 * height 过渡先例） */
.selection-indicator,
.selection-hint {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  pointer-events: none;
  transition:
    transform var(--duration-fast) var(--ease-out),
    height var(--duration-fast) var(--ease-out);
}

.selection-indicator {
  border-radius: var(--radius-panel);
  background: var(--ui-active-fill);
}

/* 视口跟随时滑层滞后起步：滚动先行（JS 侧同曲线 --duration-normal + --ease-inout），
 * 同曲线 + 延迟保证滑层进度（位移与高度）恒 ≤ 滚动揭示量，不滑入未揭示的裁剪区。
 * 滞后量 --selection-follow-lag（theme.css）：CSS 过渡延迟与 JS 连击窗口同源消费，
 * 单一定义 */
.selection-indicator.follow,
.selection-hint.follow {
  transition:
    transform var(--duration-normal) var(--ease-inout) var(--selection-follow-lag),
    height var(--duration-normal) var(--ease-inout) var(--selection-follow-lag);
}

/* 聚焦行背景交由滑块承载：特异性（0,4,0）压过 theme .ui-active 色块的 !important；
 * 多选（selectedIds）行不挂此类，保留自身静态色块 */
.list-body > [role='option'].list-focus-row {
  background: transparent !important;
}
</style>
