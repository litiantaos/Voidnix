import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))

import WelcomeView from './WelcomeView.vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
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

  it('键位图（纯信息展示）：主快捷键板 + ⌘ 占位弱化板 + 五颗功能键板 + 等距网格 + 全引出线标注 + 左下角图签', () => {
    const wrapper = mountView()
    const text = wrapper.text()
    // 图纸：两块主快捷键板（修饰键/Space）+ ⌘ 占位板 + 五块功能键板 + 「/」语法键；底层横向网格线
    expect(wrapper.findAll('g.w-plate').length).toBe(9)
    // 网格线：贴键底缘的横向线族（三排键上下底缘 6 层，沿 u 轴；板占位内断开成多段）
    expect(wrapper.findAll('path.w-grid').length).toBeGreaterThanOrEqual(6)
    expect(wrapper.find('g.w-plate-ghost').text()).toContain('⌘')
    expect(wrapper.find('svg.w-iso').attributes('aria-label')).toBe('唤起窗口 ⌥ + Space')
    // 标注全部经引出线：修饰键/唤起窗口 + 五个功能词 + 「/」语法键，共 8 组
    expect(wrapper.findAll('g.w-callout').length).toBe(8)
    expect(text).toContain('修饰键')
    expect(text).toContain('唤起窗口')
    expect(text).toContain('Agent')
    expect(text).toContain('截屏')
    expect(text).toContain('访达')
    expect(text).toContain('剪贴板')
    expect(text).toContain('记事本')
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
    // 标题底部与图纸按键底部对齐：口号底缘（基线 + 中文下降 ≈2）≈ 板阵最低点
    const plateBottom = Math.max(
      ...wrapper
        .findAll('g.w-plate path')
        .flatMap((p) =>
          [...(p.attributes('d') ?? '').matchAll(/(-?[\d.]+) (-?[\d.]+)/g)].map((m) =>
            parseFloat(m[2]!),
          ),
        ),
    )
    expect(Math.abs(parseFloat(tagline.attributes('y')!) + 2 - plateBottom)).toBeLessThan(3)
    expect(wrapper.findAll('button').length).toBe(0)
  })

  it('en 口号不设 textLength（词形自然宽，防同宽挤压字距）；图签标题恒设', () => {
    locale.value = 'en'
    const wrapper = mountView()
    expect(wrapper.find('text.w-tagline').attributes('textLength')).toBeUndefined()
    expect(wrapper.find('text.w-title').attributes('textLength')).toBe('96')
    // 「/」语法键引出线同语言切换（双行词块，含 // 搜索文案）
    expect(wrapper.text()).toContain('Type / to show extensions')
    expect(wrapper.text()).toContain('Type // for quick search')
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
    for (const p of wrapper.findAll('path.w-grid')) check(nums(p.attributes('d') ?? ''), 'grid')
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
    expect(groups.length).toBe(8)
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
      expect(tx.attributes('text-anchor') ?? 'start').toBe(goesRight ? 'end' : 'start')
      expect(parseFloat(tx.attributes('x')!)).toBeCloseTo(
        goesRight ? Math.max(x2, x3) : Math.min(x2, x3),
      )
      expect(parseFloat(tx.attributes('y')!)).toBeCloseTo(y2 - 8)
      // 「/」语法键双行词块：首行同锚右对齐、整体上移一行（行距 13）
      if (texts.length === 2) {
        const first = texts[0]!
        expect(first.attributes('text-anchor')).toBe(tx.attributes('text-anchor'))
        expect(parseFloat(first.attributes('x')!)).toBeCloseTo(parseFloat(tx.attributes('x')!))
        expect(parseFloat(first.attributes('y')!)).toBeCloseTo(y2 - 8 - 13)
      }
      // Space 组：斜段下行（肘点为全线最低点），转折开口向上；N 组：斜段自顶部上行
      if (g.text().includes('唤起窗口')) {
        expect(y2).toBeGreaterThan(y1)
        expect(y2).toBeGreaterThanOrEqual(Math.max(y1, y3))
      }
      if (g.text().includes('记事本')) {
        expect(y2).toBeLessThan(y1)
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
    // 板 (col, row, u 半宽) 布局表
    const plates: Array<[number, number, number]> = [
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
    const segs = wrapper.findAll('path.w-grid')
    expect(segs.length).toBeGreaterThan(0)
    for (const seg of segs) {
      const m = (seg.attributes('d') ?? '').match(
        /^M (-?[\d.]+) (-?[\d.]+) L (-?[\d.]+) (-?[\d.]+)$/,
      )
      expect(m, `网格段无法解析: ${seg.attributes('d')}`).not.toBeNull()
      const x1 = parseFloat(m![1]!)
      const y1 = parseFloat(m![2]!)
      const x2 = parseFloat(m![3]!)
      const y2 = parseFloat(m![4]!)
      for (const [px, py] of [
        [x1, y1],
        [x2, y2],
        [(x1 + x2) / 2, (y1 + y2) / 2],
      ]) {
        expect(
          insidePlate(px, py),
          `网格段 ${seg.attributes('d')} 采样点 (${px},${py}) 深入板内`,
        ).toBe(false)
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
    // 双修饰占满修饰槽，无弱化占位板
    expect(wrapper.find('g.w-plate-ghost').exists()).toBe(false)
    expect(wrapper.find('svg.w-iso').attributes('aria-label')).toBe('唤起窗口 ⌃ + ⌥ + Space')
  })

  it('单键主快捷键：修饰槽双弱化占位（⌥⌘ ghost），单键落宽板，修饰键标注不渲染', () => {
    useSettingsStore().globalShortcut = 'F1'
    const wrapper = mountView()
    const ghosts = wrapper.findAll('g.w-plate-ghost').map((g) => g.text())
    expect(ghosts.sort()).toEqual(['⌘', '⌥'])
    // 标注仅剩 唤起窗口 + 五个功能词 + 「/」语法键（无修饰键可标）
    expect(wrapper.findAll('g.w-callout').length).toBe(7)
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
    // dev 注册叠加的 ⇧ 不挤占键盘布局示意：⌥ main + ⌘ 弱化占位 + Space（「/」非快捷键不受影响）
    const symbols = wrapper.findAll('text.w-cap').map((c) => c.text())
    expect(symbols).toContain('⌥')
    expect(symbols).toContain('⌘')
    expect(symbols).toContain('Space')
    expect(symbols).toContain('/')
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
    // 其余四颗功能键板照常（+「/」语法键不受注册表影响）
    for (const s of ['A', 'S', 'F', 'C']) expect(symbols).toContain(s)
    expect(wrapper.findAll('g.w-plate').length).toBe(8)
  })

  it('Enter / Escape 键盘路径 emit done（组件自治按键，一次性）并落盘 onboarded', () => {
    // 生产中组件仅在 fullscreen 槽激活期间挂载（requestDone 据槽判定完结有效性）
    useAppStore().setFullscreenView(WelcomeView)
    const wrapper = mountView()
    const settings = useSettingsStore()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(wrapper.emitted('done')).toHaveLength(1)
    // 完结落盘归供给方自持
    expect(settings.onboarded).toBe(true)
    // 完结一次性：done 之后的重复按键不再 emit
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(wrapper.emitted('done')).toHaveLength(1)
  })

  it('Escape 始终完结（焦点残留不拦截）', () => {
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
})
