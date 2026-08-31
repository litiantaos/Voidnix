// 落地页双语字典。zh 为类型源，en 必须同构（TS 编译期校验完整性）。
// 用法：const t = getDict(lang) → t.hero.eyebrow

export type Lang = 'zh' | 'en'

// ── 页面级文案 ──
const zh = {
  layout: {
    title: 'Voidnix - macOS 效率启动器',
    description: '模块化的 macOS 效率启动器。Rust + 原生原语，24 个一等公民扩展，常驻低占用。',
    htmlLang: 'zh-CN',
    ogLocale: 'zh_CN',
    demoTitle: 'Voidnix Demo',
    backToTop: '回到顶部',
    langLabel: 'EN',
    langHref: '/en/',
  },
  hero: {
    titleLead: '想到就到，',
    titleAccent: '触手可及',
    slogan: '极速、轻量的效率启动器。按下快捷键，搜索、翻译、截屏、剪贴板，随叫随到。',
    download: '下载',
    meta: 'Apple Silicon · macOS 13+',
  },
  philosophy: {
    eyebrow: '设计哲学',
    title: '结构清晰，性能克制',
    lead: '复杂藏在结构里，不在界面上。',
    items: [
      {
        icon: 'ri-stack-line',
        title: '两层正交',
        desc: 'runtime 调度核心与 platform 原语分离，换实现即跨平台，业务零泄漏。',
      },
      {
        icon: 'ri-flashlight-line',
        title: 'Rust + 原语',
        desc: '常驻近乎零负担，LaunchAgent 长期采样背书其稳定与克制。',
      },
      {
        icon: 'ri-leaf-line',
        title: 'Mica 玻璃',
        desc: '仅浅色，克制圆角与中性墨阴影。',
      },
    ],
  },
  capabilities: {
    eyebrow: '核心能力',
    title: '六块拼图',
    lead: '默认配齐，按需开关。',
    items: [
      {
        id: 'search',
        icon: 'ri-search-line',
        title: '全局搜索',
        tag: 'Option + Space',
        desc: '一次按键召回一切，快结果不等慢结果。',
        bullets: ['流式增量召回', '拼音 / 全拼 / 缩写', '即时答案：计算 / 汇率 / 时间戳'],
      },
      {
        id: 'clipboard',
        icon: 'ri-clipboard-line',
        title: '剪贴板',
        tag: 'Option + C',
        desc: '复制过的都在，搜索即达，回车即粘贴。',
        bullets: ['文本 / 图片 / 文件全记录', '搜索 · 收藏 · 编辑', '多选批量粘贴'],
      },
      {
        id: 'agent',
        icon: 'ri-robot-2-line',
        title: 'AI Agent',
        tag: 'Option + A',
        desc: '对话即操作，过程全程可见。',
        bullets: ['工具调用循环', '联网搜索 + 执行命令', '统一 Key 中枢'],
      },
      {
        id: 'screenshot',
        icon: 'ri-screenshot-line',
        title: '截屏',
        tag: 'Option + S',
        desc: '截屏即标注、识别、钉图，存盘路径可配。',
        bullets: ['标注 / OCR / 二维码', '钉图常驻', '滚动长截图'],
      },
      {
        id: 'window',
        icon: 'ri-layout-grid-line',
        title: '窗口管理',
        tag: '鼠标至屏顶',
        desc: '鼠标移至屏顶中心唤起面板，多屏下布局相对当前屏计算。',
        bullets: ['顶部分屏面板', '自定义尺寸', '跨屏迁移'],
      },
      {
        id: 'terminal',
        icon: 'ri-terminal-box-line',
        title: '终端建议',
        tag: 'zsh',
        desc: '输入即建议，→ 接受。',
        bullets: ['frecency 智能补全', '→ 接受 / Tab 切换', '零侵入 zsh'],
      },
    ],
    // capabilities 微场景文案（search 场景的分组标题、agent 场景的对话等）
    scenes: {
      groupFile: '文件',
      agentUser: '列出 extensions 目录里的 ts 文件',
      agentToolOut: '24 个 index.ts，含 native 的 16 个',
      agentAction: '复制路径',
      snapLabel: '右半屏',
    },
  },
  extensions: {
    eyebrow: '扩展矩阵',
    title: '{n} 个扩展，统一架构',
    lead: '都是一等公民，声明即用。',
    clusters: [
      {
        title: '搜索与计算',
        items: [
          {
            id: 'search',
            name: '全局搜索',
            desc: '应用 / 文件 / 扩展 / 剪贴板，流式增量召回',
            icon: 'ri-search-line',
          },
          {
            id: 'calculator',
            name: '计算器',
            desc: '输入即算，回车复制',
            icon: 'ri-calculator-line',
          },
          {
            id: 'currency',
            name: '汇率',
            desc: '金额与币种即时换算',
            icon: 'ri-exchange-cny-line',
          },
          { id: 'time', name: '时间戳', desc: 'Unix 与日期互转', icon: 'ri-time-line' },
          { id: 'ip', name: 'IP 信息', desc: '空查本机，输入查归属', icon: 'ri-global-line' },
          { id: 'uuid', name: 'UUID', desc: 'UUID v4 / NanoID 生成', icon: 'ri-fingerprint-line' },
          { id: 'base64', name: 'Base64', desc: '文本编解码', icon: 'ri-code-s-slash-line' },
        ],
      },
      {
        title: 'AI 智能',
        items: [
          {
            id: 'agent',
            name: 'AI Agent',
            desc: '对话与工具调用，联网搜索与执行命令，会话自动留存',
            icon: 'ri-robot-2-line',
          },
          {
            id: 'translate',
            name: '翻译',
            desc: '选中文本即译，中英方向自动反转，可朗读',
            icon: 'ri-translate-2',
          },
          {
            id: 'ai-providers',
            name: 'AI 提供商',
            desc: '统一管理 Key，供应用与外部工具共用',
            icon: 'ri-key-2-line',
          },
        ],
      },
      {
        title: '捕获与媒体',
        items: [
          {
            id: 'clipboard',
            name: '剪贴板',
            desc: '后台记录文本 / 图片 / 文件，搜索、收藏与编辑',
            icon: 'ri-clipboard-line',
          },
          {
            id: 'screenshot',
            name: '截屏',
            desc: '全屏 / 标注 / OCR / 二维码 / 钉图 / 滚动长截图 / 取色',
            icon: 'ri-screenshot-line',
          },
          {
            id: 'video',
            name: '视频处理',
            desc: '压缩 / 转格式 / 抽音频，按需下载核心',
            icon: 'ri-video-line',
          },
          {
            id: 'image',
            name: '图片处理',
            desc: '移除背景 / 拼接长图，原生 Vision 分割',
            icon: 'ri-image-edit-line',
          },
        ],
      },
      {
        title: '系统与效率',
        items: [
          {
            id: 'window-manager',
            name: '窗口管理',
            desc: '顶部分屏面板，自定义尺寸与跨屏迁移',
            icon: 'ri-layout-grid-line',
          },
          {
            id: 'finder-ext',
            name: '访达工具',
            desc: '拷贝路径 / 终端打开 / 新建文件 / 隐藏文件',
            icon: 'ri-folder-add-line',
          },
          {
            id: 'notes',
            name: '记事本',
            desc: '随手记录，逐字动效，自动暂存',
            icon: 'ri-sticky-note-line',
          },
          {
            id: 'system-status',
            name: '系统状态',
            desc: 'CPU / 内存 / 磁盘 / 网络概览',
            icon: 'ri-pulse-line',
          },
          {
            id: 'awake',
            name: '保持唤醒',
            desc: '接入电源时合盖熄屏不休眠',
            icon: 'ri-macbook-line',
          },
          {
            id: 'clean-mode',
            name: '清洁模式',
            desc: '全屏黑屏锁定键鼠，长按退出',
            icon: 'ri-contrast-2-fill',
          },
          {
            id: 'zsh-autosuggestions',
            name: '终端建议',
            desc: 'zsh 历史 frecency 智能补全',
            icon: 'ri-terminal-box-line',
          },
          { id: 'homebrew', name: 'Homebrew', desc: '包管理与一键更新升级', icon: 'ri-cup-fill' },
          {
            id: 'proxy',
            name: '代理',
            desc: 'TUN 模式，节点切换与测速',
            icon: 'ri-signal-tower-line',
          },
          {
            id: 'settings',
            name: '设置',
            desc: '外观 / 快捷键 / 自启 / 权限 / 更新',
            icon: 'ri-settings-3-line',
          },
        ],
      },
    ],
  },
}

type Dict = typeof zh

const en: Dict = {
  layout: {
    title: 'Voidnix - macOS Productivity Launcher',
    description:
      'A modular macOS productivity launcher. Rust + native primitives, 24 first-class extensions, minimal footprint.',
    htmlLang: 'en',
    ogLocale: 'en_US',
    demoTitle: 'Voidnix Demo',
    backToTop: 'Back to top',
    langLabel: '中文',
    langHref: '/',
  },
  hero: {
    titleLead: 'From thought',
    titleAccent: 'to action.',
    slogan:
      'A blazing-fast, lightweight productivity launcher. Search, translate, screenshot, clipboard — one shortcut away.',
    download: 'Download',
    meta: 'Apple Silicon · macOS 13+',
  },
  philosophy: {
    eyebrow: 'Design Philosophy',
    title: 'Clear Structure, Restrained Performance',
    lead: 'Complexity lives in the architecture, not the interface.',
    items: [
      {
        icon: 'ri-stack-line',
        title: 'Two-Layer Orthogonal',
        desc: 'Runtime scheduling core and platform primitives are cleanly separated — swap the platform layer to go cross-platform, with zero business logic leakage.',
      },
      {
        icon: 'ri-flashlight-line',
        title: 'Rust + Primitives',
        desc: 'Near-zero idle footprint. Long-term LaunchAgent sampling backs its stability and restraint.',
      },
      {
        icon: 'ri-leaf-line',
        title: 'Mica Glass',
        desc: 'Light-only, restrained radii and neutral ink shadows.',
      },
    ],
  },
  capabilities: {
    eyebrow: 'Core Capabilities',
    title: 'Six Pieces',
    lead: 'Equipped by default, toggle as needed.',
    items: [
      {
        id: 'search',
        icon: 'ri-search-line',
        title: 'Global Search',
        tag: 'Option + Space',
        desc: 'One keystroke summons everything. Fast results never wait for slow ones.',
        bullets: [
          'Streaming incremental recall',
          'Pinyin / full / abbreviation',
          'Instant answers: math / currency / time',
        ],
      },
      {
        id: 'clipboard',
        icon: 'ri-clipboard-line',
        title: 'Clipboard',
        tag: 'Option + C',
        desc: 'Everything you copied, one search away. Enter to paste.',
        bullets: [
          'Text / image / file history',
          'Search · favorites · edit',
          'Multi-select batch paste',
        ],
      },
      {
        id: 'agent',
        icon: 'ri-robot-2-line',
        title: 'AI Agent',
        tag: 'Option + A',
        desc: 'Conversational actions with full process visibility.',
        bullets: ['Tool-calling loop', 'Web search + command execution', 'Unified key hub'],
      },
      {
        id: 'screenshot',
        icon: 'ri-screenshot-line',
        title: 'Screenshot',
        tag: 'Option + S',
        desc: 'Capture, annotate, OCR, pin — configurable save path.',
        bullets: ['Annotate / OCR / QR code', 'Pin to screen', 'Scrolling capture'],
      },
      {
        id: 'window',
        icon: 'ri-layout-grid-line',
        title: 'Window Management',
        tag: 'Cursor to top',
        desc: 'Move cursor to top-center to summon the panel. Multi-monitor layouts are relative to the current screen.',
        bullets: ['Top snap panel', 'Custom sizes', 'Cross-screen migration'],
      },
      {
        id: 'terminal',
        icon: 'ri-terminal-box-line',
        title: 'Terminal Suggestions',
        tag: 'zsh',
        desc: 'Suggestions as you type. Press → to accept.',
        bullets: ['Frecency smart completion', '→ accept / Tab cycle', 'Zero-intrusion zsh'],
      },
    ],
    scenes: {
      groupFile: 'Files',
      agentUser: 'List the .ts files in the extensions directory',
      agentToolOut: '24 index.ts files, 16 with native code',
      agentAction: 'Copy path',
      snapLabel: 'Right half',
    },
  },
  extensions: {
    eyebrow: 'Extension Matrix',
    title: '{n} extensions, unified architecture',
    lead: 'All first-class citizens. Declare and use.',
    clusters: [
      {
        title: 'Search & Compute',
        items: [
          {
            id: 'search',
            name: 'Global Search',
            desc: 'Apps / files / extensions / clipboard — streaming incremental recall',
            icon: 'ri-search-line',
          },
          {
            id: 'calculator',
            name: 'Calculator',
            desc: 'Type to calculate, Enter to copy',
            icon: 'ri-calculator-line',
          },
          {
            id: 'currency',
            name: 'Currency',
            desc: 'Instant amount and currency conversion',
            icon: 'ri-exchange-cny-line',
          },
          { id: 'time', name: 'Timestamp', desc: 'Unix ↔ date conversion', icon: 'ri-time-line' },
          {
            id: 'ip',
            name: 'IP Info',
            desc: 'Empty query for local, input for geolocation',
            icon: 'ri-global-line',
          },
          {
            id: 'uuid',
            name: 'UUID',
            desc: 'UUID v4 / NanoID generator',
            icon: 'ri-fingerprint-line',
          },
          {
            id: 'base64',
            name: 'Base64',
            desc: 'Text encode / decode',
            icon: 'ri-code-s-slash-line',
          },
        ],
      },
      {
        title: 'AI',
        items: [
          {
            id: 'agent',
            name: 'AI Agent',
            desc: 'Conversational tool calling, web search & commands, persistent sessions',
            icon: 'ri-robot-2-line',
          },
          {
            id: 'translate',
            name: 'Translate',
            desc: 'Select text to translate, auto-detect direction, speak',
            icon: 'ri-translate-2',
          },
          {
            id: 'ai-providers',
            name: 'AI Providers',
            desc: 'Unified key management for apps & external tools',
            icon: 'ri-key-2-line',
          },
        ],
      },
      {
        title: 'Capture & Media',
        items: [
          {
            id: 'clipboard',
            name: 'Clipboard',
            desc: 'Background text / image / file recording, search, favorites & edit',
            icon: 'ri-clipboard-line',
          },
          {
            id: 'screenshot',
            name: 'Screenshot',
            desc: 'Fullscreen / annotate / OCR / QR code / pin / scrolling capture / pick color',
            icon: 'ri-screenshot-line',
          },
          {
            id: 'video',
            name: 'Video',
            desc: 'Compress / convert / extract audio, on-demand core download',
            icon: 'ri-video-line',
          },
          {
            id: 'image',
            name: 'Image',
            desc: 'Background removal / stitch panoramas, native Vision segmentation',
            icon: 'ri-image-edit-line',
          },
        ],
      },
      {
        title: 'System & Efficiency',
        items: [
          {
            id: 'window-manager',
            name: 'Window Manager',
            desc: 'Top snap panel, custom sizes & cross-screen migration',
            icon: 'ri-layout-grid-line',
          },
          {
            id: 'finder-ext',
            name: 'Finder Tools',
            desc: 'Copy path / open terminal / new file / toggle hidden',
            icon: 'ri-folder-add-line',
          },
          {
            id: 'notes',
            name: 'Notes',
            desc: 'Quick notes, per-character motion, auto-saved',
            icon: 'ri-sticky-note-line',
          },
          {
            id: 'system-status',
            name: 'System Status',
            desc: 'CPU / memory / disk / network overview',
            icon: 'ri-pulse-line',
          },
          {
            id: 'awake',
            name: 'Keep Awake',
            desc: 'Sleep display without system sleep when plugged in',
            icon: 'ri-macbook-line',
          },
          {
            id: 'clean-mode',
            name: 'Clean Mode',
            desc: 'Full-screen blackout, lock input, long-press to exit',
            icon: 'ri-contrast-2-fill',
          },
          {
            id: 'zsh-autosuggestions',
            name: 'Terminal Suggestions',
            desc: 'zsh history frecency smart completion',
            icon: 'ri-terminal-box-line',
          },
          {
            id: 'homebrew',
            name: 'Homebrew',
            desc: 'Package management & one-click update',
            icon: 'ri-cup-fill',
          },
          {
            id: 'proxy',
            name: 'Proxy',
            desc: 'TUN mode, node switching & speed test',
            icon: 'ri-signal-tower-line',
          },
          {
            id: 'settings',
            name: 'Settings',
            desc: 'Appearance / shortcuts / autostart / permissions / updates',
            icon: 'ri-settings-3-line',
          },
        ],
      },
    ],
  },
}

const dicts: Record<Lang, Dict> = { zh, en }

export function getDict(lang: Lang): Dict {
  return dicts[lang]
}

export function getLangFromUrl(url: URL): Lang {
  return url.pathname.startsWith('/en') ? 'en' : 'zh'
}

export const totalExtensions = zh.extensions.clusters.reduce((n, c) => n + c.items.length, 0)
