import { defineConfig, presetAttributify, presetIcons, presetWind4 } from 'unocss'

export default defineConfig({
  presets: [
    presetWind4(),
    presetAttributify(),
    presetIcons({
      warn: false,
      collections: {
        ri: () => import('@iconify-json/ri/icons.json').then((i) => i.default),
      },
    }),
  ],
  theme: {
    // ── 语义色阶（Voidnix 设计系统，单一真相源在 docs/design.md）──
    colors: {
      surface: {
        DEFAULT: '#fafafa',
      },
      accent: {
        DEFAULT: '#3b82f6',
      },
      primary: 'rgba(0, 0, 0, 0.89)',
      secondary: 'rgba(0, 0, 0, 0.60)',
      muted: 'rgba(0, 0, 0, 0.40)',
    },

    // ── 动画 easing（全仓单一源；duration 用内置数值 duration-100/150/200）──
    transitionTimingFunction: {
      out: 'cubic-bezier(0, 0, 0.2, 1)', // 进场 / hover
      in: 'cubic-bezier(0.4, 0, 1, 1)', // 离场
      spring: 'cubic-bezier(0.34, 1.56, 0.64, 1)', // 弹簧回弹
    },
  },
  shortcuts: {
    // ── 圆角语义（值源 theme.css --radius-*；Wind4 用 var() 任意值）──
    // panel 10 = 外框（搜索栏 / 列表选中 / 浮层）；ctrl 6 = 框内嵌元素（标签 / 图标井 / 按钮）
    'radius-ctrl': 'rounded-[var(--radius-ctrl)]',
    'radius-panel': 'rounded-[var(--radius-panel)]',
    'radius-window': 'rounded-[var(--radius-window)]',

    // ── 材质：Mica 窗口壳（叠在原生 NSVisualEffect 之上的前端白染 + 高光环）──
    // 白染 40%：/60 过粉墙；/30 花壁纸易脏。窗级与截屏浮层分轨，勿绑同一 opacity
    'mica-tint': 'bg-white/40',
    'mica-ring':
      'shadow-[inset_0_2px_0_0_rgba(255,255,255,0.7),inset_0_0_0_1px_rgba(255,255,255,0.35)]',
    'mica-shell': 'mica-tint mica-ring radius-window overflow-hidden',
    // 叠在花图上的浮层（截屏工具条等）：独立高白染，与窗级 mica-tint 解耦
    'mica-panel':
      'bg-white/90 backdrop-blur-xl glass-ring radius-panel border border-black/10 select-none',
    'mica-bar': 'bg-white/90 backdrop-blur-xl glass-ring radius-panel border border-black/10',

    // ── 材质：Acrylic 仅外框（主窗搜索栏 / 下拉，叠在已有 Mica 上）；内嵌禁止再叠磨砂 ──
    // 白底 45%：叠在窗级 mica 上仍有层次与透感
    acrylic: 'bg-white/45 backdrop-blur-2xl backdrop-saturate-125',
    'glass-ring': 'shadow-[inset_0_2px_0_0_rgba(255,255,255,0.65)]',
    'acrylic-bar': 'acrylic glass-ring radius-panel border border-black/10',
    'acrylic-panel': 'acrylic glass-ring radius-panel border border-black/10 select-none',
    'dropdown-panel': 'acrylic-panel p-1',

    // ── 内嵌实色填充（标签 / 图标井 / 按钮 / 选中；无 blur，保证叠在玻璃上仍可读）──
    'fill-ctrl': 'bg-black/4',
    'fill-hover': 'bg-black/5',
    'fill-active': 'bg-black/8',

    // ── 控件状态 ──
    'ui-ctrl':
      'h-7 px-3 radius-ctrl outline-none border-none text-xs font-medium fill-ctrl text-primary transition-colors duration-150 ease-out focus-within:ring-1 focus-within:ring-inset focus-within:ring-accent/40 select-none',
    'ui-disabled': 'opacity-50 cursor-not-allowed',
    // 选中 / 焦点行：浅灰实色（black/5，比 fill-active 的 /8 更轻）
    'ui-active': 'fill-hover',

    // ── 布局 ──
    'flex-center': 'flex items-center justify-center',
    // 模块 View 根布局惯例（撑满由 ContentView :deep flex-1 统一注入，无需自带 h-full）
    'flex-col-full': 'flex flex-col',
    'flex-col-full-pb': 'pb-3 flex flex-col',

    // ── 表单 ──
    'form-label': 'text-xs text-muted font-medium',
    'form-field': 'flex flex-col gap-1.5',
    'input-base': 'outline-none bg-transparent flex-1 min-w-0',

    // ── 杂项 ──
    'group-header': 'text-xs text-muted tracking-wider font-medium px-3 py-1.5 uppercase',
    'overlay-abs': 'pointer-events-none absolute',
  },
  content: {
    pipeline: {
      include: [
        /\.(vue|svelte|[jt]sx|mdx?|astro|elm|php|phtml|html)($|\?)/,
        'src/**/*.ts',
        'extensions/**/*.ts',
      ],
    },
  },
})
