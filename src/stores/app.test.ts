import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import type { Component } from 'vue'
import { useAppStore } from './app'
import { showToast } from '@/composables/useToast'

vi.mock('@/composables/useToast', () => ({ showToast: vi.fn() }))

describe('app store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.mocked(showToast).mockClear()
    sessionStorage.clear()
  })

  it('初始状态', () => {
    const store = useAppStore()
    expect(store.activeExtId).toBeNull()
    expect(store.searchQuery).toBe('')
    expect(store.isComposing).toBe(false)
    expect(store.isDialogOpen).toBe(false)
  })

  it('激活扩展写入 sessionStorage（WebContent navigate 重载恢复源）', () => {
    const store = useAppStore()
    store.setActiveExtension('agent')
    expect(sessionStorage.getItem('voidnix.active-ext')).toBe('agent')
    store.setActiveExtension(null)
    expect(sessionStorage.getItem('voidnix.active-ext')).toBeNull()
  })

  it('store 创建时从 sessionStorage 恢复激活扩展（重载后回到隐藏前视图）', () => {
    sessionStorage.setItem('voidnix.active-ext', 'agent')
    const store = useAppStore()
    expect(store.activeExtId).toBe('agent')
  })

  describe('setActiveExtension', () => {
    it('切换扩展并重置 subview', () => {
      const store = useAppStore()
      store.openSubview('settings')
      expect(store.activeSubview).toBe('settings')

      store.setActiveExtension('clipboard')
      expect(store.activeExtId).toBe('clipboard')
      expect(store.activeSubview).toBeNull()
      expect(store.subviewExternal).toBe(false)
    })

    it('退出扩展回到全局模式', () => {
      const store = useAppStore()
      store.setActiveExtension('clipboard')
      expect(store.activeExtId).toBe('clipboard')

      store.setActiveExtension(null)
      expect(store.activeExtId).toBeNull()
    })

    it('进入扩展快照入口 query（entryQuery）', () => {
      const store = useAppStore()
      store.setSearchQuery('/calc')
      store.setActiveExtension('calculator')
      expect(store.entryQuery).toBe('/calc')
    })

    it('带 from 的导航记忆 Esc 返回目标，其余激活路径清空', () => {
      const store = useAppStore()
      store.setActiveExtension('ai-gateway')
      expect(store.extensionReturnExtId).toBeNull()

      store.setActiveExtension('ai-providers', 'ai-gateway')
      expect(store.extensionReturnExtId).toBe('ai-gateway')

      // 返回跳转（不带 from）：目标消费即清空
      store.setActiveExtension('ai-gateway')
      expect(store.extensionReturnExtId).toBeNull()

      // 导航后再切第三方扩展：过期目标清空
      store.setActiveExtension('ai-providers', 'ai-gateway')
      store.setActiveExtension('notes')
      expect(store.extensionReturnExtId).toBeNull()
    })

    it('退出扩展清空 entryQuery', () => {
      const store = useAppStore()
      store.setSearchQuery('/calc')
      store.setActiveExtension('calculator')
      store.setActiveExtension(null)
      expect(store.entryQuery).toBe('')
    })

    it('ext→ext 切换保留原入口（OCR→translate 等跨扩展导航）', () => {
      const store = useAppStore()
      store.setSearchQuery('/')
      store.setActiveExtension('screenshot')
      store.setSearchQuery('ocr text')
      // 跨扩展切换不清空 entryQuery，ESC 回到最初进入点
      store.setActiveExtension('translate')
      expect(store.entryQuery).toBe('/')
    })

    it('全局快捷键 toggle 路径：setActiveExtension 后清 query，entryQuery 已先行快照', () => {
      // 模拟 makeToggleHandler：从工具列表按快捷键进入扩展
      const store = useAppStore()
      store.setSearchQuery('/')
      store.setActiveExtension('clipboard') // 快照 entryQuery='/'
      store.setSearchQuery('') // toggle handler 清空搜索
      expect(store.entryQuery).toBe('/')
      expect(store.searchQuery).toBe('')
    })

    it('切换扩展时关闭全局 confirm（避免 Teleport 遮罩残留）', async () => {
      const store = useAppStore()
      store.setActiveExtension('clipboard')
      const promise = store.showConfirm({ title: '确认？' })
      expect(store.isDialogOpen).toBe(true)
      store.setActiveExtension('translate')
      expect(store.isDialogOpen).toBe(false)
      expect(await promise).toBe(false)
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

    it('fullscreenView 槽：setFullscreenView 置换 / 清空', () => {
      const store = useAppStore()
      expect(store.fullscreenView).toBeNull()
      const view = { render: () => null } as unknown as Component
      store.setFullscreenView(view)
      // store 为 reactive：读回是代理对象，断言槽语义（非空接管 / null 让位）
      expect(store.fullscreenView).toBeTruthy()
      store.setFullscreenView(null)
      expect(store.fullscreenView).toBeNull()
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
    it('showStatus 委托 showToast（默认 success）', () => {
      const store = useAppStore()
      store.showStatus('已复制')
      expect(showToast).toHaveBeenCalledWith('已复制', undefined)
    })

    it('showStatus 透传 kind: error', () => {
      const store = useAppStore()
      store.showStatus('启用失败', { kind: 'error' })
      expect(showToast).toHaveBeenCalledWith('启用失败', { kind: 'error' })
    })

    it('showStatus 透传 duration', () => {
      const store = useAppStore()
      store.showStatus('已复制', { duration: 800 })
      expect(showToast).toHaveBeenCalledWith('已复制', { duration: 800 })
    })
  })
})
