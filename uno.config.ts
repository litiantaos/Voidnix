import {
  defineConfig,
  presetAttributify,
  presetIcons,
  presetWind4,
} from 'unocss'

export default defineConfig({
  presets: [
    presetWind4(),
    presetAttributify(),
    presetIcons({
      warn: true,
      collections: {
        ri: () => import('@iconify-json/ri/icons.json').then((i) => i.default),
      },
    }),
  ],
  theme: {
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
        disabled: 'rgba(0, 0, 0, 0.25)',
      },
    },
  },
  shortcuts: {
    'ui-ctrl':
      'h-7 px-3 rounded-md outline-none border-none text-xs font-medium bg-black/4 text-tx-primary transition-all focus-within:ring-1 focus-within:ring-inset focus-within:ring-accent/40 select-none',
    'ui-disabled': 'opacity-50 cursor-not-allowed',
    'ui-hover': 'hover:bg-black/4',
    'ui-active': 'bg-accent/10',
  },
  content: {
    pipeline: {
      include: [
        /\.(vue|svelte|[jt]sx|mdx?|astro|elm|php|phtml|html)($|\?)/,
        'src/**/*.ts',
      ],
    },
  },
})
