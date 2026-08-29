<template>
  <!-- 顶距交给 CHROME_HEIGHT,与列表 px-3 pb-3 同构,勿 p-t 叠双层 -->
  <div p="x-3 b-3" relative flex="~ col" class="notes-root">
    <!-- 输入区壳:ui-field(soft-surface 底 + border 描边,聚焦改边框色)+ panel 圆角,
         与 BaseTextarea 主输入(panel 档)同款面;固定窗高,壳高恒定 -->
    <div class="notes-box ui-field radius-panel" p="3" flex="~ 1 col" min-h="0">
      <!-- 渲染层:逐字符 span + 自绘光标。白名单 pre-wrap(与 textarea 同语义),
           user-select 关闭(选区自绘),点击/键盘定位经隐藏 textarea 承接。
           固定窗高下渲染层自身即滚动容器(局部固定高度区域,长文内部滚动):
           ghost/caret 均 absolute 锚定于滚动内容系,随滚动同步移动;
           overflow-x 锁 hidden 截断行尾 ghost 飘散的瞬时越界,免横向滚动泄漏 -->
      <div
        ref="layerEl"
        class="notes-layer hide-scrollbar overflow-x-hidden overflow-y-auto"
        relative
        flex="1"
        min-h="30"
        @mousedown="onLayerDown"
        @animationend="onAnimEnd"
      >
        <!-- 双容器切分(splitContentIdx):头部 = 编辑行区(编辑点至其行尾 \n,inline
             流,非均匀重排走逐字符 FLIP);尾块 = 下方完整逻辑行(block——起始必为
             行首,与上方内容布局无关,编辑仅使其整体纵移 k×LINE_H),刚性平移用
             单元素 transform 过渡,O(1) 动画任意深度尾部 -->
        <span class="flow-head">
          <span
            v-for="cell in headCells"
            :key="cell.id"
            :ref="(el) => registerCell(cell.id, el)"
            class="ch"
            :class="cellClass(cell)"
            :style="cellStyle(cell)"
            :data-cid="cell.id"
            :data-ti="cell.leaving ? undefined : cell.ti"
            >{{ cell.ch }}</span
          >
        </span>
        <span ref="tailWrapEl" class="flow-tail">
          <span
            v-for="cell in tailCells"
            :key="cell.id"
            :ref="(el) => registerCell(cell.id, el)"
            class="ch"
            :class="cellClass(cell)"
            :style="cellStyle(cell)"
            :data-cid="cell.id"
            :data-ti="cell.leaving ? undefined : cell.ti"
            >{{ cell.ch }}</span
          >
        </span>
        <span v-if="showPlaceholder" class="notes-ph">{{ t('notes.placeholder') }}</span>
        <!-- 隐形零宽探针(恒 inline):ghost 行盒高的测量源——被删字符可能仍在进场动画中
             (inline-block,行盒 = LINE_H),直接测它拿不到内容盒高;探针与字体/引擎同步 -->
        <span ref="probeEl" class="gh-probe" aria-hidden="true">&#8203;</span>
        <!-- 光标双层:外层 left/top 布局定位 + transform 平滑滑移(FLIP);内层视觉 + 闪烁 -->
        <span ref="caretEl" class="caret" :class="{ 'caret-on': caretOn }"
          ><span ref="caretCoreEl" class="caret-core"
        /></span>
      </div>
    </div>

    <!-- 真实输入源:键盘/IME/选区语义全由它承接,opacity 0 不可见(原生 caret 一并隐藏)。
         几何镜像渲染层(同宽同 wrap 规则同滚动,anchorIME 对齐),内部 caret 布局位
         即自绘光标位,IME 候选栏锚定零坐标换算 -->
    <textarea
      ref="inputEl"
      class="notes-input"
      :value="text"
      spellcheck="false"
      autocomplete="off"
      autocapitalize="off"
      @input="onInput"
      @keydown="onKeydown"
      @focus="onFocusChange"
      @blur="onFocusChange"
      @keyup="onKeyUp"
      @select="onSelectEv"
      @compositionstart="onCompStart"
      @compositionend="onCompEnd"
    />
  </div>
</template>

<script setup lang="ts">
import {
  ref,
  computed,
  nextTick,
  watch,
  onMounted,
  onActivated,
  onDeactivated,
  onUnmounted,
} from 'vue'
import { t } from '@/runtime/i18n'
import { useAppStore } from '@/stores/app'
import { whenConfigReady } from '@/runtime/storage'
import { config } from './config'
import {
  toChars,
  diffChars,
  buildIndexMap,
  ANIM_MAX_DIFF,
  FLIP_WINDOW,
  STAGGER_STEP,
  STAGGER_CAP,
  type IndexMap,
} from './logic'

// ── 常量(与 <style> 中 line-height / keyframes 时长同步)──────────────────
const LINE_H = 24 // 渲染层行高(px)
const CARET_H = 18 // 光标视觉高(px),行内垂直居中
const FLIP_MS = 150 // FLIP 位移过渡(--duration-fast)
const GHOST_MS = 150 // 离场动画时长(--duration-fast)

interface CharCell {
  id: number
  ch: string
  /** 当前文本 code point 索引;-1 = 离场 ghost(已不在文本中) */
  ti: number
  /** 进场 pop 动画(动画态 inline-block,animationend 转 inline 恢复断词) */
  fresh?: boolean
  /** 离场 ghost:absolute 定位在原位,播完移除 */
  leaving?: boolean
  gx?: number
  gy?: number
  /** ghost 行盒高(隐形 inline 探针实测的内容盒高,注入 --gh;字体 metrics 随引擎不同,禁硬编码) */
  gh?: number
  /** IME 组合中(点状下划线标记) */
  comp?: boolean
  /** 批量新增(粘贴/IME)的进场 stagger 延迟(ms) */
  delay?: number
}

const appStore = useAppStore()
const layerEl = ref<HTMLElement>()
const tailWrapEl = ref<HTMLSpanElement>()
const inputEl = ref<HTMLTextAreaElement>()
const caretEl = ref<HTMLSpanElement>()
const caretCoreEl = ref<HTMLSpanElement>()
const probeEl = ref<HTMLSpanElement>()

/// text 是唯一真理(cells 是它的视图模型);textIds 维护 code point 索引 → cell id 映射
const text = ref('')
const cells = ref<CharCell[]>([])

/** 内容坐标切分点:编辑行行尾 \n 之后(下方完整行块起点)。Infinity = 全在头部。 */
const splitContentIdx = ref(Number.MAX_SAFE_INTEGER)

/// 边界 cell 下标 = 首个 ti ≥ split 的非 ghost cell(ghost ti=-1 恒属头部)。
/// 按 ti 而非数组下标定位——ghost 回收等数组变动后重推导恒正确
function splitBoundary(): number {
  const k = cells.value.findIndex((c) => !c.leaving && c.ti >= splitContentIdx.value)
  return k < 0 ? cells.value.length : k
}
const headCells = computed(() => cells.value.slice(0, splitBoundary()))
const tailCells = computed(() => cells.value.slice(splitBoundary()))

let textIds: number[] = []
let idxMap: IndexMap = buildIndexMap('')
let nextId = 1
const composing = ref(false)
const focused = ref(false)
const selStart = ref(0)
const selEnd = ref(0)
const charEls = new Map<number, HTMLSpanElement>()
const cleanupTimers = new Set<ReturnType<typeof setTimeout>>()
let caretPos = { x: 0, y: 0 }
let caretAnim: Animation | null = null // 光标 Q 弹形变(连续移动时新动画取代旧动画)
let flipFlight = 0 // FLIP 航次计数(连续编辑时旧轮清场让位新轮)
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)')

const showPlaceholder = computed(() => text.value === '')
const caretOn = computed(() => focused.value && selStart.value === selEnd.value)

function registerCell(id: number, el: unknown) {
  if (el instanceof HTMLSpanElement) charEls.set(id, el)
  else charEls.delete(id)
}

function cellClass(cell: CharCell) {
  // 换行 span 恒 inline(inline-block 内的 \n 会在盒内断行,高度错乱),且无动画需求
  if (cell.ch === '\n') return { sel: !cell.leaving && isSel(cell.ti) }
  return {
    anim: cell.fresh,
    ghost: cell.leaving,
    comp: cell.comp,
    sel: !cell.leaving && isSel(cell.ti),
  }
}

function cellStyle(cell: CharCell) {
  if (cell.leaving)
    return {
      left: `${cell.gx ?? 0}px`,
      top: `${cell.gy ?? 0}px`,
      '--gh': `${cell.gh ?? 16}px`,
    }
  // 进场字符注入确定性伪随机微旋转(id 派生,±5.6° 步进 1.4°):
  // 每个字以各自的倾斜落定,打破同质轨迹的机械感
  if (cell.fresh) {
    const rot = (((cell.id * 37) % 9) - 4) * 1.4
    return {
      '--rot': `${rot}deg`,
      ...(cell.delay && cell.ch !== '\n' ? { animationDelay: `${cell.delay}ms` } : {}),
    }
  }
  if (cell.delay && cell.ch !== '\n') return { animationDelay: `${cell.delay}ms` }
  return undefined
}

function isSel(ti: number) {
  return ti >= selStart.value && ti < selEnd.value
}

// ── 文本应用(diff → cells 增量更新 → FLIP/ghost/caret)────────────────────

/// 字符视觉盒(相对渲染层内容系):Range over 文本节点。这是 FLIP 位移差 / ghost 出生位 /
/// 光标锚定共用的唯一测量基元——与 display 状态无关(inline 字形盒 = inline-block 内
/// 字形盒,offsetTop/Height 随盒语义漂移,Range 恒定),transform 期间反映视觉真位
/// (动画中删除时 ghost 接续字符当前视觉位置,连续无跳变)。层自身可滚,gBCR 差是
/// 视口系坐标,须加 scrollOffset 还原内容系(ghost/FLIP 基线均锚定滚动内容系)。
function charRect(el: HTMLElement): { x: number; y: number } {
  const layer = layerEl.value
  if (!layer) return { x: 0, y: 0 }
  const range = document.createRange()
  range.selectNodeContents(el)
  const r = range.getBoundingClientRect()
  // 零宽回退(\n 等):布局位经 layerTop 还原绝对系——字符若在滑移中的尾块内,
  // 裸 offsetTop 是块内相对坐标,基线会错乱
  if (r.width === 0 && r.height === 0) return { x: el.offsetLeft, y: layerTop(el) }
  const lr = layer.getBoundingClientRect()
  return {
    x: r.left - lr.left + layer.scrollLeft,
    y: r.top - lr.top + layer.scrollTop,
  }
}

function onInput() {
  const el = inputEl.value
  if (!el) return
  applyText(el.value)
  config.content = el.value
  // cells 的 DOM 在 nextTick 落地后方可测量光标落点(applyText 的 nextTick 回调先注册先执行)
  nextTick(() => syncCaret())
}

/// 新文本落位:单点 diff,前缀/后缀 cell 保 id 复用;中间删除段可见字符 ghost 化
/// (absolute 原位消失),新增段 fresh(逐字 pop,批量 stagger)。位移动画按区域分工:
/// 编辑行区(编辑点至行尾)非均匀重排走逐字符 FLIP(FLIP_WINDOW 限界);下方完整
/// 行块刚性整体纵移走尾块单元素过渡(applyTailShift)。IME 组合期间同样开动画:
/// 拼音字母逐个弹入,提交 diff(字母→汉字)天然产生旧字飘散 + 新字弹入。
/// 超 ANIM_MAX_DIFF 的大批量编辑降级为静态直更(无逐字动画),仍走增量 diff——
/// 门槛只看单次编辑规模,长文档的日常键入/删除照常动效。
function applyText(newVal: string) {
  const oldChars = toChars(text.value)
  const newChars = toChars(newVal)
  const d = diffChars(oldChars, newChars)
  const anim = d.added + d.removed <= ANIM_MAX_DIFF

  // 切分点:编辑点后首个 \n 之后 = 下方完整行块起点(这些行布局与上方无关,仅纵移)
  let nl = -1
  for (let i = d.prefix + d.added; i < newChars.length; i++) {
    if (newChars[i] === '\n') {
      nl = i
      break
    }
  }
  const splitNew = nl < 0 ? newChars.length : nl + 1
  splitContentIdx.value = splitNew
  // 尾块首字符映射回旧索引(suffix 区新旧偏移 = added − removed)
  const splitOld = splitNew - (d.added - d.removed)

  // 1. 记录基线(视觉盒基元,DOM 更新前测量):ghost 原位(删除段)、编辑行区旧位
  //    (窗口截断——其重排非均匀,须逐字符)、尾块旧位(其首字符,单点即可代表整块)
  const flipOld = new Map<number, { x: number; y: number }>()
  const ghostOld = new Map<number, { x: number; y: number }>()
  let tailOldTop: number | null = null
  if (anim && (d.added > 0 || d.removed > 0)) {
    for (let i = d.prefix; i < d.prefix + d.removed; i++) {
      const el = charEls.get(textIds[i])
      if (el) ghostOld.set(textIds[i], charRect(el))
    }
    const start = d.prefix + d.removed
    const flipEnd = Math.min(oldChars.length, splitOld, start + FLIP_WINDOW)
    for (let i = start; i < flipEnd; i++) {
      const id = textIds[i]
      const el = charEls.get(id)
      if (el) flipOld.set(id, charRect(el))
    }
    if (splitOld >= 0 && splitOld < oldChars.length) {
      const el = charEls.get(textIds[splitOld])
      // 行网格量化:块首字符字形顶(含半行距)→ 行顶,与尾块 offsetTop(块顶)同系,
      // 消除普通行内编辑的虚假 ±4px 平移(非换行编辑 dy 恒 0,不触发尾块过渡)
      if (el) tailOldTop = Math.round(charRect(el).y / LINE_H) * LINE_H
    }
  }

  // 2. 组装新 cells:一律 textIds 驱动(id → cell 映射取真实 cell)。
  //    cells 数组可能残留飘散中的旧 ghost(置于前缀后),按数组下标 slice 取
  //    被删段会把 ghost 错抓走、真实被删字符逃逸为「可见但不在 textIds」的
  //    幽灵(不可编辑);旧 ghost 不进新数组,由新 diff 中断其动画。
  const cellById = new Map(cells.value.map((c) => [c.id, c]))
  const removedCells = textIds
    .slice(d.prefix, d.prefix + d.removed)
    .map((id) => cellById.get(id))
    .filter((c): c is CharCell => !!c)
  const prefixCells = textIds
    .slice(0, d.prefix)
    .map((id) => cellById.get(id))
    .filter((c): c is CharCell => !!c)
  const suffixCells = textIds
    .slice(d.prefix + d.removed)
    .map((id) => cellById.get(id))
    .filter((c): c is CharCell => !!c)
  const ghosts: CharCell[] = []
  // ghost 行盒高取探针实测(被删字符可能仍在动画态,行盒 = LINE_H,不可直接测)
  const gh = probeEl.value?.offsetHeight ?? 16
  for (const c of removedCells) {
    const pos = ghostOld.get(c.id)
    if (!anim || c.ch === '\n' || c.ch === ' ' || !pos) continue
    ghosts.push({
      ...c,
      ti: -1,
      leaving: true,
      gx: pos.x,
      gy: pos.y,
      gh,
      fresh: false,
      comp: false,
    })
  }
  const added: CharCell[] = []
  const stagger = d.added > 1 && anim
  for (let i = 0; i < d.added; i++) {
    added.push({
      id: nextId++,
      ch: newChars[d.prefix + i],
      ti: d.prefix + i,
      fresh: anim,
      comp: composing.value,
      delay: stagger ? Math.min(i * STAGGER_STEP, STAGGER_CAP) : undefined,
    })
  }
  for (let i = 0; i < suffixCells.length; i++) {
    suffixCells[i].ti = d.prefix + d.added + i
    suffixCells[i].comp = false
  }
  cells.value = [...prefixCells, ...ghosts, ...added, ...suffixCells]
  textIds = [
    ...textIds.slice(0, d.prefix),
    ...added.map((c) => c.id),
    ...textIds.slice(d.prefix + d.removed),
  ]

  // 3. DOM 更新后:编辑行 FLIP + 尾块整体平移 + ghost 到期移除
  if (anim && (flipOld.size > 0 || ghosts.length > 0 || tailOldTop !== null)) {
    nextTick(() => {
      applyFlip(flipOld)
      applyTailShift(tailOldTop)
      for (const g of ghosts) {
        const timer = setTimeout(() => {
          cleanupTimers.delete(timer)
          cells.value = cells.value.filter((c) => c.id !== g.id)
        }, GHOST_MS + 60)
        cleanupTimers.add(timer)
      }
    })
  }

  text.value = newVal
  idxMap = buildIndexMap(newVal)
}

/// FLIP:受影响字符以 transform 从旧位平滑滑到新位(仅 transform,GPU 合成)。
/// 作用域 = flipOld(编辑行区,上游已限 FLIP_WINDOW;下方完整行的刚性纵移由
/// applyTailShift 单元素承接,不进逐字符通道)。全程内联 style(不经 Vue class
/// 绑定),重渲染不中断进行中的过渡。
/// 读批/写批分离:测量与写 transform 交替会逐元素强制重排,长文 O(n) 次
/// layout 是卡顿根源;先测完全部新位(单次 layout flush)再统一写,压回 O(1) 次。
/// 换行符 span 跳过:inline-block 化会使 \n 的换行只作用于盒内而非外部文本流,
/// 强制换行瞬间失效致后续多行并作一行,FLIP 清理后才恢复(中间输入时可见的行结构闪变)
function applyFlip(old: Map<number, { x: number; y: number }>) {
  // 读批:一次性测完全部新位(与旧位同基元的视觉盒,display 切换不产生垂直分量)
  const moved: { el: HTMLSpanElement; dx: number; dy: number }[] = []
  for (const [id, o] of old) {
    const el = charEls.get(id)
    if (!el || el.textContent === '\n') continue
    const now = charRect(el)
    const dx = o.x - now.x
    const dy = o.y - now.y
    if (dx !== 0 || dy !== 0) moved.push({ el, dx, dy })
  }
  if (moved.length === 0) return
  // 航次戳:快速连续编辑时同一字符被新一轮 FLIP 接管,旧轮清场须让位
  // (中途清 transition 会把新轮过渡掐断,字符瞬移落位)
  const flight = ++flipFlight
  // 写批 1:落 FLIP 起始帧(旧位偏移);inline-block 使 transform 生效
  for (const m of moved) {
    m.el.dataset.flipFlight = String(flight)
    m.el.style.display = 'inline-block'
    m.el.style.transition = 'none'
    m.el.style.transform = `translate(${m.dx}px, ${m.dy}px)`
  }
  void layerEl.value?.offsetHeight // 单次强制 layout,统一启动过渡
  // 写批 2:清偏移回落自然位,transform 过渡接管
  for (const m of moved) {
    m.el.style.transition = 'transform var(--duration-fast) var(--ease-out)'
    m.el.style.transform = ''
  }
  const timer = setTimeout(() => {
    cleanupTimers.delete(timer)
    for (const m of moved) {
      if (m.el.dataset.flipFlight !== String(flight)) continue // 已被后续航次接管
      delete m.el.dataset.flipFlight
      m.el.style.display = ''
      m.el.style.transition = ''
    }
  }, FLIP_MS + 60)
  cleanupTimers.add(timer)
}

/// 尾块整体平移:编辑改变行数时,编辑行以下全部完整行纵向平移 k×LINE_H。
/// 块内行布局与上方内容无关,位移恒均匀刚性——单元素 transform 过渡即可动画
/// 任意深度尾部(O(1),合成器承接,无逐字符成本),回车/删行时下方内容整体
/// 顺滑分开/合拢。基线 = 块首字符旧视觉位(gBCR 含进行中的过渡,连击从当前
/// 视觉位续接);目标 = 块 offsetTop(布局位,不受 transform 影响)。
/// 航次守卫:快速连续换行编辑时,旧轮清场定时器不得掐断进行中的新滑移
let tailFlight = 0
function applyTailShift(oldTop: number | null) {
  const el = tailWrapEl.value
  if (!el || oldTop === null || el.offsetHeight === 0) return // 空块 = 编辑在末行
  const dy = oldTop - el.offsetTop
  if (dy === 0) return
  const flight = ++tailFlight
  el.style.transition = 'none'
  el.style.transform = `translateY(${dy}px)`
  void el.offsetHeight
  el.style.transition = 'transform var(--duration-fast) var(--ease-out)'
  el.style.transform = ''
  const timer = setTimeout(() => {
    cleanupTimers.delete(timer)
    if (tailFlight !== flight) return // 后续滑移已接管,清场交由其定时器
    el.style.transition = ''
    el.style.transform = ''
  }, FLIP_MS + 60)
  cleanupTimers.add(timer)
}

// ── 光标 ────────────────────────────────────────────────────────────────

/// 读 textarea 选区(code unit → code point),更新选区渲染并移动光标。
function syncCaret() {
  const el = inputEl.value
  if (!el) return
  const { cu2cp } = idxMap
  selStart.value = cu2cp[el.selectionStart] ?? 0
  selEnd.value = cu2cp[el.selectionEnd] ?? 0
  if (caretOn.value) {
    const pos = measureCaret(selStart.value)
    moveCaret(pos.x, pos.y)
    ensureCaretVisible()
    // 滚动归位后再锚 IME:壳须取滚动后的视觉位,先锚后滚会滞留旧位
    anchorIME()
  }
}

/// 测量光标在渲染层内容坐标系的落点(纯布局位:offsetLeft/offsetWidth/offsetTop)。
/// 单字符 span 的盒边界 = advance 边界,布局位与 Range 零宽锚定等价且不受任何
/// transform 影响——中间插入后 FLIP 滑移进行中测量不被污染,光标不落错位。
/// Y 布局位 + 行网格量化:字符进场动画的瞬时位移不影响行位,量化同时吸收
/// inline(4)/inline-block(0)的盒语义差。
function measureCaret(cp: number): { x: number; y: number } {
  const ids = textIds
  const n = ids.length
  if (n === 0) return { x: 0, y: 0 }
  const anchorEl = charEls.get(ids[Math.min(cp, n - 1)])
  if (!anchorEl) return caretPos
  const y = Math.round(layerTop(anchorEl) / LINE_H) * LINE_H
  if (cp === n && text.value.endsWith('\n')) return { x: 0, y: y + LINE_H }
  return {
    x: cp < n ? anchorEl.offsetLeft : anchorEl.offsetLeft + anchorEl.offsetWidth,
    y,
  }
}

/// 元素相对渲染层内容系的布局顶位。尾块过渡期间其 transform 使之成为内部字符的
/// offsetParent,裸 offsetTop 变为块内相对坐标(回车后光标锚点恰在尾块首行)——
/// 叠加尾块自身 offsetTop 还原绝对布局位(布局位不受 transform 影响)
function layerTop(el: HTMLElement): number {
  const tail = tailWrapEl.value
  return el.offsetTop + (tail && el.offsetParent === tail ? tail.offsetTop : 0)
}

/// 光标移动:left/top 直接落位(布局),旧→新差值经 transform 反演 + transition 平滑滑入。
/// 光标平滑滑移到位(FLIP,无过冲);移动期间自身沿移动方向 Q 弹形变(拉伸→弹回)
function moveCaret(x: number, y: number) {
  const el = caretEl.value
  const core = caretCoreEl.value
  if (!el || !core) return
  const dx = caretPos.x - x
  const dy = caretPos.y - y
  caretPos = { x, y }
  el.style.left = `${x}px`
  el.style.top = `${y + (LINE_H - CARET_H) / 2}px`
  if (dx !== 0 || dy !== 0) {
    el.style.transition = 'none'
    el.style.transform = `translate(${dx}px, ${dy}px)`
    void el.offsetHeight
    el.style.transition = ''
    el.style.transform = ''
    caretAnim?.cancel()
    spawnCaretTrail(dx, dy)
    // Q 弹:三段 squash-stretch(拉伸→反弹压缩→回正),与位移并行。
    // 高度方向承载弹形(压扁→回弹,主可感知面);宽度方向独立增粗(2px 基宽的
    // 可见变宽:拖行中变粗、回弹段回细再归位),横/纵向位移均有宽度变化。
    // 分段 easing:拉出 ease-out 快速到位,回弹段 spring 过冲(WAAPI 不识别 CSS var,与 --ease-spring 同值)
    const horiz = Math.abs(dx) >= Math.abs(dy)
    if (!reduceMotion.matches) {
      const dist = Math.hypot(dx, dy)
      const s = Math.min(1.2 + dist / 90, 1.6) // 主轴拉伸幅度(下限 1.3:逐字输入也清晰,封顶 1.6)
      const squash = 1 - (s - 1) * 0.8 // 拉伸态副轴压缩
      const inv = 2 - s // 反弹态主轴压缩
      const cross = 1 + (s - 1) * 0.6 // 反弹态副轴拉伸
      const wS = Math.min(1.5 + dist / 80, 2.6) // 拉伸态宽度增粗(位移越长越宽)
      const wR = Math.max(2 - wS, 0.5) // 反弹态回细(与增粗对称的欠冲)
      const frame = (h: number, w: number) => `scaleY(${h}) scaleX(${w})`
      caretAnim = core.animate(
        [
          {
            transform: frame(horiz ? squash : s, wS),
            offset: 0,
            easing: 'cubic-bezier(0, 0, 0.2, 1)',
          },
          {
            transform: frame(horiz ? cross : inv, wR),
            offset: 0.45,
            easing: 'cubic-bezier(0.34, 1.56, 0.64, 1)',
          },
          { transform: 'none' },
        ],
        { duration: 300 }, // 与 --duration-slow 同值(WAAPI 不识别 CSS var)
      )
    }
  }
  // 重置闪烁相位:输入瞬间光标恒可见,不从半透明相位开始
  core.style.animation = 'none'
  void core.offsetHeight
  core.style.animation = ''
}

/// 光标拖尾:位移时在旧→新路径上生成一条光标全高的运动模糊条(旧位端透明 →
/// 新位端 accent 渐变,向新位收缩渐隐)。逐字输入/删除的短距条在连续击键下
/// 首尾相接成持续拖影,行导航/跳转呈长条扫过。仅 transform/opacity 过渡(合成器
/// 属性,渐变静态),animationend 自移除 + 超时兜底;reduce-motion 跳过。
/// 纯瞬时视觉层,不经 Vue(同 FLIP 内联 style 范式),随渲染层 DOM 卸载一并回收。
/// 调用时 caretPos 已是新位,旧位 = 新位 + d(d = 旧 − 新)
function spawnCaretTrail(dx: number, dy: number) {
  const layer = layerEl.value
  if (!layer || reduceMotion.matches) return
  if (layer.querySelectorAll('.caret-trail').length >= 12) return // 连击上限,防堆积
  if (Math.abs(dx) + Math.abs(dy) < 2) return // 窄字符(i/l ~4px)也保持拖尾一致
  const el = document.createElement('span')
  const gap = (LINE_H - CARET_H) / 2
  if (Math.abs(dx) >= Math.abs(dy)) {
    // 方向类决定渐变朝向(r = 新位在右)
    el.className = `caret-trail caret-trail-x ${dx < 0 ? 'caret-trail-r' : 'caret-trail-l'}`
    el.style.left = `${Math.min(caretPos.x + dx, caretPos.x)}px`
    el.style.top = `${caretPos.y + gap}px`
    el.style.width = `${Math.max(Math.abs(dx), 6)}px`
    el.style.height = `${CARET_H}px`
    el.style.transformOrigin = dx < 0 ? 'right' : 'left' // 收缩向新位(d<0 = 右移)
  } else {
    el.className = `caret-trail caret-trail-y ${dy < 0 ? 'caret-trail-d' : 'caret-trail-u'}`
    el.style.left = `${caretPos.x}px`
    el.style.top = `${Math.min(caretPos.y + dy, caretPos.y) + gap}px`
    el.style.width = '3px'
    el.style.height = `${Math.max(Math.abs(dy), 6)}px`
    el.style.transformOrigin = dy < 0 ? 'bottom' : 'top'
  }
  layer.appendChild(el)
  const timer = setTimeout(() => el.remove(), 500)
  cleanupTimers.add(timer)
  el.addEventListener('animationend', () => {
    clearTimeout(timer)
    cleanupTimers.delete(timer)
    el.remove()
  })
}

/// IME 候选栏锚定:隐藏 textarea 以「几何镜像」贴合渲染层——壳盒与层盒同位同尺寸
/// (gBCR 差值换算,与 offsetParent 参照解耦),内部滚动同步层滚动;同宽 + 同 wrap
/// 规则 + 同字体度量下,壳内换行与渲染层逐行重合,原生 caret 布局位即 caretPos。
/// WebKit 报给 IME 的 caret 屏幕位置 = 壳盒 + caretPos - 滚动,与自绘光标视觉位
/// 天然重合,零坐标换算。旧 1px 壳方案(wrap=off 内部按逻辑行布局 + 编程滚动拉回)
/// 在长行软换行下逻辑行号 ≠ 渲染层行号,中段锚定随折行差漂移,已废弃。
function anchorIME() {
  const el = inputEl.value
  const layer = layerEl.value
  const root = layer?.closest('.notes-root')
  if (!el || !layer || !root) return
  const lr = layer.getBoundingClientRect()
  const rr = root.getBoundingClientRect()
  el.style.left = `${lr.left - rr.left}px`
  el.style.top = `${lr.top - rr.top}px`
  el.style.width = `${layer.clientWidth}px`
  el.style.height = `${layer.clientHeight}px`
  el.scrollTop = layer.scrollTop
  el.scrollLeft = layer.scrollLeft
}

/// 光标行超出输入区可视范围时滚动渲染层自身(固定窗高,输入区内部滚动,
/// 无悬浮 chrome 需避让,贴行顶/行底对齐即可)
function ensureCaretVisible() {
  const layer = layerEl.value
  if (!layer) return
  const top = layer.scrollTop
  const bottom = layer.scrollTop + layer.clientHeight
  if (caretPos.y < top) layer.scrollTop = caretPos.y
  else if (caretPos.y + LINE_H > bottom) layer.scrollTop = caretPos.y + LINE_H - layer.clientHeight
}

// ── 交互承接 ─────────────────────────────────────────────────────────────

/// 点击渲染层 → 光标定位(mousedown preventDefault 保住 textarea 焦点,自管 caret offset)
function onLayerDown(e: MouseEvent) {
  e.preventDefault()
  const el = inputEl.value
  if (!el) return
  el.focus()
  const cp = caretCpFromPoint(e.clientX, e.clientY)
  const cu = idxMap.cpStart[cp] ?? 0
  el.setSelectionRange(cu, cu)
  syncCaret()
}

/// 视口坐标 → code point 偏移:caretRangeFromPoint 命中字符 span(data-ti)±半字
function caretCpFromPoint(x: number, y: number): number {
  const layer = layerEl.value
  if (!layer) return 0
  const n = textIds.length
  const doc = document as Document & {
    caretRangeFromPoint?: (x: number, y: number) => Range | null
  }
  const r = typeof doc.caretRangeFromPoint === 'function' ? doc.caretRangeFromPoint(x, y) : null
  if (!r) return n
  const node = r.startContainer
  if (node.nodeType === Node.TEXT_NODE) {
    const span = node.parentElement
    const ti = span?.dataset.ti
    if (ti !== undefined) return Number(ti) + (r.startOffset > 0 ? 1 : 0)
    return n // 命中 ghost(离场 150ms 窗口):防御落末尾
  }
  // 命中元素间隙:取 offset 前最近的非 ghost 字符 span,光标落其后
  // (字符 span 嵌于双容器内,按文档序全量取)
  const kids = Array.from(layer.querySelectorAll('.ch'))
  const at = Math.min(Math.max(r.startOffset, 0), kids.length)
  for (let i = at - 1; i >= 0; i--) {
    const ti = liveTi(kids[i])
    if (ti !== null) return ti + 1
  }
  for (const kid of kids) {
    const ti = liveTi(kid)
    if (ti !== null) return ti
  }
  return n
}

function liveTi(el: Element): number | null {
  if (!el.classList.contains('ch') || el.classList.contains('ghost')) return null
  const ti = el.getAttribute('data-ti')
  return ti !== null && ti !== '' ? Number(ti) : null
}

function onKeyUp() {
  syncCaret()
}

// ── 行导航 ──────────────────────────────────────────────────────────────
// 行/行首/行尾语义按渲染层真实行结构统一接管(行号由字符 offsetTop 量化,与光标 Y
// 同源;Cmd+Left/Right 取视觉行首/尾而非原生逻辑行首,跨引擎行语义不依赖 textarea)。

/// 渲染层行结构(按键时按需构建):每行 = 可落光标位置序列(cp + 该位置 x 布局位)。
/// 位置 i 归属行 = 字符 i 所在行(\n 后的位置由下一行行首字符决定);
/// 文末位置末字符为换行时归新行行首(与 measureCaret 语义一致)
function buildRowPositions(): { cp: number; x: number }[][] {
  const ids = textIds
  const n = ids.length
  const rows: { cp: number; x: number }[][] = []
  let curRow = -2
  let cur: { cp: number; x: number }[] = []
  for (let i = 0; i <= n; i++) {
    let r: number
    let x: number
    if (i < n) {
      const el = charEls.get(ids[i])
      if (!el) continue
      r = Math.round(layerTop(el) / LINE_H)
      x = el.offsetLeft
    } else if (n > 0) {
      const last = charEls.get(ids[n - 1])
      if (!last) return rows.length > 0 ? rows : [[{ cp: 0, x: 0 }]]
      // 末字符为换行:文末位置在新行行首
      if (last.textContent === '\n') {
        r = Math.round(layerTop(last) / LINE_H) + 1
        x = 0
      } else {
        r = Math.round(layerTop(last) / LINE_H)
        x = last.offsetLeft + last.offsetWidth
      }
    } else {
      r = 0
      x = 0
    }
    if (r !== curRow) {
      if (cur.length > 0) rows.push(cur)
      cur = []
      curRow = r
    }
    cur.push({ cp: i, x })
  }
  if (cur.length > 0) rows.push(cur)
  return rows
}

/// 行导航统一入口。dir = 上下(0 = 行首/行尾导航);edge 优先于 dir。
/// 上下键以当前光标 x 即时匹配目标行最近列(不做期望列记忆);
/// extend = Shift 扩展选区(活动端由 selectionDirection 判定);首行上/末行下落文档首/尾
function navigateRows(dir: -1 | 0 | 1, edge: 'start' | 'end' | null, extend: boolean) {
  const el = inputEl.value
  if (!el) return
  const backward = el.selectionDirection === 'backward'
  const anchor = backward ? el.selectionEnd : el.selectionStart // 固定端
  const activeCu = backward ? el.selectionStart : el.selectionEnd // 活动端
  const activeCp = idxMap.cu2cp[activeCu] ?? 0
  const rows = buildRowPositions()
  const rowIndex = rows.findIndex((r) => r.some((p) => p.cp === activeCp))
  if (rowIndex < 0) return
  const desiredX = measureCaret(activeCp).x

  let target: number
  if (edge === 'start') {
    target = rows[rowIndex][0].cp
  } else if (edge === 'end') {
    target = rows[rowIndex][rows[rowIndex].length - 1].cp
  } else {
    const ti = Math.min(Math.max(rowIndex + dir, 0), rows.length - 1)
    if (ti === rowIndex) {
      // 越界:首行上 → 文档首;末行下 → 文档尾(原生行为)
      target = dir < 0 ? 0 : textIds.length
    } else {
      let best = rows[ti][0]
      for (const p of rows[ti]) {
        if (Math.abs(p.x - desiredX) < Math.abs(best.x - desiredX)) best = p
      }
      target = best.cp
    }
  }

  const cu = idxMap.cpStart[target] ?? 0
  if (extend) {
    el.setSelectionRange(
      Math.min(anchor, cu),
      Math.max(anchor, cu),
      cu < anchor ? 'backward' : 'forward',
    )
  } else {
    el.setSelectionRange(cu, cu)
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.isComposing) return
  const isUp = e.key === 'ArrowUp'
  const isDown = e.key === 'ArrowDown'
  const isLineStart = e.metaKey && e.key === 'ArrowLeft'
  const isLineEnd = e.metaKey && e.key === 'ArrowRight'
  if (!isUp && !isDown && !isLineStart && !isLineEnd) return
  e.preventDefault()
  navigateRows(
    isUp ? -1 : isDown ? 1 : 0,
    isLineStart ? 'start' : isLineEnd ? 'end' : null,
    e.shiftKey,
  )
  syncCaret()
}

function onSelectEv() {
  syncCaret()
}

function onFocusChange() {
  focused.value = document.activeElement === inputEl.value
  syncCaret()
}

// ── IME ─────────────────────────────────────────────────────────────────

function onCompStart() {
  composing.value = true
}

/// compositionend(WebKit 中在末次 input 之后):提交 diff(字母→汉字)的
/// pop/ghost/FLIP 已由该次 input 承担,此处仅清组合下划线标记
function onCompEnd() {
  const el = inputEl.value
  if (el && el.value !== text.value) applyText(el.value)
  composing.value = false
  let touched = false
  for (const c of cells.value) {
    if (c.comp) {
      c.comp = false
      touched = true
    }
  }
  if (touched) cells.value = [...cells.value]
  nextTick(() => syncCaret())
}

/// animationend 委托:pop 播完清标记(inline-block 恢复 inline,断词回归正常)
function onAnimEnd(e: AnimationEvent) {
  const el = e.target
  if (!(el instanceof HTMLSpanElement)) return
  if (!el.classList.contains('anim')) return
  const id = Number(el.dataset.cid)
  const cell = cells.value.find((c) => c.id === id)
  if (!cell) return
  cell.fresh = false
  cells.value = [...cells.value]
}

// ── 持久化恢复 + 生命周期 ────────────────────────────────────────────────

/// 从 config 全量初始化(恢复/重挂载路径,无动画直渲)
function initText(s: string) {
  const list = toChars(s)
  const rebuilt: CharCell[] = list.map((ch, i) => ({ id: nextId++, ch, ti: i }))
  textIds = rebuilt.map((c) => c.id)
  cells.value = rebuilt
  // 恢复路径无动画,整体入头部容器;后续编辑按其切分点重划分
  splitContentIdx.value = Number.MAX_SAFE_INTEGER
  text.value = s
  idxMap = buildIndexMap(s)
}

// Settings 清空(config 外部置空)时经统一 applyText 走清空动画;
// 正常输入路径 config 恒追随 text,不会触发本 watch 的反向同步
watch(
  () => config.content,
  (v) => {
    if (v === '' && text.value) applyText('')
  },
)

onMounted(() => {
  whenConfigReady('extensions/notes/config').then(() => {
    if (!text.value && config.content) {
      initText(config.content)
      // 编程置 value 会把原生选区复位到文末,selectionchange → syncCaret →
      // ensureCaretVisible 即滚到底;value 的 DOM 补丁在下次 flush,归零选区
      // 须排其后——恢复后停在顶部,光标在开头
      nextTick(() => inputEl.value?.setSelectionRange(0, 0))
    }
  })
  document.addEventListener('selectionchange', onDocSelectionChange)
  window.addEventListener('window-focused', onWinFocused)
  // 输入区滚动即时重锚 IME 壳:壳定位在层外(notes-root),不随层内容滚动,
  // scroll 事件是唯一同步时机——不等下一次输入,否则 compositionstart 首次
  // 锚定读到滚前旧位,候选栏漂移一个滚动量
  layerEl.value?.addEventListener('scroll', anchorIME, { passive: true })
})

function onDocSelectionChange() {
  if (document.activeElement === inputEl.value) syncCaret()
}

onUnmounted(() => {
  document.removeEventListener('selectionchange', onDocSelectionChange)
  window.removeEventListener('window-focused', onWinFocused)
  layerEl.value?.removeEventListener('scroll', anchorIME)
  caretAnim?.cancel()
  for (const timer of cleanupTimers) clearTimeout(timer)
  cleanupTimers.clear()
})

/// 窗口隐藏时跳过聚焦(WKWebView 可编辑元素聚焦会抢前台),获焦后补聚焦(translate 同范式)
function maybeFocus() {
  if (!document.hasFocus()) return
  nextTick(() => inputEl.value?.focus())
}
function onWinFocused() {
  if (appStore.activeExtId !== 'notes' || appStore.activeSubview) return
  maybeFocus()
}
onMounted(maybeFocus)
onActivated(maybeFocus)
onDeactivated(() => {
  inputEl.value?.blur()
})
</script>

<style scoped>
.notes-layer {
  font-size: 14px;
  line-height: 24px; /* 与 LINE_H 同步 */
  white-space: pre-wrap;
  word-break: break-word;
  user-select: none;
  -webkit-user-select: none;
  cursor: text;
}

/* 尾块(编辑行下方完整行):block 化以承接单元素 transform 过渡——inline 盒
   transform 不生效;块起始必为行首(split 落在行尾 \n 后),块边界不改变行布局。
   头部容器保持默认 inline,行内断词跨容器边界与单流一致(断行按文本内容驱动) */
.flow-tail {
  display: block;
}

/* 字符:稳定态 inline(正常断词);动画态 inline-block(transform 生效的前提) */
.ch {
  display: inline;
}
/* 落字:spring 单段弹入——更小更深的起点放大过冲绝对量(落定瞬间字形轻微膨胀),
   半透明凝聚起点(墨迹凝成),伪随机微旋转经 --rot 注入(每字各自的倾斜落定) */
.ch.anim {
  display: inline-block;
  animation: ch-pop var(--duration-normal) var(--ease-spring) backwards;
}
/* 离场 ghost:absolute 定位原位(脱离行内流,后字即时合拢由 FLIP 接管),
   向右飘移渐隐。行盒高经 --gh 注入被删字符的内容盒实测值(offsetHeight):
   gx/gy 取自 inline 字符的 offsetLeft/Top(= 字形内容盒顶),行盒高等于内容盒高时
   half-leading 为 0、盒顶即字形顶——垂直对齐与字体 metrics / 渲染引擎无关(硬编码
   行高在 WKWebView 下会因 ascent/descent 解析差异把字形顶出行盒,呈整体偏上) */
.ch.ghost {
  position: absolute;
  display: inline-block;
  line-height: var(--gh, 16px);
  pointer-events: none;
  animation: ch-ghost var(--duration-fast) var(--ease-in) forwards;
}
.ch.sel {
  background: color-mix(in srgb, var(--color-accent) 24%, transparent);
}
.ch.comp {
  text-decoration: underline dotted;
  text-decoration-color: color-mix(in srgb, var(--color-accent) 55%, transparent);
  text-underline-offset: 3px;
}

.notes-ph {
  position: absolute;
  left: 0;
  top: 0;
  pointer-events: none;
  color: var(--color-text-muted);
}

/* 隐形零宽 inline 探针:零宽不占位不断行,offsetHeight 恒为该引擎的内容盒高 */
.gh-probe {
  visibility: hidden;
}

.caret {
  position: absolute;
  width: 2px;
  height: 18px; /* 与 CARET_H 同步 */
  pointer-events: none;
  opacity: 0;
  transition:
    transform var(--duration-fast) var(--ease-out),
    opacity var(--duration-fast) var(--ease-out);
}
.caret-on {
  opacity: 1;
}
.caret-core {
  display: block;
  width: 100%;
  height: 100%;
  border-radius: 1px;
  background: var(--color-accent);
}
.caret-on .caret-core {
  animation: caret-blink 1.1s var(--ease-in-out) infinite;
}

/* 光标拖尾:位移路径上的运动模糊条(渐变方向/尺寸由 inline 注入,旧位端透明 →
   新位端 accent,origin 收缩向新位)。横/纵两取向各自的收缩轴;合成器属性。
   元素为命令式创建(无 scoped 属性),须 :deep 后代选择器命中 */
:deep(.caret-trail) {
  position: absolute;
  pointer-events: none;
  border-radius: 2px;
}
/* 渐变方向随移动方向(旧位端透明 → 新位端 accent)。
   渐变留在 CSS:JS 模板串里的 var(--…)) 字面量会被 UnoCSS attributify 扫描
   误提取为工具类,生成非法 CSS 令生产构建失败 */
:deep(.caret-trail-r) {
  background: linear-gradient(to right, transparent, var(--color-accent));
}
:deep(.caret-trail-l) {
  background: linear-gradient(to left, transparent, var(--color-accent));
}
:deep(.caret-trail-d) {
  background: linear-gradient(to bottom, transparent, var(--color-accent));
}
:deep(.caret-trail-u) {
  background: linear-gradient(to top, transparent, var(--color-accent));
}
:deep(.caret-trail-x) {
  animation: caret-trail-x var(--duration-slow) var(--ease-out) forwards;
}
:deep(.caret-trail-y) {
  animation: caret-trail-y var(--duration-slow) var(--ease-out) forwards;
}
@keyframes caret-trail-x {
  from {
    opacity: 0.55;
  }
  to {
    opacity: 0;
    transform: scaleX(0.15);
  }
}
@keyframes caret-trail-y {
  from {
    opacity: 0.55;
  }
  to {
    opacity: 0;
    transform: scaleY(0.15);
  }
}

.notes-input {
  position: absolute;
  /* 尺寸/位置由 anchorIME 镜像渲染层(inline);此处仅占位,聚焦前不可见 */
  width: 1px;
  height: 24px;
  opacity: 0;
  pointer-events: none;
  border: none;
  padding: 0;
  resize: none;
  overflow: hidden;
  background: transparent;
  /* 与渲染层同字体度量 + 同 wrap 规则(pre-wrap + break-word,同 .notes-layer):
     同引擎同约束下换行逐行重合,内部 caret 布局位 == caretPos */
  font-family: inherit;
  font-size: 14px;
  line-height: 24px;
  white-space: pre-wrap;
  word-break: break-word;
}

@keyframes ch-pop {
  from {
    opacity: 0.2;
    transform: translateY(0.5em) scale(0.18) rotate(var(--rot, 0deg));
  }
  to {
    opacity: 1;
    transform: none;
  }
}
@keyframes ch-ghost {
  from {
    opacity: 1;
    transform: none;
  }
  to {
    opacity: 0;
    transform: translateX(0.9em);
  }
}
@keyframes caret-blink {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.14;
  }
}

@media (prefers-reduced-motion: reduce) {
  .ch.anim,
  .ch.ghost,
  :deep(.caret-trail-x),
  :deep(.caret-trail-y) {
    animation: none;
  }
  .caret {
    transition: none;
  }
  .caret-on .caret-core {
    animation: none;
  }
}
</style>
