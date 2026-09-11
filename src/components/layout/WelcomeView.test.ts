import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'main', onCloseRequested: () => Promise.resolve(() => {}) }),
}))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
// isTauri=true 打开权限 chip 的点击链路（组件内仅 handlePerm 消费）
vi.mock('@/utils/tauri', () => ({ isTauri: true }))

import WelcomeView from './WelcomeView.vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useSystemStore } from '@/stores/system'
import { defineExtension, _resetForTest } from '@/runtime/extension-registry'
import { locale } from '@/runtime/i18n'
import '@/locales'

/// 全窗口视图直接渲染（无 Teleport），断言走 wrapper 本身
const mountedWrappers: ReturnType<typeof mount>[] = []

function mountView() {
  // attachTo body：focus() 需元素在文档内（happy-dom 对游离元素不生效）
  const wrapper = mount(WelcomeView, { attachTo: document.body })
  mountedWrappers.push(wrapper)
  return wrapper
}

/// 功能键默认键位经扩展注册表读取（globalShortcuts[].default），测试注册轻量 stub
function registerFnStubs(skip?: string) {
  for (const [id, def] of [
    ['agent', 'Alt+A'],
    ['screenshot', 'Alt+S'],
    ['translate', 'Alt+T'],
    ['finder-ext', 'Alt+F'],
    ['clipboard', 'Alt+C'],
    ['notes', 'Alt+N'],
  ] as const) {
    if (id === skip) continue
    defineExtension({
      meta: { id, name: id, icon: 'i-ri-apps-2-line', order: 0 },
      globalShortcuts: [{ id, default: def, onExecute: () => {} }],
    })
  }
}

describe('WelcomeView 首启引导视图', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    locale.value = 'zh-CN'
    _resetForTest()
    registerFnStubs()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
    _resetForTest()
  })

  it('键位图（图纸区纯信息展示）：主快捷键板 + ⌘ 占位弱化板 + 五颗功能键板 + 等距网格 + 全引出线标注 + 左下角图签', () => {
    const wrapper = mountView()
    const text = wrapper.text()
    // 图纸：顶排 T 板 + 两块主快捷键板（修饰键/Space）+ ⌘ 占位板 + 六块功能键板 + 「/」语法键；底层横向网格线
    expect(wrapper.findAll('g.w-plate').length).toBe(10)
    // 网格线：贴键底缘的横向线族（四排键上下底缘 8 层，沿 u 轴）——全部段合并单 path
    const gridD = wrapper.find('path.w-grid').attributes('d') ?? ''
    expect((gridD.match(/M /g) ?? []).length).toBeGreaterThanOrEqual(8)
    expect(wrapper.find('g.w-plate-ghost').text()).toContain('⌘')
    // 修饰键板特殊色（mod tone）：与 main/fn 的 accent 蓝线系区分，标注「都要按」
    expect(wrapper.find('g.w-plate-mod').text()).toContain('⌥')
    expect(wrapper.find('svg.w-iso').attributes('aria-label')).toBe('唤起窗口 ⌥ + Space')
    // 标注全部经引出线：修饰键/唤起窗口 + 六个功能词 + 「/」语法键，共 9 组
    expect(wrapper.findAll('g.w-callout').length).toBe(9)
    expect(text).toContain('修饰键')
    expect(text).toContain('唤起窗口')
    expect(text).toContain('Agent')
    expect(text).toContain('截屏')
    expect(text).toContain('翻译')
    expect(text).toContain('访达工具')
    expect(text).toContain('剪贴板')
    expect(text).toContain('记事本')
    // 图纸态底部单句按键提示（右下角）：Enter / → 下一步 · ← 上一步；无 ↩ 键帽
    expect(wrapper.find('.w-footer .w-note').text()).toBe('Enter / → 下一步 · ← 上一步')
    // 图纸初始等比缩放 1（展开态随 expandF 插值到 0.8；缩放层不含图签）
    expect(wrapper.find('g.w-zoom').attributes('transform')).toMatch(/scale\(1\.0000\)/)
    const caps = wrapper.findAll('text.w-cap').map((c) => c.text())
    expect(caps).not.toContain('↩')
    expect(caps).not.toContain('⌘↩')
    // 图签：左下角应用名（入场序列末位）+ 品牌口号（名字下方）；纯信息展示无按钮
    const title = wrapper.find('text.w-title')
    expect(title.text()).toBe('Voidnix')
    // 「/」语法键：键帽 + 引出线双行词块（动词「输入」，与快捷键名词标注区分；含 // 搜索）
    expect(wrapper.findAll('text.w-cap').map((c) => c.text())).toContain('/')
    expect(text).toContain('输入/显示扩展')
    expect(text).toContain('输入//快速搜索')
    expect(parseFloat(title.attributes('x')!)).toBeLessThan(100)
    expect(parseFloat(title.attributes('y')!)).toBeGreaterThan(340)
    const tagline = wrapper.find('text.w-tagline')
    expect(tagline.text()).toBe('想到就到，触手可及')
    // 口号在名字下方：同列左对齐、基线低于图签，textLength 同宽（两端严格对齐）
    expect(parseFloat(tagline.attributes('x')!)).toBe(parseFloat(title.attributes('x')!))
    expect(parseFloat(tagline.attributes('y')!)).toBeGreaterThan(parseFloat(title.attributes('y')!))
    expect(tagline.attributes('textLength')).toBe(title.attributes('textLength'))
    // 标题块整体下沉 12px：口号底缘（基线 + 中文下降 ≈2）沉到板阵最低点下方（不再齐平）
    const plateBottom = Math.max(
      ...wrapper
        .findAll('g.w-plate path')
        .flatMap((p) =>
          [...(p.attributes('d') ?? '').matchAll(/(-?[\d.]+) (-?[\d.]+)/g)].map((m) =>
            parseFloat(m[2]!),
          ),
        ),
    )
    expect(parseFloat(tagline.attributes('y')!) + 2 - plateBottom).toBeGreaterThan(10)
    // 图纸区零交互元素；权限面板常驻 DOM 但未展开（w-expanded 未挂、不可交互）
    expect(wrapper.find('.welcome-view').classes()).not.toContain('w-expanded')
    expect(wrapper.findAll('.w-perm-row button')).toHaveLength(3)
  })

  it('en 口号不设 textLength（词形自然宽，防同宽挤压字距）；图签标题恒设', () => {
    locale.value = 'en'
    const wrapper = mountView()
    expect(wrapper.find('text.w-tagline').attributes('textLength')).toBeUndefined()
    expect(wrapper.find('text.w-title').attributes('textLength')).toBe('96')
    // 「/」语法键引出线同语言切换（双行词块，含 // 搜索文案）
    expect(wrapper.text()).toContain('Type / to show extensions')
    expect(wrapper.text()).toContain('Type // for quick search')
    // 底部按键提示同语言切换（图纸态右下角单句）
    expect(wrapper.find('.w-footer .w-note').text()).toBe('Enter / → next · ← back')
  })

  it('画布不溢出：全部图形元素坐标（板/网格路径、引出线锚点、图签）落在 viewBox 720×480 内', () => {
    const wrapper = mountView()
    const W = 720
    const H = 480
    const nums = (s: string) => [...s.matchAll(/-?[\d.]+/g)].map((m) => parseFloat(m[0]!))
    const check = (vals: number[], what: string) => {
      for (let i = 0; i + 1 < vals.length; i += 2) {
        expect(vals[i]!, `${what} x=${vals[i]}`).toBeGreaterThanOrEqual(-1)
        expect(vals[i]!, `${what} x=${vals[i]}`).toBeLessThanOrEqual(W + 1)
        expect(vals[i + 1]!, `${what} y=${vals[i + 1]}`).toBeGreaterThanOrEqual(-1)
        expect(vals[i + 1]!, `${what} y=${vals[i + 1]}`).toBeLessThanOrEqual(H + 1)
      }
    }
    for (const p of wrapper.findAll('g.w-plate path')) check(nums(p.attributes('d') ?? ''), 'plate')
    check(nums(wrapper.find('path.w-grid').attributes('d') ?? ''), 'grid')
    for (const g of wrapper.findAll('g.w-callout')) {
      check(nums(g.find('path').attributes('d') ?? ''), 'callout path')
      const c = g.find('circle')
      check([...nums(c.attributes('cx') ?? ''), ...nums(c.attributes('cy') ?? '')], 'callout dot')
    }
    // 图签坐标即画布坐标（无整图旋转）
    for (const cls of ['text.w-title', 'text.w-tagline']) {
      const el = wrapper.find(cls)!
      check([parseFloat(el.attributes('x')!), parseFloat(el.attributes('y')!)], cls)
    }
  })

  it('键位图几何：顶面为等距菱形（远侧角在中心上方，防投影符号回归）', () => {
    const wrapper = mountView()
    const plates = wrapper.findAll('g.w-plate')
    expect(plates.length).toBeGreaterThan(0)
    for (const p of plates) {
      const d = p.find('path.w-top').attributes('d') ?? ''
      // 四个角圆弧的 Q 控制点即四角：前/后角关于中心对称，y 极差 = KU·a+KV·b
      // （单键 ≈ 47.9）。back/left 的 y 符号写反时菱形塌缩成自交沙漏
      const ctrlYs = [...d.matchAll(/Q (-?[\d.]+) (-?[\d.]+)/g)].map((m) => parseFloat(m[2]))
      expect(ctrlYs).toHaveLength(4)
      expect(Math.max(...ctrlYs) - Math.min(...ctrlYs)).toBeGreaterThan(25)
    }
  })

  it('引出线与剖切几何：文字对齐水平段尾端（左拉左端/右拉右端）且离线 8px，Space 线下行转折开口向上、N 线自顶部上行右拉，侧壁剖切左壁 45°/右壁 70° 加密', () => {
    const wrapper = mountView()
    // 剖切填充：均右上向——左壁 45°、线距 6；右壁 70°、线距 5（缘线陡升 54°，45° 近并行显平）
    expect(wrapper.find('pattern#w-hatch-l').attributes('patternTransform')).toBe('rotate(45)')
    const hatchR = wrapper.find('pattern#w-hatch-r')
    expect(hatchR.attributes('patternTransform')).toBe('rotate(20)')
    expect(hatchR.attributes('width')).toBe('5')

    const groups = wrapper.findAll('g.w-callout')
    expect(groups.length).toBe(9)
    // 标注词定位走 transform（x/y attribute 变化触发 SVG text 重排，动画帧预算关键）
    const coPos = (el: { attributes: (n: string) => unknown }) => {
      const m = String(el.attributes('transform') ?? '').match(
        /translate\((-?[\d.]+) (-?[\d.]+)\)/,
      )!
      return [parseFloat(m[1]!), parseFloat(m[2]!)] as const
    }
    for (const g of groups) {
      const m = (g.find('path').attributes('d') ?? '').match(
        /^M (-?[\d.]+) (-?[\d.]+) L (-?[\d.]+) (-?[\d.]+) L (-?[\d.]+) (-?[\d.]+)$/,
      )
      expect(m, `path 无法解析: ${g.find('path').attributes('d')}`).not.toBeNull()
      // 首两个空洞跳过全匹配与未用的 x1
      const [, , y1, x2, y2, x3, y3] = m!.map(Number)
      expect(y2).toBe(y3) // 末段水平
      // 文字对齐水平段尾端（左拉 start 于左端、右拉 end 于右端）；全图单级，基线离线 8px
      const goesRight = x3 > x2
      const texts = g.findAll('text')
      expect(texts.length).toBeLessThanOrEqual(2)
      const tx = texts[texts.length - 1]!
      const [tx0, ty0] = coPos(tx)
      expect(tx.attributes('text-anchor') ?? 'start').toBe(goesRight ? 'end' : 'start')
      expect(tx0).toBeCloseTo(goesRight ? Math.max(x2, x3) : Math.min(x2, x3))
      expect(ty0).toBeCloseTo(y2 - 8)
      // 「/」语法键双行词块：首行同锚右对齐、整体上移一行（行距 13）
      if (texts.length === 2) {
        const first = texts[0]!
        const [fx, fy] = coPos(first)
        expect(first.attributes('text-anchor')).toBe(tx.attributes('text-anchor'))
        expect(fx).toBeCloseTo(tx0)
        expect(fy).toBeCloseTo(y2 - 8 - 13)
      }
      // Space 组：斜段下行（肘点为全线最低点），转折开口向上；N 组：斜段自顶部上行；
      // T（翻译）组：斜段下行右折（转折开口向上，同 Space 折角）
      if (g.text().includes('唤起窗口')) {
        expect(y2).toBeGreaterThan(y1)
        expect(y2).toBeGreaterThanOrEqual(Math.max(y1, y3))
      }
      if (g.text().includes('记事本')) {
        expect(y2).toBeLessThan(y1)
      }
      if (g.text().includes('翻译')) {
        expect(y2).toBeGreaterThan(y1)
      }
    }
  })

  it('N / Space 引出线端点：N 锚顶部远侧边中心（外移 6px）、Space 正对右侧边中心（沿边法向外移 12px）且脱出 8px 壁带', () => {
    const wrapper = mountView()
    // w-cap 贴面矩阵 matrix(1 KU/2 −1 KV/2 cx cy)：中心在 e/f 分量
    const capPt = (sym: string) => {
      const plate = wrapper.findAll('g.w-plate').find((p) => p.find('text.w-cap').text() === sym)!
      const nums = (plate.find('text.w-cap').attributes('transform') ?? '').match(/-?[\d.]+/g)!
      return [parseFloat(nums[4]!), parseFloat(nums[5]!)] as const
    }
    const dotOf = (label: string) => {
      const g = wrapper.findAll('g.w-callout').find((gg) => gg.text().includes(label))!
      const c = g.find('circle')
      return [parseFloat(c.attributes('cx')!), parseFloat(c.attributes('cy')!)] as const
    }
    const KEY_B = 17.53
    const N_HAT: readonly [number, number] = [0.8094, 0.5872] // 边法向单位向量（KU=0.5995 / KV=2.7569）
    // Space 端点 = 边中点 (cx + a, cy + KU·a/2) + 边法向 × 12（正对边中心的外侧锚定）
    const offset = (dot: readonly [number, number], center: readonly [number, number], a: number) =>
      Math.hypot(
        dot[0] - (center[0] + a + 12 * N_HAT[0]),
        dot[1] - (center[1] + (0.5995 * a) / 2 + 12 * N_HAT[1]),
      )
    expect(offset(dotOf('唤起窗口'), capPt('Space'), 137.5)).toBeLessThan(0.5)
    // N 端点 = 远侧边中点 (KEY_B, −KV·KEY_B/2) 沿 (KU, −2) 单位向量外移 6px（CO_FAR）
    const nCenter = capPt('N')
    const nDot = dotOf('记事本')
    expect(Math.hypot(nDot[0] - (nCenter[0] + 19.25), nDot[1] - (nCenter[1] - 29.92))).toBeLessThan(
      0.5,
    )
    // Space dot 沿边的竖直深入须超出壁底（8px）至少 dot 半径 3px
    const depth = (
      dot: readonly [number, number],
      center: readonly [number, number],
      a: number,
    ) => {
      const fx = center[0] + a - KEY_B
      const fy = center[1] + (0.5995 * a + 2.7569 * KEY_B) / 2
      return dot[1] - (fy - 1.37845 * (dot[0] - fx))
    }
    expect(depth(dotOf('唤起窗口'), capPt('Space'), 137.5)).toBeGreaterThan(11)
  })

  it('网格线防穿透：每段（端点与中点）反投影后不深入任何板顶面菱形（壁深 8px 含入）', () => {
    const wrapper = mountView()
    // 网格几何常量（WelcomeView 生产值镜像）：O/U/V、半宽（槽位）、壁深
    const OX = 230
    const OY = 119
    const UY = 22.18
    const VY = 88.22
    const UX = 74 // u 步进（横排键距）
    const VX = 64 // v 步进（排距）
    const A_SLOTS = 31.5 / UX // 单键 u 半宽
    const SPACE_A_SLOTS = 137.5 / UX
    const B_SLOTS = 17.53 / VX // v 半宽
    const WALL_T = 8 / (UY + VY) // 壁垂直挤出在网格平面的 (P,Q) 同增量
    const TOL = 0.05 // ~3px：断开端点贴切边（浮点抖动），深入超过容差才算穿透
    // 板 (col, row, u 半宽) 布局表（含顶排 T 板 row −1）
    const plates: Array<[number, number, number]> = [
      [4, -1, A_SLOTS],
      [0, 0, A_SLOTS],
      [1, 0, A_SLOTS],
      [3, 0, A_SLOTS],
      [2, 1, A_SLOTS],
      [4, 1, A_SLOTS],
      [6, 1, A_SLOTS],
      [0, 2, A_SLOTS],
      [1, 2, A_SLOTS],
      [3.4324, 2, SPACE_A_SLOTS],
    ]
    const insidePlate = (x: number, y: number) => {
      // 屏幕点反 drop（回到顶面平面）再严格解 (P, Q)：两轴步进分离（UX≠VX），
      // 由 x' = UX·p − VX·q、y' = UY·p + VY·q 消 p 解 q
      const x1 = x - OX
      const y1 = y - 8 - OY
      const q = (y1 - (UY / UX) * x1) / (VY + (VX * UY) / UX)
      const p = (x1 + VX * q) / UX
      return plates.some(
        ([col, row, a]) =>
          Math.abs(p - col) < a - TOL && Math.abs(q - row) < B_SLOTS + WALL_T - TOL,
      )
    }
    // 单 path 合并的网格段拆回子路径逐段校验
    const segs =
      (wrapper.find('path.w-grid').attributes('d') ?? '').match(
        /M -?[\d.]+ -?[\d.]+ L -?[\d.]+ -?[\d.]+/g,
      ) ?? []
    expect(segs.length).toBeGreaterThan(0)
    for (const seg of segs) {
      const m = seg.match(/^M (-?[\d.]+) (-?[\d.]+) L (-?[\d.]+) (-?[\d.]+)$/)
      expect(m, `网格段无法解析: ${seg}`).not.toBeNull()
      const x1 = parseFloat(m![1]!)
      const y1 = parseFloat(m![2]!)
      const x2 = parseFloat(m![3]!)
      const y2 = parseFloat(m![4]!)
      for (const [px, py] of [
        [x1, y1],
        [x2, y2],
        [(x1 + x2) / 2, (y1 + y2) / 2],
      ]) {
        expect(insidePlate(px, py), `网格段 ${seg} 采样点 (${px},${py}) 深入板内`).toBe(false)
      }
    }
  })

  it('列位对齐：⌘ 紧邻 Space 仅一条缝，N 右缘与 Space 右缘齐平（悬于右段上方）', () => {
    const wrapper = mountView()
    const capX = (sym: string) => {
      const plate = wrapper.findAll('g.w-plate').find((p) => p.find('text.w-cap').text() === sym)
      const nums = (plate!.find('text.w-cap').attributes('transform') ?? '').match(/-?[\d.]+/g)!
      return parseFloat(nums[4]!)
    }
    const KEY_A = 31.5 // 单键半宽（沿 u 屏幕分量）
    const SPACE_A = 137.5 // Space 半宽（⌘–Space 水平重叠与单键一致取值）
    const cmd = capX('⌘')
    const space = capX('Space')
    const n = capX('N')
    // 横向间距均匀：⌘–Space 缝与单键间缝一致（均为 U − 2a）
    expect(space - SPACE_A - (cmd + KEY_A)).toBeCloseTo(74 - 2 * KEY_A)
    // N 悬于 Space 右段上方，右缘精确齐平
    expect(n + KEY_A).toBeCloseTo(space + SPACE_A)
  })

  it('双修饰主快捷键：两修饰占满 col0/col1（⌘ 占位让位），动作键落宽板，aria 完整', () => {
    useSettingsStore().globalShortcut = 'Control+Alt+Space'
    const wrapper = mountView()
    const symbols = wrapper.findAll('text.w-cap').map((c) => c.text())
    expect(symbols).toContain('⌃')
    expect(symbols).toContain('⌥')
    expect(symbols).toContain('Space')
    // 双修饰占满修饰槽，无弱化占位板；两块修饰板均琥珀（都要按标记）
    expect(wrapper.find('g.w-plate-ghost').exists()).toBe(false)
    expect(wrapper.findAll('g.w-plate-mod').length).toBe(2)
    expect(wrapper.find('svg.w-iso').attributes('aria-label')).toBe('唤起窗口 ⌃ + ⌥ + Space')
  })

  it('单键主快捷键：修饰槽双弱化占位（⌥⌘ ghost），单键落宽板，修饰键标注不渲染', () => {
    useSettingsStore().globalShortcut = 'F1'
    const wrapper = mountView()
    const ghosts = wrapper.findAll('g.w-plate-ghost').map((g) => g.text())
    expect(ghosts.sort()).toEqual(['⌘', '⌥'])
    // 标注仅剩 唤起窗口 + 六个功能词 + 「/」语法键（无修饰键可标）
    expect(wrapper.findAll('g.w-callout').length).toBe(8)
    expect(wrapper.text()).not.toContain('修饰键')
    expect(wrapper.find('svg.w-iso').attributes('aria-label')).toBe('唤起窗口 F1')
    // 单键落宽板（main tone）
    const wide = wrapper.findAll('g.w-plate').find((p) => p.find('text.w-cap').text() === 'F1')
    expect(wide).toBeTruthy()
    expect(wide!.classes()).toContain('w-plate-main')
  })

  it('三修饰主快捷键：前两修饰落板，余下修饰并入宽板键名（信息完整不丢键）', () => {
    useSettingsStore().globalShortcut = 'Control+Alt+Shift+K'
    const wrapper = mountView()
    const symbols = wrapper.findAll('text.w-cap').map((c) => c.text())
    expect(symbols).toContain('⌃')
    expect(symbols).toContain('⌥')
    // 第三修饰与动作键并入宽板
    expect(symbols).toContain('⇧K')
    expect(wrapper.find('g.w-plate-ghost').exists()).toBe(false)
  })

  it('dev 构建叠加不进板阵：⌥/⌘ 占位/Space 保持 Alt 基布局示意（回归锚点）', () => {
    // dev 注册态的 Shift 叠加在 Rust shortcut.rs 侧，板阵恒读 store 原值（Alt 基）
    const wrapper = mountView()
    // dev 注册叠加的 ⇧ 不挤占键盘布局示意：⌥ mod 琥珀 + ⌘ 弱化占位 + Space（「/」非快捷键不受影响）
    const symbols = wrapper.findAll('text.w-cap').map((c) => c.text())
    expect(symbols).toContain('⌥')
    expect(symbols).toContain('⌘')
    expect(symbols).toContain('Space')
    expect(symbols).toContain('/')
    expect(wrapper.find('g.w-plate-mod').text()).toContain('⌥')
    expect(wrapper.find('g.w-plate-ghost').text()).toContain('⌘')
    expect(wrapper.find('svg.w-iso').attributes('aria-label')).toBe('唤起窗口 ⌥ + Space')
  })

  it('功能键 id 失效：板与引出线整组跳过（运行时读注册表，不镜像默认值）', () => {
    _resetForTest()
    registerFnStubs('notes')
    const wrapper = mountView()
    const symbols = wrapper.findAll('text.w-cap').map((c) => c.text())
    expect(symbols).not.toContain('N')
    expect(wrapper.text()).not.toContain('记事本')
    // 其余五颗功能键板照常（+「/」语法键不受注册表影响）
    for (const s of ['A', 'S', 'T', 'F', 'C']) expect(symbols).toContain(s)
    expect(wrapper.findAll('g.w-plate').length).toBe(9)
  })

  it('Enter 两段式：图纸态展开权限面板（不完结），展开态完结落盘；重复按键不二次 emit', async () => {
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    const settings = useSettingsStore()

    // 图纸态：Enter 展开权限面板（图纸缩小左移 + 右侧面板滑入），不完结不落盘
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await nextTick()
    expect(wrapper.emitted('done')).toBeUndefined()
    expect(settings.onboarded).toBe(false)
    expect(wrapper.find('.welcome-view').classes()).toContain('w-expanded')

    // 展开态：Enter 完结，自持落盘 onboarded
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(wrapper.emitted('done')).toHaveLength(1)
    expect(settings.onboarded).toBe(true)
    // 完结一次性：done 之后的重复按键不再 emit
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(wrapper.emitted('done')).toHaveLength(1)
  })

  it('方向键：ArrowRight 展开权限面板 / ArrowLeft 收起回图纸，不触发完结', async () => {
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await nextTick()
    expect(wrapper.find('.welcome-view').classes()).toContain('w-expanded')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }))
    await nextTick()
    expect(wrapper.find('.welcome-view').classes()).not.toContain('w-expanded')
    expect(wrapper.find('svg.w-iso').exists()).toBe(true)
    expect(wrapper.emitted('done')).toBeUndefined()
    // 收起后底部提示切回下一步引导
    expect(wrapper.find('.w-footer .w-note').text()).toBe('Enter / → 下一步 · ← 上一步')
  })

  it('展开转变：投影因子驱动等距→俯视连续插值，动画完成后顶面塌缩为轴对齐矩形、整图等比缩小 0.8 且不出 viewBox', async () => {
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await nextTick()
    // rAF 驱动 300ms 插值，轮询至收敛（f=1 纯俯视：u 轴水平、壁厚 0）——固定
    // sleep 在 CI 高负载下帧延迟超余量即偶发失败
    await vi.waitFor(() =>
      expect(wrapper.find('g.w-zoom').attributes('transform') ?? '').toMatch(/scale\(0\.8000\)/),
    )
    expect(wrapper.find('.welcome-view').classes()).toContain('w-expanded')
    // 图纸几何内容整体等比缩小：俯视终点 scale 0.8、平移补偿 (72, 23)（水平中心 360、
    /// 垂直中心 ZOOM_CY 115 上提让位权限面板；attribute transform 随 expandF 逐帧插值）
    const zoom = wrapper.find('g.w-zoom').attributes('transform') ?? ''
    expect(zoom).toMatch(/scale\(0\.8000\)/)
    expect(zoom).toMatch(/translate\(72\.00 23\.00\)/)
    // 图签在缩放层外：位置恒定不随缩小移动（展开时原地渐隐）
    expect(wrapper.find('text.w-title').attributes('x')).toBe('53')
    for (const p of wrapper.findAll('g.w-plate')) {
      const d = p.find('path.w-top').attributes('d') ?? ''
      // 四个角圆弧的 Q 控制点即四角：俯视矩形 x/y 各自仅两取值（等距菱形 y 有四取值）
      const ctrl = [...d.matchAll(/Q (-?[\d.]+) (-?[\d.]+)/g)].map((m) => [
        parseFloat(m[1]!),
        parseFloat(m[2]!),
      ])
      expect(ctrl).toHaveLength(4)
      expect(new Set(ctrl.map((c) => Math.round(c[0]! * 2))).size).toBe(2)
      expect(new Set(ctrl.map((c) => Math.round(c[1]! * 2))).size).toBe(2)
      // 单键俯视尺寸 = 等距视觉尺寸（视角转正不改键盘）：横宽 2a·u1 = 等距 u 边
      // 视觉长 65.8、纵高 2b·kv = 等距 v 边视觉长 59.7（Space 宽板跳过）
      const xs = ctrl.map((c) => c[0]!)
      const ys = ctrl.map((c) => c[1]!)
      const width = Math.max(...xs) - Math.min(...xs)
      const height = Math.max(...ys) - Math.min(...ys)
      if (width < 100) {
        expect(Math.abs(width - 63 * 1.0444)).toBeLessThan(1)
        expect(Math.abs(height - 35.06 * 1.7023)).toBeLessThan(1)
      }
    }
    // 俯视图整体仍在 viewBox 内（含壁厚 0 的网格/引出线随动）
    const nums = (s: string) => [...s.matchAll(/-?[\d.]+/g)].flatMap((m) => [parseFloat(m[0]!)])
    for (const path of wrapper.findAll('g.w-plate path, path.w-grid, g.w-callout path')) {
      const v = nums(path.attributes('d') ?? '')
      for (let i = 0; i < v.length; i += 2) {
        expect(v[i]).toBeGreaterThanOrEqual(-1)
        expect(v[i]).toBeLessThanOrEqual(721)
        expect(v[i + 1]).toBeGreaterThanOrEqual(-1)
        expect(v[i + 1]).toBeLessThanOrEqual(481)
      }
    }
    // 板阵水平居中（中心 ≈ viewBox 360，词块在右侧作视觉配重允许微偏）
    const plateXs = wrapper
      .findAll('g.w-plate path.w-top')
      .flatMap((p) => nums(p.attributes('d') ?? '').filter((_, i) => i % 2 === 0))
    const centerX = (Math.max(...plateXs) + Math.min(...plateXs)) / 2
    expect(Math.abs(centerX - 360)).toBeLessThan(20)
  })

  it('Escape 始终完结（任意页，焦点残留不拦截）', () => {
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(wrapper.emitted('done')).toHaveLength(1)
  })

  it('槽让位后的残留按键不完结：不落盘 onboarded、不 emit done（防未经确认永久跳过引导）', () => {
    // 槽为 null 模拟让位后的残留按键（守卫为组件契约，不依赖挂载方式）
    const wrapper = mountView()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(wrapper.emitted('done')).toBeUndefined()
    expect(useSettingsStore().onboarded).toBe(false)
  })

  it('权限面板：功能 ↔ 权限映射渲染，三项未授权可点直达系统设置，辅助功能先经系统弹窗请求再跳', async () => {
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    // 展开权限面板
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await nextTick()

    // 底部提示同一元素同一位置：展开态内容切换为收尾导航（面板无标题行，任何标题写法都不得出现）
    expect(
      wrapper
        .find(
          '.w-perm-panel h1, .w-perm-panel h2, .w-perm-panel h3, .w-perm-panel h4, .w-perm-panel h5, .w-perm-panel h6, .w-perm-panel [role="heading"]',
        )
        .exists(),
    ).toBe(false)
    expect(wrapper.find('.w-footer .w-note').text()).toBe('← 上一步 · Enter 开始使用')
    const rows = wrapper.findAll('.w-perm-row')
    expect(rows.map((r) => r.find('.w-perm-name').text())).toEqual([
      '屏幕录制',
      '辅助功能',
      '完全磁盘',
    ])
    expect(rows.map((r) => r.find('.w-perm-use').text())).toEqual([
      '截屏标注 · 窗口管理',
      '划词翻译 · 访达隐藏文件切换',
      '下载/图片等保存免弹窗',
    ])

    const buttons = wrapper.findAll('.w-perm-row button')
    expect(buttons.map((b) => b.text())).toEqual(['授权', '授权', '授权'])

    await buttons[0]!.trigger('click')
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('open_privacy_settings', {
      kind: 'screen_recording',
    })

    vi.mocked(invoke).mockClear()
    await buttons[1]!.trigger('click')
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(1, 'request_accessibility_permission')
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(2, 'open_privacy_settings', {
      kind: 'accessibility',
    })

    vi.mocked(invoke).mockClear()
    await buttons[2]!.trigger('click')
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('open_privacy_settings', {
      kind: 'full_disk_access',
    })
  })

  it('已授权项渲染为静态完成态（非按钮），状态经获焦刷新链路实时反映', async () => {
    useSystemStore().permScreenRecording = true
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await nextTick()
    expect(wrapper.findAll('.w-perm-row button')).toHaveLength(2)
    // 屏幕录制行转静默完成态（勾 + 已授权），名称仍在行首
    const firstRow = wrapper.findAll('.w-perm-row')[0]!
    expect(firstRow.find('.w-perm-name').text()).toBe('屏幕录制')
    expect(firstRow.find('.w-perm-done').exists()).toBe(true)

    // 剩余项授权完成后（系统设置返回 → 窗口获焦 → refresh）全部转静默完成态
    const systemStore = useSystemStore()
    systemStore.permAccessibility = true
    systemStore.permFullDiskAccess = true
    await nextTick()
    expect(wrapper.findAll('.w-perm-row button')).toHaveLength(0)
    expect(wrapper.findAll('.w-perm-done')).toHaveLength(3)
  })
})
