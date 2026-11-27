/// 订阅二级界面状态与动作（SubscriptionsView.vue 仅模板）。
/// 节点列表不在此刷新：激活切换 / 激活订阅更新经 config 与 proxy-subscription-updated
/// 事件回到 useProxyPanel，由主视图在重新激活时拉取（二级界面期间主视图已被 KeepAlive 停用）。

import { computed, onDeactivated, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { toErrorMessage } from '@/utils/format'
import {
  type Subscription,
  config,
  addSubscription,
  removeSubscription,
  updateSubscription,
} from './config'
import { fetchSubscription, nodesUpdatedMessage } from './subscriptions'

/// 二级界面列表行：顶部「添加订阅」动作行 + 订阅行（均可键盘选中回车）。
/// id 供 BaseList 稳定 key（订阅行 sub.id，添加行固定 'add'）。
export type SubscriptionRow =
  { type: 'subscription'; id: string; sub: Subscription } | { type: 'add'; id: 'add' }

export function useSubscriptions() {
  const appStore = useAppStore()
  const selectedIndex = ref(0)

  // 订阅编辑弹窗（agent 模型提供商模式）
  const editingId = ref('')
  const isCreating = ref(false)
  const showEditModal = ref(false)
  const editForm = ref({ name: '', url: '' })
  // 订阅删除确认
  const deletingSub = ref<Subscription | null>(null)
  /// 正在行内更新的订阅 id（空 = 无更新进行中）：该行按钮转 spinner，单飞禁用其余行更新按钮
  const updatingSubId = ref('')

  /// 列表项：置顶「添加订阅」动作行（独立「通用」组），其下为订阅行。
  /// 订阅行按名称 / 链接过滤，添加行按自身标题过滤（与其他行同语义）。
  /// 恒返回新数组——增删经引用变化驱动 BaseList 重落选中滑层；订阅元数据更新不改变
  /// 身份，行内文本由各自响应式字段直接刷新。
  const items = computed<SubscriptionRow[]>(() => {
    const q = appStore.searchQuery.trim().toLowerCase()
    const match = (text: string) => !q || text.toLowerCase().includes(q)
    const rows: SubscriptionRow[] = []
    if (match(t('proxy.addSubscription'))) rows.push({ type: 'add', id: 'add' })
    rows.push(
      ...config.subscriptions
        .filter((s) => match(s.name || '') || match(s.url || ''))
        .map((sub): SubscriptionRow => ({ type: 'subscription', id: sub.id, sub })),
    )
    return rows
  })

  // 过滤收缩后选中归首项（防索引指向已滤除行）
  watch(items, () => {
    selectedIndex.value = 0
  })

  /// 回车/双击分派：添加订阅动作；有节点的订阅切换激活（主操作，仅激活订阅的节点入
  /// mihomo）；空订阅（未配置/未拉取）打开编辑配置。
  function onExecute(row: SubscriptionRow) {
    if (appStore.isComposing) return
    if (row.type === 'add') {
      openCreateModal()
      return
    }
    const s = row.sub
    if (s.proxyCount > 0) void setActiveSubscription(s.id)
    else openEditModal(s)
  }

  /// 切换激活订阅：写 config（持久化）+ 通知 Rust 更新 run_params 并热重载（含 idle 常驻）。
  /// 节点列表由主视图的 activeSubscriptionId watch 置脏，回到主视图时整体重载。
  async function setActiveSubscription(id: string) {
    if (config.activeSubscriptionId === id) return
    config.activeSubscriptionId = id
    try {
      await invoke(CMD.proxySetActiveSubscription, { id })
    } catch (e) {
      appStore.showStatus(toErrorMessage(e, t('proxy.switchSubscriptionFailed')), {
        duration: 4000,
        kind: 'error',
      })
    }
  }

  /// 添加订阅行：打开新建弹窗（不预创建项，保存时才 add）
  function openCreateModal() {
    editingId.value = ''
    isCreating.value = true
    editForm.value = { name: '', url: '' }
    showEditModal.value = true
  }

  function openEditModal(s: Subscription) {
    editingId.value = s.id
    isCreating.value = false
    editForm.value = { name: s.name, url: s.url }
    showEditModal.value = true
  }

  function closeEditModal() {
    showEditModal.value = false
    editingId.value = ''
    isCreating.value = false
  }

  /// 从订阅链接提取域名主体作为默认名称（如 https://a.example.com → example）
  function domainFromUrl(url: string): string {
    try {
      const parts = new URL(url).hostname.split('.')
      return parts.length >= 2 ? parts[parts.length - 2] : (parts[0] ?? '')
    } catch {
      return ''
    }
  }

  /// url 变化时，若用户未手填 name 则自动截取域名
  watch(
    () => editForm.value.url,
    (url) => {
      if (!editForm.value.name.trim()) {
        editForm.value.name = domainFromUrl(url)
      }
    },
  )

  /// 保存：新建则 add，编辑则 update；url 非空则拉取（含热重启 + 节点刷新）。
  /// 新建拉取成功且有节点即自动激活（首次添加即用）；节点列表回主视图时统一重载。
  async function saveSub() {
    const name = editForm.value.name.trim()
    const url = editForm.value.url.trim()
    const wasCreating = isCreating.value
    let id: string
    if (isCreating.value) {
      id = addSubscription(name, url)
    } else {
      id = editingId.value
      if (!id) return
      updateSubscription(id, { name, url })
    }
    closeEditModal()
    if (!url) return
    try {
      const res = await fetchSubscription(id, url)
      appStore.showStatus(nodesUpdatedMessage(res), { duration: 2000 })
      if (wasCreating && res.count > 0) await setActiveSubscription(id)
    } catch (e) {
      appStore.showStatus(toErrorMessage(e, t('proxy.updateFailed')), {
        duration: 4000,
        kind: 'error',
      })
    }
  }

  /// 行内更新订阅：按现有 URL 重新拉取（Rust 拉取 + 保存 + 热重载一体）。单飞：
  /// 任一更新中其余行更新按钮禁用。激活订阅更新后节点列表整体替换（主视图经
  /// proxy-subscription-updated 事件置脏重载）；非激活订阅仅回填元数据。
  async function refreshSub(s: Subscription) {
    if (!s.url || updatingSubId.value) return
    updatingSubId.value = s.id
    try {
      const res = await fetchSubscription(s.id, s.url)
      appStore.showStatus(nodesUpdatedMessage(res), { duration: 2000 })
    } catch (e) {
      appStore.showStatus(toErrorMessage(e, t('proxy.updateFailed')), {
        duration: 4000,
        kind: 'error',
      })
    } finally {
      updatingSubId.value = ''
    }
  }

  /// 编辑弹窗内删除 → 二次确认
  function confirmRemoveFromModal() {
    const s = config.subscriptions.find((x) => x.id === editingId.value)
    if (!s) return
    closeEditModal()
    deletingSub.value = s
  }

  async function doRemoveSub() {
    const s = deletingSub.value
    if (!s) return
    deletingSub.value = null
    // 新激活：删的若是当前激活则回退到剩余首项（splice 前 filter，规避 watch 异步补项时序）
    const others = config.subscriptions.filter((x) => x.id !== s.id)
    const newActive = others[0]?.id ?? ''
    if (config.activeSubscriptionId === s.id) {
      config.activeSubscriptionId = newActive
    }
    removeSubscription(s.id)
    try {
      // 热重启完成（含 wait_ready）后主视图重激活时按新激活订阅重载节点
      await invoke(CMD.proxyRemoveSubscription, { id: s.id, newActiveSubId: newActive })
    } catch (e) {
      appStore.showStatus(toErrorMessage(e, t('proxy.cleanupSubscriptionFailed')), {
        duration: 4000,
        kind: 'error',
      })
    }
  }

  // 离开二级界面（Esc / 主快捷键 / 跨视图）：
  // - 清空过滤词：该词只属于订阅列表，残留会继续过滤主视图的节点列表
  // - 收起弹窗：弹窗打开时主快捷键等路径不经 Esc 直接离开，KeepAlive 缓存会把
  //   残留弹窗带到下次进入界面
  onDeactivated(() => {
    appStore.setSearchQuery('')
    closeEditModal()
    deletingSub.value = null
  })

  return {
    items,
    selectedIndex,
    config,
    onExecute,
    openEditModal,
    refreshSub,
    updatingSubId,
    showEditModal,
    isCreating,
    closeEditModal,
    saveSub,
    editForm,
    confirmRemoveFromModal,
    deletingSub,
    doRemoveSub,
  }
}
