// 22 扩展元数据 + 领域分簇。图标走 Remix Icon（ri-* class）。
// 数据源：各扩展 index.ts 的 meta 字段，描述按官网语境精简。

export interface ExtItem {
  id: string
  name: string
  desc: string
  icon: string // ri-* class（不含前缀 i-）
}

export interface ExtCluster {
  title: string
  items: ExtItem[]
}

export const clusters: ExtCluster[] = [
  {
    title: '搜索与计算',
    items: [
      {
        id: 'search',
        name: '全局搜索',
        desc: '应用 / 文件 / 扩展 / 剪贴板，流式增量召回',
        icon: 'ri-search-line',
      },
      { id: 'calculator', name: '计算器', desc: '输入即算，回车复制', icon: 'ri-calculator-line' },
      { id: 'currency', name: '汇率', desc: '金额与币种即时换算', icon: 'ri-exchange-cny-line' },
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
        desc: '对话与工具调用，联网搜索与执行命令',
        icon: 'ri-robot-2-line',
      },
      {
        id: 'translate',
        name: '翻译',
        desc: '选中文本即译，中英方向自动反转',
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
        desc: '后台记录文本 / 图片 / 文件，搜索与收藏',
        icon: 'ri-clipboard-line',
      },
      {
        id: 'screenshot',
        name: '截屏',
        desc: '标注 / OCR / 二维码 / 钉图 / 滚动长截图',
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
        id: 'system-status',
        name: '系统状态',
        desc: 'CPU / 内存 / 磁盘 / 网络概览',
        icon: 'ri-pulse-line',
      },
      { id: 'awake', name: '保持唤醒', desc: '接入电源时合盖熄屏不休眠', icon: 'ri-macbook-line' },
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
      { id: 'proxy', name: '代理', desc: 'TUN 模式，节点切换与测速', icon: 'ri-signal-tower-line' },
      { id: 'settings', name: '设置', desc: '快捷键 / 权限 / 更新', icon: 'ri-settings-3-line' },
    ],
  },
]

export const totalExtensions = clusters.reduce((n, c) => n + c.items.length, 0)
