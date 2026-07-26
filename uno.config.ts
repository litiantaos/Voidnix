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
    colors: {
      surface: { DEFAULT: 'var(--color-surface)' },
      canvas: { DEFAULT: 'var(--color-canvas)' },
      // 字面 hex：Wind4 对纯 CSS 变量做 color-mix 时，text/bg-accent 在 WK 下不可靠；
      // 与 theme.css --color-accent 同值（改色两处同步）。实心底按钮优先 .ui-btn-primary。
      accent: { DEFAULT: '#3d82f0' },
      mist: {
        DEFAULT: 'var(--color-mist)',
        solid: 'var(--color-mist-solid)',
      },
      bubble: { DEFAULT: 'var(--color-bubble)' },
      primary: 'var(--color-text-primary)',
      secondary: 'var(--color-text-secondary)',
      muted: 'var(--color-text-muted)',
      // 语义色（业务优先用这些；文件类型图标等可用 palette 区分色，见 design.md）
      danger: {
        DEFAULT: 'var(--color-danger)',
        soft: 'var(--color-danger-soft)',
      },
      warning: {
        DEFAULT: 'var(--color-warning)',
        soft: 'var(--color-warning-soft)',
      },
      success: {
        DEFAULT: 'var(--color-success)',
        soft: 'var(--color-success-soft)',
      },
    },
    transitionTimingFunction: {
      out: 'cubic-bezier(0, 0, 0.2, 1)',
      in: 'cubic-bezier(0.4, 0, 1, 1)',
      spring: 'cubic-bezier(0.34, 1.56, 0.64, 1)',
    },
  },
  shortcuts: {
    'radius-ctrl': 'rounded-[var(--radius-ctrl)]',
    'radius-panel': 'rounded-[var(--radius-panel)]',
    'radius-window': 'rounded-[var(--radius-window)]',

    /*
     * 面：数值真相在 theme.css :root + 类规则。
     * soft-surface：Uno 展开 fill/border/blur（acrylic 组合用）；类名再挂 theme 时以 theme saturate 为准。
     * soft-chip / ui-active / mica-tint：仅占位扫描，完整面只在 theme.css。
     */
    // blur/saturate 与 theme --soft-surface-* 同值（40 / 1.35）
    'soft-surface':
      'border border-solid border-[var(--soft-surface-border)] bg-[var(--soft-surface-fill)] shadow-none backdrop-blur-[40px] backdrop-saturate-135',
    // 抬升卡：soft-surface + radius-panel；阴影见 theme .soft-card
    'soft-card': 'soft-surface radius-panel',
    // 窗壳：纯白磨砂在 theme.css .mica-tint / .mica-shell
    'soft-chip': 'select-none',
    'ui-active': 'border-0 shadow-none',

    'mica-tint': 'shadow-none',
    // box-shadow 由 theme.css .mica-shell 管（var(--mica-ring-shadow)，浅/深双轨）
    'mica-shell': 'radius-window overflow-hidden',
    'mica-panel': 'soft-surface radius-panel select-none',
    'mica-bar': 'soft-surface radius-panel',

    // 搜索栏 / 浮层 = soft-surface + elevation 档
    'acrylic-bar': 'soft-surface radius-panel !shadow-[var(--shadow-bar)]',
    'acrylic-panel': 'soft-surface radius-panel select-none !shadow-[var(--shadow-panel)]',
    'dropdown-panel': 'acrylic-panel p-1',

    'fill-ctrl': 'bg-[var(--color-fill-4)]',
    'fill-hover': 'bg-[var(--color-fill-5)]',
    'fill-active': 'bg-[var(--color-fill-8)]',
    'fill-strong': 'bg-[var(--color-fill-12)]',
    'fill-mist': 'bg-mist',
    'border-divider': 'border-[var(--color-divider)]',
    'border-soft': 'border-[var(--color-border)]',

    // ui-ctrl：壳（尺寸/字号）；面由 soft-chip 或 soft-surface / ui-field 另挂
    'ui-ctrl':
      'h-7 px-3 radius-ctrl outline-none text-xs font-medium text-primary transition-[background-color,box-shadow,border-color] duration-150 ease-out select-none',
    // 大输入：soft-surface + theme .ui-field 描边/聚焦
    'ui-field':
      'radius-ctrl outline-none text-sm font-medium soft-surface text-primary select-none',
    'ui-disabled': 'cursor-not-allowed',

    'flex-center': 'flex items-center justify-center',
    'flex-col-full': 'flex flex-col',
    'flex-col-full-pb': 'pb-3 flex flex-col',

    'form-label': 'text-xs text-muted font-medium',
    'form-field': 'flex flex-col gap-1.5',
    'input-base': 'outline-none bg-transparent flex-1 min-w-0',

    // group-header：min-h-7 锚定控件高度，有无操作项行高一致（flex/gap-2 由容器兜底，slot 直平铺）
    'group-header': 'text-xs text-muted font-medium px-3 min-h-7 flex items-center gap-2',
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
