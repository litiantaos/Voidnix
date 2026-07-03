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
    // ── 语义色阶 ──
    colors: {
      surface: {
        DEFAULT: '#fcfcfc',
      },
      accent: {
        DEFAULT: '#3b82f6',
      },
      tx: {
        primary: 'rgba(0, 0, 0, 0.85)',
        secondary: 'rgba(0, 0, 0, 0.70)',
        subtle: 'rgba(0, 0, 0, 0.50)',
        muted: 'rgba(0, 0, 0, 0.40)',
        hint: 'rgba(0, 0, 0, 0.35)',
        faint: 'rgba(0, 0, 0, 0.30)',
      },
    },

    // ── 动画 easing（全仓单一源；duration 用内置数值 duration-100/150/200）──
    transitionTimingFunction: {
      out: 'cubic-bezier(0, 0, 0.2, 1)', // 进场 / hover
      in: 'cubic-bezier(0.4, 0, 1, 1)', // 离场
      spring: 'cubic-bezier(0.34, 1.56, 0.64, 1)', // 弹簧回弹
    },
  },
  shortcuts: {
    // ── 控件状态 ──
    'ui-ctrl':
      'h-7 px-3 rounded-md outline-none border-none text-xs font-medium bg-black/4 text-tx-primary transition-colors duration-150 ease-out focus-within:ring-1 focus-within:ring-inset focus-within:ring-accent/40 select-none',
    'ui-disabled': 'opacity-50 cursor-not-allowed',
    'ui-active': 'bg-black/5',

    // ── 布局 ──
    'flex-center': 'flex items-center justify-center',
    'flex-col-full': 'flex flex-col h-full',
    'flex-col-full-pb': 'pb-4 flex flex-col h-full',

    // ── 表单 ──
    'form-label': 'text-xs text-tx-faint font-medium',
    'form-field': 'flex flex-col gap-1.5',
    'input-base': 'outline-none bg-transparent flex-1 min-w-0',

    // ── 杂项 ──
    'group-header': 'text-xs text-tx-faint tracking-wider font-medium px-3 py-1.5 uppercase',
    'overlay-abs': 'pointer-events-none absolute',
    'dropdown-panel': 'p-1 rounded-lg bg-white shadow-lg select-none',
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
