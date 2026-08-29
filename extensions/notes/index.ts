import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import NotesView from './View.vue'
import NotesActions from './Actions.vue'
import NotesSettings from './Settings.vue'
import './locales'

export default defineExtension({
  meta: {
    id: 'notes',
    name: { 'zh-CN': '记事本', en: 'Notes' },
    description: { 'zh-CN': '随手记录,自动暂存', en: 'Quick notes, auto-saved' },
    icon: 'i-ri-sticky-note-line',
    keywords: ['note', 'notes', 'memo', 'pad', '记事本', '笔记', '备忘', '暂存'],
    order: 105,
  },

  disableSearchInput: true,
  mainView: () => NotesView,
  searchBarAccessory: () => NotesActions,
  subviews: { config: () => NotesSettings },
  windowHeight: 'auto',
  globalShortcuts: [
    {
      id: 'notes',
      default: 'Alt+N',
      onExecute: makeToggleHandler('notes'),
    },
  ],
})
