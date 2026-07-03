import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAppStore } from './app'
import { toasts, clearToasts } from '@/composables/useToast'

describe('app store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    clearToasts()
  })

  it('初始状态', () => {
    const store = useAppStore()
    expect(store.activeModuleId).toBeNull()
    expect(store.searchQuery).toBe('')
    expect(store.isComposing).toBe(false)
    expect(store.isDialogOpen).toBe(false)
  })

  describe('setActiveModule', () => {
    it('切换模块并重置 subview', () => {
      const store = useAppStore()
      store.openSubview('settings')
      expect(store.activeSubview).toBe('settings')

      store.setActiveModule('clipboard')
      expect(store.activeModuleId).toBe('clipboard')
      expect(store.activeSubview).toBeNull()
      expect(store.subviewExternal).toBe(false)
    })

    it('退出模块回到全局模式', () => {
      const store = useAppStore()
      store.setActiveModule('clipboard')
      expect(store.activeModuleId).toBe('clipboard')

      store.setActiveModule(null)
      expect(store.activeModuleId).toBeNull()
    })

    it('进入模块快照入口 query（entryQuery）', () => {
      const store = useAppStore()
      store.setSearchQuery('/calc')
      store.setActiveModule('calculator')
      expect(store.entryQuery).toBe('/calc')
    })

    it('退出模块清空 entryQuery', () => {
      const store = useAppStore()
      store.setSearchQuery('/calc')
      store.setActiveModule('calculator')
      store.setActiveModule(null)
      expect(store.entryQuery).toBe('')
    })

    it('module→module 切换保留原入口（OCR→translate 等跨模块导航）', () => {
      const store = useAppStore()
      store.setSearchQuery('/')
      store.setActiveModule('screenshot')
      store.setSearchQuery('ocr text')
      // 跨模块切换不清空 entryQuery，ESC 回到最初进入点
      store.setActiveModule('translate')
      expect(store.entryQuery).toBe('/')
    })

    it('全局快捷键 toggle 路径：setActiveModule 后清 query，entryQuery 已先行快照', () => {
      // 模拟 makeToggleHandler：从工具列表按快捷键进入模块
      const store = useAppStore()
      store.setSearchQuery('/')
      store.setActiveModule('clipboard') // 快照 entryQuery='/'
      store.setSearchQuery('') // toggle handler 清空搜索
      expect(store.entryQuery).toBe('/')
      expect(store.searchQuery).toBe('')
    })
  })

  describe('搜索状态', () => {
    it('setSearchQuery 更新查询', () => {
      const store = useAppStore()
      store.setSearchQuery('测试')
      expect(store.searchQuery).toBe('测试')
    })

    it('setComposing 更新输入法状态', () => {
      const store = useAppStore()
      store.setComposing(true)
      expect(store.isComposing).toBe(true)
      store.setComposing(false)
      expect(store.isComposing).toBe(false)
    })
  })

  describe('确认对话框', () => {
    it('showConfirm 打开对话框并返回 Promise', () => {
      const store = useAppStore()
      const promise = store.showConfirm({ title: '确认删除？' })
      expect(store.isDialogOpen).toBe(true)
      expect(store.dialogOptions?.title).toBe('确认删除？')
      expect(promise).toBeInstanceOf(Promise)
    })

    it('resolveConfirm(true) 完成 Promise', async () => {
      const store = useAppStore()
      const promise = store.showConfirm({ title: '测试' })
      store.resolveConfirm(true)
      expect(await promise).toBe(true)
      expect(store.isDialogOpen).toBe(false)
    })

    it('resolveConfirm(false) 完成 Promise', async () => {
      const store = useAppStore()
      const promise = store.showConfirm({ title: '测试' })
      store.resolveConfirm(false)
      expect(await promise).toBe(false)
    })

    it('关闭后记录 lastDialogCloseTime', () => {
      const store = useAppStore()
      store.showConfirm({ title: '测试' })
      store.resolveConfirm(true)
      expect(store.lastDialogCloseTime).toBeGreaterThan(0)
    })
  })

  describe('Subview 管理', () => {
    it('openSubview 设置当前子视图', () => {
      const store = useAppStore()
      store.openSubview('config')
      expect(store.activeSubview).toBe('config')
      expect(store.subviewExternal).toBe(false)
    })

    it('openSubview external=true 标记外部打开', () => {
      const store = useAppStore()
      store.openSubview('ocr', true)
      expect(store.activeSubview).toBe('ocr')
      expect(store.subviewExternal).toBe(true)
    })

    it('closeSubview 清除当前子视图', () => {
      const store = useAppStore()
      store.openSubview('config')
      store.closeSubview()
      expect(store.activeSubview).toBeNull()
      expect(store.subviewExternal).toBe(false)
    })

    it('closeSubview 清除 external 标记', () => {
      const store = useAppStore()
      store.openSubview('ocr', true)
      store.closeSubview()
      expect(store.subviewExternal).toBe(false)
    })
  })

  describe('快捷键录制', () => {
    it('setShortcutRecording 切换录制状态', () => {
      const store = useAppStore()
      store.setShortcutRecording(true)
      expect(store.shortcutRecording).toBe(true)
      store.setShortcutRecording(false)
      expect(store.shortcutRecording).toBe(false)
    })

    it('setShortcutError / clearShortcutError 管理错误', () => {
      const store = useAppStore()
      store.setShortcutError('main', '冲突')
      expect(store.shortcutErrors['main']).toBe('冲突')
      store.clearShortcutError('main')
      expect(store.shortcutErrors['main']).toBeUndefined()
    })

    it('clearShortcutError 无错误时不报错', () => {
      const store = useAppStore()
      store.clearShortcutError('nonexistent')
      expect(Object.keys(store.shortcutErrors)).toHaveLength(0)
    })
  })

  describe('toast 消息', () => {
    it('showStatus 推入 toast', () => {
      const store = useAppStore()
      store.showStatus('已复制', { duration: 0 })
      expect(toasts.value).toHaveLength(1)
      expect(toasts.value[0].message).toBe('已复制')
      expect(toasts.value[0].kind).toBe('success')
    })

    it('showStatus duration 后自动清除', () => {
      vi.useFakeTimers()
      const store = useAppStore()
      store.showStatus('已复制', { duration: 1000 })
      expect(toasts.value).toHaveLength(1)
      vi.advanceTimersByTime(1000)
      expect(toasts.value).toHaveLength(0)
      vi.useRealTimers()
    })

    it('连续 showStatus 堆叠多条', () => {
      vi.useFakeTimers()
      const store = useAppStore()
      store.showStatus('第一条', { duration: 5000 })
      store.showStatus('第二条', { duration: 5000 })
      expect(toasts.value).toHaveLength(2)
      expect(toasts.value[1].message).toBe('第二条')
      vi.useRealTimers()
    })

    it('showStatus kind: error 切换错误语义', () => {
      const store = useAppStore()
      store.showStatus('启用失败', { duration: 0, kind: 'error' })
      expect(toasts.value[0].kind).toBe('error')
      expect(toasts.value[0].message).toBe('启用失败')
    })
  })
})
