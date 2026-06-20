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
    fontSize: {
      '2xs': '10px',
    },
    // H10：消除 arbitrary 值——为 ResultIcon 的 115% 与 BaseSelect 的 80vw 提供命名档位
    size: {
      '115pct': '115%',
    },
    maxWidth: {
      '115pct': '115%',
      '80vw': '80vw',
    },
    // 动画 timing token（统一性审查 P0-3）：全仓 duration / easing 单一源
    // 引用：duration-fast → 100ms、duration-normal → 150ms、duration-slow → 200ms
    //      ease-out / ease-in / ease-spring（弹簧回弹）
    transitionDuration: {
      fast: '100ms',
      normal: '150ms',
      slow: '200ms',
    },
    transitionTimingFunction: {
      out: 'cubic-bezier(0, 0, 0.2, 1)', // ease-out（进场 / hover）
      in: 'cubic-bezier(0.4, 0, 1, 1)', // ease-in（离场）
      spring: 'cubic-bezier(0.34, 1.56, 0.64, 1)', // 弹簧回弹
    },
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
  },
  shortcuts: {
    // 统一性审查 P0-5：transition-all → transition-colors（控件只有 bg/text 变化，避免不必要的全属性过渡）
    'ui-ctrl':
      'h-7 px-3 rounded-md outline-none border-none text-xs font-medium bg-black/4 text-tx-primary transition-colors duration-fast ease-out focus-within:ring-1 focus-within:ring-inset focus-within:ring-accent/40 select-none',
    'ui-disabled': 'opacity-50 cursor-not-allowed',
    'ui-active': 'bg-black/5',
    'ui-hover': 'hover:bg-black/3',
    'flex-center': 'flex items-center justify-center',
    'flex-col-full': 'flex flex-col h-full',
    'flex-col-full-pb': 'pb-4 flex flex-col h-full',
    'form-label': 'text-xs text-tx-faint font-medium',
    'input-base': 'outline-none bg-transparent flex-1 min-w-0',
    'action-footer': 'pt-3 border-t border-tx-faint/20 flex gap-2',
    'form-field': 'flex flex-col gap-1.5',
    'group-header': 'text-xs text-tx-faint tracking-wider font-medium px-3 py-1.5 uppercase',
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
