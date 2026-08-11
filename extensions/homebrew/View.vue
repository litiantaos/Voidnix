<template>
  <div flex="~ col">
    <BaseEmptyState v-if="error" icon="i-ri-error-warning-line" :title="error" />

    <BaseEmptyState v-else-if="loading" :loading="true" />

    <template v-else-if="status">
      <BaseList
        :items="listItems"
        v-model:selected-index="selectedIndex"
        :group-field="(item: ListItem) => item.kind"
        :group-title="groupTitle"
        @execute="onExecute"
      >
        <template #item="{ item }">
          <!-- Homebrew 状态行 -->
          <BaseListItem
            v-if="item.type === 'status'"
            icon="i-ri-cup-fill"
            icon-wrapper-class="fill-mist"
            title="Homebrew"
          >
            <template #subtitle>
              <span v-if="status.version" text="muted" font="mono" shrink="0"
                >v{{ status.version }}</span
              >
              <span text="muted" mx="1">·</span>
              <span text="secondary">{{ formulaCount }} formula</span>
              <span text="muted" mx="1">·</span>
              <span text="secondary">{{ caskCount }} cask</span>
              <template v-if="outdatedCount > 0">
                <span text="muted" mx="1">·</span>
                <span text="warning font-medium">{{
                  t('homebrew.updates', { count: outdatedCount })
                }}</span>
              </template>
            </template>
            <template v-if="status.has_update" #trailing>
              <BaseButton
                variant="primary"
                :icon="running ? 'i-ri-loader-4-line animate-spin' : 'i-ri-arrow-up-circle-line'"
                :disabled="running"
                @click.stop="run('update_upgrade')"
              >
                {{
                  running
                    ? stepLabels[runningStep] || t('homebrew.processing')
                    : t('homebrew.update')
                }}
              </BaseButton>
            </template>
          </BaseListItem>

          <!-- 服务行 -->
          <BaseListItem
            v-else-if="item.type === 'service'"
            :icon="serviceIcon(item.status)"
            icon-wrapper-class="fill-mist"
            :title="item.name"
            :tone="item.status === 'started' ? 'accent' : undefined"
          >
            <template #subtitle>
              <span
                text="xs"
                shrink="0"
                :class="item.status === 'started' ? 'text-success' : 'text-muted'"
              >
                {{ serviceStatusText(item.status) }}
              </span>
            </template>
            <template #trailing>
              <div flex gap="1">
                <BaseButton
                  v-if="item.status !== 'started'"
                  variant="ghost"
                  icon="i-ri-play-line"
                  :disabled="running"
                  class="flex-center !px-0 !w-7"
                  :title="t('homebrew.start')"
                  @click.stop="runService('services_start', item.name)"
                />
                <BaseButton
                  v-if="item.status === 'started'"
                  variant="ghost"
                  icon="i-ri-stop-line"
                  :disabled="running"
                  class="flex-center !px-0 !w-7"
                  :title="t('homebrew.stop')"
                  @click.stop="runService('services_stop', item.name)"
                />
                <BaseButton
                  variant="ghost"
                  icon="i-ri-restart-line"
                  :disabled="running"
                  class="flex-center !px-0 !w-7"
                  :title="t('homebrew.restart')"
                  @click.stop="runService('services_restart', item.name)"
                />
              </div>
            </template>
          </BaseListItem>

          <!-- 包行 -->
          <BaseListItem v-else :title="item.name" :tone="item.outdated ? 'accent' : undefined">
            <template #subtitle>
              <span
                text="xs"
                font="mono"
                shrink="0"
                :class="item.outdated ? 'text-warning' : 'text-muted'"
              >
                {{ item.version }}<span v-if="item.outdated"> -> {{ item.new_version }}</span>
              </span>
              <span v-if="item.desc" text="muted" mx="1">·</span>
              <span v-if="item.desc" text="xs muted" class="flex-1 min-w-0 truncate">{{
                item.desc
              }}</span>
            </template>
          </BaseListItem>
        </template>
      </BaseList>

      <BaseEmptyState
        v-if="listItems.length === 0"
        :icon="hasQuery ? 'i-ri-search-eye-line' : 'i-ri-inbox-line'"
        :title="hasQuery ? t('homebrew.noMatch') : t('homebrew.noInstalled')"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'

interface InstalledPackage {
  name: string
  kind: string
  desc: string
  version: string
  new_version: string
}
interface BrewStatus {
  version: string
  packages: InstalledPackage[]
  has_update: boolean
}
interface BrewEvent {
  kind: string
  text: string
}
interface BrewService {
  name: string
  status: string
}
interface BrewRunState {
  operation: string
  step: string
}
type ListItem =
  | { type: 'status'; id: '__status__'; kind: '__status__' }
  | { type: 'service'; id: string; kind: '__service__'; name: string; status: string }
  | {
      type: 'package'
      id: string
      kind: string
      name: string
      desc: string
      version: string
      new_version: string
      outdated: boolean
    }

const appStore = useAppStore()
const status = ref<BrewStatus | null>(null)
const services = ref<BrewService[]>([])
const loading = ref(false)
const error = ref('')
const running = ref(false)
const runningStep = ref('')
const selectedIndex = ref(0)

const stepLabels = computed<Record<string, string>>(() => ({
  update: t('homebrew.step.update'),
  upgrade: t('homebrew.step.upgrade'),
  cleanup: t('homebrew.step.cleanup'),
  autoremove: t('homebrew.step.autoremove'),
  uninstall: t('homebrew.step.uninstall'),
  'services start': t('homebrew.step.servicesStart'),
  'services stop': t('homebrew.step.servicesStop'),
  'services restart': t('homebrew.step.servicesRestart'),
}))

const hasQuery = computed(() => appStore.searchQuery.trim().length > 0)

const formulaCount = computed(
  () => status.value?.packages.filter((p) => p.kind === 'formula').length ?? 0,
)
const caskCount = computed(
  () => status.value?.packages.filter((p) => p.kind === 'cask').length ?? 0,
)
const outdatedCount = computed(
  () => (status.value?.packages ?? []).filter((p) => p.new_version).length,
)

const filteredPackages = computed(() => {
  const pkgs = status.value?.packages ?? []
  const q = appStore.searchQuery.trim().toLowerCase()
  if (!q) return pkgs
  return pkgs.filter((p) => p.name.toLowerCase().includes(q))
})

const listItems = computed<ListItem[]>(() => {
  const items: ListItem[] = []
  if (!hasQuery.value) {
    items.push({ type: 'status', id: '__status__', kind: '__status__' })
    for (const s of services.value) {
      items.push({
        type: 'service',
        id: `svc:${s.name}`,
        kind: '__service__',
        name: s.name,
        status: s.status,
      })
    }
  }
  for (const p of filteredPackages.value) {
    items.push({
      type: 'package',
      id: `${p.kind}:${p.name}`,
      kind: p.kind,
      name: p.name,
      desc: p.desc,
      version: p.version,
      new_version: p.new_version,
      outdated: !!p.new_version,
    })
  }
  return items
})

function groupTitle(g: string): string {
  if (g === '__status__') return ''
  if (g === '__service__') return t('homebrew.services')
  return g === 'cask' ? 'Casks' : 'Formulae'
}

function serviceIcon(status: string): string {
  return status === 'started' ? 'i-ri-flashlight-line' : 'i-ri-shut-down-line'
}

function serviceStatusText(status: string): string {
  if (status === 'started') return t('homebrew.running')
  if (status === 'stopped') return t('homebrew.stopped')
  if (status === 'error') return t('homebrew.error')
  return status
}

watch(listItems, (list) => {
  if (selectedIndex.value >= list.length) selectedIndex.value = 0
})

async function fetchStatus() {
  if (!isTauri || running.value) return
  loading.value = true
  error.value = ''
  try {
    const [s, svc] = await Promise.all([
      invoke<BrewStatus>(CMD.brewStatus),
      invoke<BrewService[]>(CMD.brewServices).catch(() => [] as BrewService[]),
    ])
    status.value = s
    services.value = svc
  } catch (e) {
    error.value = String(e ?? t('common.unknownError'))
  } finally {
    loading.value = false
  }
}

async function run(operation: string) {
  if (!isTauri || running.value) return
  running.value = true
  runningStep.value = ''
  const channel = new Channel<BrewEvent>()
  channel.onmessage = (e: BrewEvent) => {
    if (e.kind === 'step') runningStep.value = e.text
  }

  try {
    await invoke(CMD.brewRun, { operation, onEvent: channel })
    running.value = false
    await fetchStatus()
    appStore.showStatus(t('homebrew.updateDone'))
  } catch (e) {
    appStore.showStatus(String(e ?? t('common.unknownError')), { kind: 'error' })
  } finally {
    running.value = false
    runningStep.value = ''
  }
}

async function runService(operation: string, name: string) {
  if (!isTauri || running.value) return
  running.value = true
  runningStep.value = ''
  const channel = new Channel<BrewEvent>()
  channel.onmessage = (e: BrewEvent) => {
    if (e.kind === 'step') runningStep.value = e.text
  }

  try {
    await invoke(CMD.brewRun, { operation, target: name, onEvent: channel })
    services.value = await invoke<BrewService[]>(CMD.brewServices).catch(() => [] as BrewService[])
    const actionMap: Record<string, string> = {
      start: t('homebrew.start'),
      stop: t('homebrew.stop'),
      restart: t('homebrew.restart'),
    }
    const action = operation.replace('services_', '')
    appStore.showStatus(`${actionMap[action] ?? action} ${name}`)
  } catch (e) {
    appStore.showStatus(String(e ?? t('common.unknownError')), { kind: 'error' })
  } finally {
    running.value = false
    runningStep.value = ''
  }
}

function onExecute(item: ListItem) {
  if (item.type === 'status') {
    if (status.value?.has_update && !running.value) run('update_upgrade')
    return
  }
  if (item.type !== 'package') return
  sessionStorage.setItem(
    'homebrew:detail',
    JSON.stringify({
      name: item.name,
      kind: item.kind,
      version: item.version,
      desc: item.desc,
    }),
  )
  appStore.openSubview('detail', false)
}

let unlistenDone: (() => void) | null = null

onMounted(async () => {
  if (!isTauri) return
  // 先注册监听再查状态，消除「查询返回 Some → 操作恰在此间隙结束 → 事件无人接收」的 TOCTOU 竞态
  let done = false
  const unlisten = await listen<BrewRunState | null>('brew-run-done', async () => {
    done = true
    unlisten()
    unlistenDone = null
    running.value = false
    runningStep.value = ''
    loading.value = false
    await fetchStatus()
  })
  // 查状态：null = 操作已结束（事件可能已被上面的监听捕获），Some = 仍在运行
  const state = await invoke<BrewRunState | null>(CMD.brewRunState)
  if (state && !done) {
    running.value = true
    runningStep.value = state.step
    loading.value = true
    unlistenDone = unlisten
  } else {
    unlisten()
    await fetchStatus()
  }
})

onUnmounted(() => {
  unlistenDone?.()
  unlistenDone = null
})
</script>
