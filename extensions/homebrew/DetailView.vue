<template>
  <div flex="~ col">
    <BaseEmptyState v-if="error" icon="i-ri-error-warning-line" :title="error" />
    <BaseEmptyState v-else-if="loading" :loading="true" />

    <BaseList
      v-else-if="filteredItems.length > 0"
      :items="filteredItems"
      v-model:selected-index="selectedIndex"
      :group-field="(item: InfoItem) => item.section"
      :group-title="groupTitle"
      @execute="onExecute"
    >
      <template #item="{ item }">
        <BaseListItem :title="item.title" :icon="item.icon">
          <template v-if="item.subtitle || item.subDesc" #subtitle>
            <span v-if="item.subtitle" text="xs muted" font="mono" shrink="0">{{
              item.subtitle
            }}</span>
            <span v-if="item.subtitle && item.subDesc" text="muted" mx="1">·</span>
            <span v-if="item.subDesc" text="xs muted" class="flex-1 min-w-0 truncate">{{
              item.subDesc
            }}</span>
          </template>
          <template v-if="item.id === 'self'" #trailing>
            <BaseButton
              variant="danger"
              icon="i-ri-uninstall-line"
              :disabled="running"
              @click.stop="confirmUninstall"
            >
              卸载
            </BaseButton>
          </template>
        </BaseListItem>
      </template>
    </BaseList>

    <BaseEmptyState v-else icon="i-ri-search-eye-line" title="无匹配" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onActivated } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { useAppStore } from '@/stores/app'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

interface PackageSummary {
  name: string
  version: string
  desc: string
}

interface BrewInfo {
  desc: string
  deps: PackageSummary[]
  uses: PackageSummary[]
}

interface DetailTarget {
  name: string
  kind: string
  version: string
  desc: string
}

interface BrewEvent {
  kind: string
  text: string
}

interface InfoItem {
  id: string
  section: string
  title: string
  subtitle?: string
  subDesc?: string
  icon?: string
  /** 回车时打开详情的包名（依赖/被依赖项可递归进入） */
  navigateTo?: string
}

const appStore = useAppStore()
const loading = ref(false)
const error = ref('')
const info = ref<BrewInfo | null>(null)
const target = ref<DetailTarget | null>(null)
const selectedIndex = ref(0)
const running = ref(false)

const HEADER_SENTINEL = '__header__'

const allItems = computed<InfoItem[]>(() => {
  if (!info.value || !target.value) return []
  const result: InfoItem[] = []

  // 第一项：与外面列表项一致（无分组标题），右侧卸载按钮
  result.push({
    id: 'self',
    section: HEADER_SENTINEL,
    title: target.value.name,
    icon: 'i-ri-box-3-line',
    subtitle: target.value.version,
    subDesc: target.value.desc || info.value.desc,
  })

  // 依赖
  for (const dep of info.value.deps) {
    result.push({
      id: `dep:${dep.name}`,
      section: '依赖',
      title: dep.name,
      subtitle: dep.version,
      subDesc: dep.desc,
      navigateTo: dep.name,
    })
  }

  // 被依赖
  for (const use of info.value.uses) {
    result.push({
      id: `use:${use.name}`,
      section: '被依赖',
      title: use.name,
      subtitle: use.version,
      subDesc: use.desc,
      navigateTo: use.name,
    })
  }

  return result
})

function groupTitle(g: string): string {
  if (g === HEADER_SENTINEL) return ''
  return g
}

const filteredItems = computed<InfoItem[]>(() => {
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return allItems.value
  return allItems.value.filter(
    (item) =>
      item.title.toLowerCase().includes(q) ||
      item.section.toLowerCase().includes(q) ||
      (item.subDesc ?? '').toLowerCase().includes(q),
  )
})

watch(filteredItems, (list) => {
  if (selectedIndex.value >= list.length) selectedIndex.value = 0
})

async function fetchInfo() {
  if (!isTauri) return
  const raw = sessionStorage.getItem('homebrew:detail')
  if (!raw) {
    error.value = '缺少包信息'
    return
  }
  target.value = JSON.parse(raw) as DetailTarget

  loading.value = true
  error.value = ''
  selectedIndex.value = 0
  appStore.setSearchQuery('')
  try {
    info.value = await invoke<BrewInfo>(CMD.brewInfo, { name: target.value.name })
  } catch (e) {
    error.value = String(e ?? '未知错误')
  } finally {
    loading.value = false
  }
}

function onExecute(item: InfoItem) {
  // 首项回车 = 卸载
  if (item.id === 'self') {
    confirmUninstall()
    return
  }
  if (item.navigateTo) {
    sessionStorage.setItem(
      'homebrew:detail',
      JSON.stringify({
        name: item.navigateTo,
        kind: '',
        version: item.subtitle ?? '',
        desc: item.subDesc ?? '',
      }),
    )
    fetchInfo()
  }
}

async function confirmUninstall() {
  if (!target.value || running.value) return
  const name = target.value.name
  const uses = info.value?.uses ?? []
  const deps = info.value?.deps ?? []

  const parts: string[] = []
  if (uses.length > 0) {
    parts.push(`此包被 ${uses.length} 个包依赖（${uses.map((u) => u.name).join('、')}）`)
  }
  if (deps.length > 0) {
    parts.push('孤立的依赖将自动清理')
  }
  parts.push('确定要卸载此包吗？')
  const message = parts.join('，')

  const ok = await appStore.showConfirm({
    title: `卸载 ${name}？`,
    message,
    okLabel: '确认',
    cancelLabel: '取消',
  })
  if (!ok) return

  running.value = true
  const channel = new Channel<BrewEvent>()
  channel.onmessage = () => {}
  try {
    await invoke(CMD.brewRun, { operation: 'uninstall', target: name, onEvent: channel })
    appStore.showStatus(`已卸载 ${name}`)
    appStore.closeSubview()
  } catch (e) {
    appStore.showStatus(String(e ?? '未知错误'), { kind: 'error' })
  } finally {
    running.value = false
  }
}

onActivated(fetchInfo)
</script>
