import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import ArchiveView from './View.vue'
import ArchiveSettings from './Settings.vue'
import ArchiveTreeView from './TreeView.vue'
import type { ArchiveInfo } from './logic'
import './locales'

/** 跨扩展投递的待处理路径（finder-ext 等经同页事件写入，View 消费后清空）。 */
export const pendingInputPaths = ref<string[]>([])

/** 当前压缩包信息 + 解压密码（View 与 entries 二级界面共享；密码不落盘）。 */
export const archiveInfo = ref<ArchiveInfo | null>(null)
export const archivePassword = ref('')

export default defineExtension({
  meta: {
    id: 'archive',
    name: { 'zh-CN': '解压缩', en: 'Archive' },
    description: {
      'zh-CN': 'zip / 7z 压缩与多格式解压，支持包内预览',
      en: 'Compress zip/7z and extract many formats with in-archive preview',
    },
    icon: 'i-ri-file-zip-line',
    order: 117,
    keywords: [
      'archive',
      'zip',
      'unzip',
      'rar',
      '7z',
      'tar',
      '压缩',
      '解压',
      '解压缩',
      '归档',
      '打包',
    ],
  },
  disableSearchInput: true,
  // 自适应：空态/压缩表单按内容收缩；解压列表经 max-h 上限 + 内部滚动封顶（防超长包撑破）
  windowHeight: 'auto',
  mainView: () => ArchiveView,
  subviews: {
    config: () => ArchiveSettings,
    entries: () => ArchiveTreeView,
  },
  subviewHeights: { config: 'auto', entries: 'auto' },
  subviewTitle: {
    config: { 'zh-CN': '设置', en: 'Settings' },
    entries: { 'zh-CN': '压缩包文件', en: 'Archive Files' },
  },
  setup: async () => {
    // 跨扩展同页投递：finder-ext 等经 window CustomEvent 同步写入待处理路径。
    // 同步是硬要求（见 image/video 同款注释）：投递先于首帧渲染，列表形状一次到位。
    window.addEventListener('archive-pending-input-path', (e) => {
      const detail = (e as CustomEvent<string[]>).detail
      pendingInputPaths.value = Array.isArray(detail) ? detail.filter(Boolean) : []
    })
  },
})
