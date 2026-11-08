<template>
  <div class="flex-col-view">
    <BaseList :items="filtered" v-model:selected-index="selectedIndex" @execute="onExecute">
      <template #item="{ item }">
        <BaseListItem :title="item.name" :icon="item.icon || 'i-ri-apps-2-line'" />
      </template>
    </BaseList>
    <BaseEmptyState
      v-if="filtered.length === 0"
      icon="i-ri-apps-2-line"
      :title="t('common.noResults')"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onActivated, onDeactivated } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import { fetchApps, orderApps, type AppEntry } from './apps'
import { config as finderConfig, rememberRecentApp } from './config'
import { runAction } from './actions'
import { reactivateTick } from './index'

const appStore = useAppStore()
const apps = ref<AppEntry[]>([])
const selectedIndex = ref(0)

/** 装载进行中标志：onMounted 与 onActivated 首次都会触发，合并为单轮探测。 */
let loadInFlight = false

/** 装载列表：访达选区（取首项，多选按首项类型推荐）→ LaunchServices 类型推荐 +
 *  MRU + 已安装全量，orderApps 统一排序。选区 / 权限缺失静默降级（仅 MRU + 全量）。 */
async function load() {
  if (loadInFlight) return
  loadInFlight = true
  try {
    const paths = await invoke<string[]>(CMD.finderSelectedPaths).catch(() => [] as string[])
    const ls = paths[0]
      ? await invoke<string[]>(CMD.finderOpenWithApps, { path: paths[0] }).catch(() => [])
      : []
    // fetchApps 缓存命中零开销；失败回退现有列表（排序尽力而为）
    const installed = await fetchApps().catch(() => apps.value)
    apps.value = orderApps(installed, ls, finderConfig.recentApps)
  } finally {
    loadInFlight = false
  }
}

onMounted(load)
onActivated(load)
// 离开二级界面清空过滤词（返回面板后残留词会继续过滤操作目录，与 archive TreeView 同款收尾）
onDeactivated(() => {
  appStore.setSearchQuery('')
})
// 快捷键重呼（窗口隐藏后再唤起，subview 保持激活无 onActivated 跟进）：重新装载
// （MRU 可能因上次执行更新、选区可能已变）
watch(reactivateTick, () => void load())

/** 搜索框过滤（扩展模式 query 即列表过滤参数）：应用名 / 路径包含匹配。 */
const filtered = computed(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return apps.value
  return apps.value.filter(
    (a) => a.name.toLowerCase().includes(q) || a.path.toLowerCase().includes(q),
  )
})

// 过滤收缩后选中归首项（防索引指向已滤除行）
watch(filtered, () => {
  selectedIndex.value = 0
})

/** 执行行：打开成功记忆 MRU（下次置顶直达）。 */
async function onExecute(app: AppEntry) {
  const ok = await runAction('open_with', undefined, app.path)
  if (ok) rememberRecentApp(app.path)
}
</script>
