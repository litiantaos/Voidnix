<template>
  <div flex="~ col">
    <BaseEmptyState v-if="error" icon="i-ri-error-warning-line" :title="error" />

    <!-- 加载态仅在无数据时接管：重激活重拉期间保留 KeepAlive 缓存列表，防闪 spinner -->
    <BaseEmptyState v-else-if="loading && !status" :loading="true" />

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
            <!-- 运行中：按钮位滚动显示进度详情（步骤名 / 升级步骤的包名+计数；后台元数据刷新
                 同位显示「拉取更新」），文案切换走纵向滚动 + 宽度渐变（宽度盒锁旧量新） -->
            <template v-if="status.has_update || refreshing" #trailing>
              <BaseButton
                :icon="running ? 'i-ri-loader-4-line animate-spin' : 'i-ri-arrow-up-circle-line'"
                :disabled="running"
                @click.stop="run('update_upgrade')"
              >
                <span ref="labelBoxRef" class="brew-label-box" @transitionend="onLabelWidthSettled">
                  <Transition
                    name="brew-roll"
                    mode="out-in"
                    @leave="onLabelLeave"
                    @enter="onLabelEnter"
                  >
                    <span :key="updateButtonLabel" class="brew-label">{{ updateButtonLabel }}</span>
                  </Transition>
                </span>
              </BaseButton>
            </template>
          </BaseListItem>

          <!-- 服务行 -->
          <BaseListItem
            v-else-if="item.type === 'service'"
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
                  icon="i-ri-play-circle-line"
                  :disabled="running"
                  :title="t('homebrew.start')"
                  @click.stop="runService('services_start', item.name)"
                />
                <BaseButton
                  v-if="item.status === 'started'"
                  icon="i-ri-stop-circle-line"
                  :disabled="running"
                  :title="t('homebrew.stop')"
                  @click.stop="runService('services_stop', item.name)"
                />
                <BaseButton
                  icon="i-ri-restart-line"
                  :disabled="running"
                  :title="t('homebrew.restart')"
                  @click.stop="runService('services_restart', item.name)"
                />
              </div>
            </template>
          </BaseListItem>

          <!-- 包行 -->
          <BaseListItem v-else :title="item.name">
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
import { ref, computed, watch, onActivated, onUnmounted } from 'vue'
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
  /** 元数据陈旧，后台 brew update 进行中（完成经 brew-run-done 驱动重拉） */
  refreshing: boolean
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
// 初值 true：首帧即 loading 态（onActivated 的 IPC 往返前不闪空白帧）
const loading = ref(true)
const error = ref('')
const running = ref(false)
const runningStep = ref('')
const runningOperation = ref('')
/** 升级步骤的单包进度：🍺 行计数完成、Pouring/Downloading/Upgrading 行取当前包名（总数 = 列表过期数） */
const upgradeDone = ref(0)
const currentPackage = ref('')
const refreshing = ref(false)
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

/** 更新按钮文案：空闲「更新」，运行中滚动进度详情——升级步骤显示当前包名 + 完成数/总数
 *  （包名未知或重挂载恢复时回落步骤名），其余步骤与服务操作显示步骤名 / 处理中 */
const updateButtonLabel = computed(() => {
  if (!running.value) return t('homebrew.update')
  if (
    runningOperation.value === 'update_upgrade' &&
    runningStep.value === 'upgrade' &&
    currentPackage.value &&
    outdatedCount.value > 0
  ) {
    return `${currentPackage.value} ${upgradeDone.value}/${outdatedCount.value}`
  }
  return stepLabels.value[runningStep.value] || t('homebrew.processing')
})

// ── 文案宽度过渡（auto↔px 无 CSS 过渡，同 BaseDialog 内容高 FLIP 范式）──
// leave 锁旧宽（out-in 空档防坍缩），enter 量新宽后由宽度盒常驻 width transition 插值，
// transitionend 清回 auto；文案的滚动 transform/opacity 与宽度互不干扰
const labelBoxRef = ref<HTMLElement | null>(null)

function onLabelLeave() {
  const box = labelBoxRef.value
  if (box) box.style.width = `${box.offsetWidth}px`
}

function onLabelEnter() {
  const box = labelBoxRef.value
  if (!box) return
  const from = parseInt(box.style.width)
  box.style.width = 'auto'
  const to = box.offsetWidth
  if (!Number.isFinite(from) || from === to) {
    box.style.width = ''
    return
  }
  box.style.width = `${from}px`
  void box.offsetWidth
  box.style.width = `${to}px`
}

function onLabelWidthSettled(e: TransitionEvent) {
  if (e.propertyName === 'width' && e.target === labelBoxRef.value) {
    labelBoxRef.value!.style.width = ''
  }
}

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

function serviceStatusText(status: string): string {
  if (status === 'started') return t('homebrew.running')
  if (status === 'stopped') return t('homebrew.stopped')
  if (status === 'error') return t('homebrew.error')
  return status
}

watch(listItems, (list) => {
  if (selectedIndex.value >= list.length) selectedIndex.value = 0
})

// 拉取竞态守卫：重挂载恢复态的拉取与 brew-run-done 重拉可能并发，仅最新一轮可落盘
let fetchSeq = 0

async function fetchStatus() {
  if (!isTauri) return
  const seq = ++fetchSeq
  loading.value = true
  error.value = ''
  try {
    const [s, svc] = await Promise.all([
      invoke<BrewStatus>(CMD.brewStatus),
      invoke<BrewService[]>(CMD.brewServices).catch(() => [] as BrewService[]),
    ])
    if (seq !== fetchSeq) return
    status.value = s
    services.value = svc
    refreshing.value = s.refreshing
    if (s.refreshing && !running.value) {
      // 后台 brew update 在途（本次拉取刚发起）：进入运行态（按钮旋转禁用），完成事件统一重拉；
      // 已在运行态（重挂载恢复）则不覆盖现有 step
      running.value = true
      runningStep.value = 'update'
    }
  } catch (e) {
    if (seq === fetchSeq) error.value = String(e ?? t('common.unknownError'))
  } finally {
    if (seq === fetchSeq) loading.value = false
  }
}

/** 升级行 → 当前包名（Pouring 的 `name--version` 取 name；Upgrading 行滤掉
 *  「N outdated packages:」表头与 cask 的「Cask」前缀；Downloading 取 ghcr blobs
 *  URL 段（下载先于 Pouring，是 formula 升级的最长阶段）；tap 全限定名归一化为短名，
 *  与列表一致 */
function currentPackageFrom(line: string): string | null {
  let m = /^==> Pouring (.+?)--/.exec(line)
  if (m) return m[1]!.split('/').pop() ?? null
  m = /^==> Downloading \S*\/([^/\s]+)\/blobs\//.exec(line)
  if (m) return m[1]!.split('/').pop() ?? null
  m = /^==> Upgrading(?: Cask)? (\S+)/.exec(line)
  if (m && !/^\d/.test(m[1]!)) return m[1]!.split('/').pop() ?? null
  m = /^==> Installing Cask (\S+)/.exec(line)
  if (m) return m[1]!.split('/').pop() ?? null
  return null
}

async function run(operation: string) {
  if (!isTauri || running.value) return
  running.value = true
  runningStep.value = ''
  runningOperation.value = operation
  upgradeDone.value = 0
  currentPackage.value = ''
  const channel = new Channel<BrewEvent>()
  channel.onmessage = (e: BrewEvent) => {
    if (e.kind === 'step') {
      runningStep.value = e.text
    } else if (e.kind === 'line' && runningStep.value === 'upgrade') {
      // 🍺 行 = 单包升级完成；Pouring / Downloading / Upgrading 行 = 当前包
      if (e.text.startsWith('🍺')) upgradeDone.value += 1
      const name = currentPackageFrom(e.text)
      if (name) currentPackage.value = name
    }
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
    runningOperation.value = ''
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

function openDetail(target: { name: string; kind: string; version: string; desc: string }) {
  sessionStorage.setItem('homebrew:detail', JSON.stringify(target))
  appStore.openSubview('detail', false)
}

function onExecute(item: ListItem) {
  if (item.type === 'status') {
    if (status.value?.has_update && !running.value) run('update_upgrade')
    return
  }
  if (item.type === 'service') {
    // 服务行也是包：回车/双击进入包详情（与包行一致），启停/重启走行内按钮
    const pkg = status.value?.packages.find((p) => p.name === item.name)
    openDetail({
      name: item.name,
      kind: pkg?.kind ?? 'formula',
      version: pkg?.version ?? '',
      desc: pkg?.desc ?? '',
    })
    return
  }
  openDetail({ name: item.name, kind: item.kind, version: item.version, desc: item.desc })
}

// KeepAlive（ContentView max=3）缓存下重进走 onActivated 而非 onMounted，数据拉取统一收口于此
let unlistenDone: (() => void) | null = null
// brew-run-done 已触发标记：事件与 invoke 响应投递通道不同、无顺序保证，查询返回过期
// Some（操作在 Rust 侧读取后结束、done 先于响应到达）时丢弃恢复，防运行态卡死
let doneSeen = false

async function ensureDoneListener() {
  if (unlistenDone) return
  // 常驻到组件卸载：先注册监听再查状态，消除「查询返回 Some → 操作恰在此间隙结束 → 事件无人接收」的 TOCTOU 竞态
  unlistenDone = await listen<BrewRunState | null>('brew-run-done', async () => {
    doneSeen = true
    running.value = false
    runningStep.value = ''
    runningOperation.value = ''
    refreshing.value = false
    loading.value = false
    await fetchStatus()
  })
}

onActivated(async () => {
  if (!isTauri) {
    loading.value = false
    return
  }
  if (running.value) return
  await ensureDoneListener()
  doneSeen = false
  // 查状态：null = 无操作（含后台元数据刷新）进行中；Some = 仍在运行（LRU 驱逐/窗口隐藏后重挂载恢复进度）
  const state = await invoke<BrewRunState | null>(CMD.brewRunState)
  if (state && !doneSeen) {
    // 仍在运行：恢复运行态并照常拉数据（brew_status 只读命令，与运行中操作并发安全），
    // 完成事件统一重拉——不阻断为加载态等后台操作结束
    running.value = true
    runningStep.value = state.step
    if (state.operation === 'update_upgrade') {
      // 更新进行中：按钮位滚动恢复当前步骤
      runningOperation.value = 'update_upgrade'
    }
  }
  await fetchStatus()
})

onUnmounted(() => {
  unlistenDone?.()
  unlistenDone = null
})
</script>

<style scoped>
/* 文案宽度盒：常驻 width transition 承接 FLIP 的 px→px 插值；overflow hidden 同时
 * 裁剪滚动的进出场位移 */
.brew-label-box {
  display: block;
  overflow: hidden;
  transition: width var(--duration-fast) var(--ease-out);
}

/* 滚动文案：inline-block 使宽度盒可量宽、transform 可作用于自身 */
.brew-label {
  display: inline-block;
  white-space: nowrap;
}

/* 更新按钮滚动详情：文案切换纵向滚动（旧文上出、新文下入），仅 transform/opacity
 * GPU 合成属性，时长/曲线走 token（同 ui-popup 族语义，位移幅度更收敛） */
.brew-roll-enter-active {
  transition:
    transform var(--duration-fast) var(--ease-out),
    opacity var(--duration-fast) var(--ease-out);
}
.brew-roll-leave-active {
  transition:
    transform var(--duration-fastest) var(--ease-in),
    opacity var(--duration-fastest) var(--ease-in);
}
.brew-roll-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.brew-roll-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
