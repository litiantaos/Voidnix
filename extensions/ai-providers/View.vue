<template>
  <div class="flex-col-full-pb">
    <BaseEmptyState v-if="keyRows.length === 0" title="请添加 AI 提供商" icon="i-ri-key-2-line" />

    <BaseList
      v-else
      :items="keyRows"
      v-model:selected-index="selectedIndex"
      group-field="group"
      :group-title="groupTitle"
      :keyboard-active="!panelOpen && !showProviderModal && !showKeyModal"
      @execute="onExecute"
    >
      <template #group-title="{ group }">
        <span class="flex-1 min-w-0 truncate">{{ groupTitle(group) }}</span>
        <BaseButton
          icon="i-ri-settings-3-line"
          title="编辑提供商"
          @click.stop="openEditProvider(group)"
        />
        <BaseButton icon="i-ri-add-line" title="添加 Key" @click.stop="openCreateKey(group)" />
      </template>

      <template #item="{ item }">
        <!-- 不用 BaseListItem trailing：hasTrailing 对异步 slot 不刷新，曲线永远不挂载 -->
        <div flex p="3" gap="3" select="none" items="center" text="primary">
          <div flex="~ col 1" min-w="0" justify="center">
            <div text="sm" font="medium" class="truncate">{{ itemTitle(item) }}</div>
            <div text="xs" class="truncate">
              <template v-for="(seg, i) in usageSegments(item)" :key="i">
                <span v-if="i > 0" :class="{ 'opacity-50': !seg.lead }">{{
                  seg.lead ? ' ' : ' · '
                }}</span>
                <span :style="{ color: TONE_COLOR[seg.tone] }">{{ seg.text }}</span>
              </template>
            </div>
          </div>
          <SparkLine
            v-if="sparkSeries(item).length > 1"
            :data="sparkSeries(item)"
            :max="sparkMax"
            :width="64"
            :height="28"
          />
        </div>
      </template>
    </BaseList>

    <!-- Cmd+Enter 动作面板（粘贴 / 删除） -->
    <Teleport to="body">
      <Transition
        appear
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="opacity-0 translate-y-2 scale-95"
        enter-to-class="opacity-100 translate-y-0 scale-100"
        leave-active-class="transition duration-100 ease-in"
        leave-from-class="opacity-100 translate-y-0 scale-100"
        leave-to-class="opacity-0 translate-y-2 scale-95"
      >
        <div
          v-if="panelOpen"
          ref="panelRef"
          tabindex="-1"
          class="dropdown-panel outline-none bottom-3 right-3 fixed z-50"
          role="menu"
        >
          <BaseDropdownItems
            :items="actionMenuItems"
            :active-index="menuIndex"
            @select="onMenuClick"
            @hover="(i: number) => (menuIndex = i)"
          />
        </div>
      </Transition>
    </Teleport>

    <!-- 提供商：URL + 模型（创建时带首把 Key） -->
    <BaseDialog
      v-if="showProviderModal"
      :title="providerModalMode === 'create' ? '添加提供商' : '编辑提供商'"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveProvider"
      @cancel="showProviderModal = false"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">名称</span>
          <BaseInput v-model="providerForm.name" :placeholder="nameFieldPlaceholder" />
        </div>

        <div class="form-field">
          <span class="form-label">API URL</span>
          <BaseInput v-model="providerForm.endpoint" :placeholder="EXAMPLE_ENDPOINT" />
        </div>

        <div class="form-field">
          <span class="form-label">模型 ID</span>
          <div flex="~ col" gap="1.5">
            <div v-for="(_, i) in providerForm.models" :key="i" flex gap="1.5" items="center">
              <BaseInput v-model="providerForm.models[i]" placeholder="模型 ID" class="flex-1" />
              <BaseButton
                v-if="i > 0"
                class="text-danger"
                icon="i-ri-close-line"
                @click="providerForm.models.splice(i, 1)"
              />
              <BaseButton v-else icon="i-ri-add-line" @click="providerForm.models.push('')" />
            </div>
          </div>
        </div>

        <template v-if="providerModalMode === 'create'">
          <div class="form-field">
            <span class="form-label">备注</span>
            <BaseInput v-model="providerForm.firstKeyLabel" placeholder="主号 / 备用" />
          </div>
          <div class="form-field">
            <span class="form-label">API Key</span>
            <BaseInput
              v-model="providerForm.firstKey"
              :type="createKeyVisible ? 'text' : 'password'"
              placeholder="sk-..."
            >
              <template #suffix>
                <BaseButton
                  variant="ghost"
                  :icon="createKeyVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
                  class="!text-muted !px-1 !shrink-0 !h-auto"
                  @click.stop="createKeyVisible = !createKeyVisible"
                />
              </template>
            </BaseInput>
          </div>
        </template>
      </div>

      <template #footer-start>
        <BaseButton
          v-if="providerModalMode === 'edit' && editingProviderId"
          variant="danger"
          @click="removeProviderAndClose"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>

    <!-- 单把 Key 编辑 -->
    <BaseDialog
      v-if="showKeyModal"
      :title="keyModalMode === 'create' ? '添加 Key' : '编辑 Key'"
      variant="form"
      size="sm"
      show-footer
      ok-label="保存"
      @confirm="saveKey"
      @cancel="showKeyModal = false"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">备注</span>
          <BaseInput v-model="keyForm.label" placeholder="主号 / 备用" />
        </div>
        <div class="form-field">
          <span class="form-label">API Key</span>
          <BaseInput
            v-model="keyForm.apiKey"
            :type="keyEditVisible ? 'text' : 'password'"
            placeholder="sk-..."
          >
            <template #suffix>
              <BaseButton
                variant="ghost"
                :icon="keyEditVisible ? 'i-ri-eye-off-line' : 'i-ri-eye-line'"
                class="!text-muted !px-1 !shrink-0 !h-auto"
                @click.stop="keyEditVisible = !keyEditVisible"
              />
            </template>
          </BaseInput>
        </div>
      </div>

      <template #footer-start>
        <BaseButton
          v-if="keyModalMode === 'edit' && canDeleteKey"
          variant="danger"
          @click="removeKeyAndClose"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import {
  ref,
  computed,
  reactive,
  watch,
  onMounted,
  onUnmounted,
  onActivated,
  onDeactivated,
  type Ref,
} from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { showToast } from '@/composables/useToast'
import { useActionPanel } from '@/composables/useActionPanel'
import { CMD } from '@/commands'
import {
  config,
  type AiKeySlot,
  addAiProvider,
  removeAiProvider,
  updateAiProvider,
  pasteOut,
  resolveUsageKind,
  newKeySlot,
  addKeyToProvider,
  removeKeyFromProvider,
  getProviderById,
  providerDisplayName,
} from '@/runtime/ai-providers'
import { providerLabelFromUrl, toErrorMessage } from '@/utils/format'
import {
  maskKey,
  normalizeZhipuMonitor,
  normalizeDeepseekBalance,
  buildZhipuUsageSegments,
  buildDeepseekUsageSegments,
  type KeyMonitor,
  type UsageTone,
  type UsageSegment,
} from './logic'
import { createProviderTick } from './bridge'
import BaseList from '@/components/ui/BaseList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseDropdownItems, { type PanelItem } from '@/components/ui/BaseDropdownItems.vue'
import SparkLine from './SparkLine.vue'

// ─── 行模型：一把 Key 一项；group = providerId（标题显示名称，空则域名）──

interface KeyRow {
  id: string
  type: 'key'
  group: string
  providerId: string
  slot: AiKeySlot
}

const selectedIndex = ref(0)
/** 缓存 key = `${providerId}:${slotId}`，避免跨提供商 slot.id 碰撞 */
const monitorByKeyId = reactive<Record<string, KeyMonitor>>({})
const loadingByKey = reactive<Record<string, boolean>>({})
const nowMs = ref(Date.now())
let countdownTimer: ReturnType<typeof setInterval> | null = null
/** 监控拉取世代：快速改配置时丢弃过期响应 */
let monitorFetchGen = 0

function monitorCacheKey(providerId: string, slotId: string): string {
  return `${providerId}:${slotId}`
}

const keyRows = computed<KeyRow[]>(() => {
  const out: KeyRow[] = []
  for (const p of config.providers) {
    for (const slot of p.keys ?? []) {
      out.push({
        id: monitorCacheKey(p.id, slot.id),
        type: 'key',
        group: p.id,
        providerId: p.id,
        slot,
      })
    }
  }
  return out
})

/** 删 Key / 提供商后清监控缓存，避免 stale series 抬高 sparkMax */
watch(
  () => keyRows.value.map((r) => r.id).join('\0'),
  () => {
    const live = new Set(keyRows.value.map((r) => r.id))
    for (const id of Object.keys(monitorByKeyId)) {
      if (!live.has(id)) delete monitorByKeyId[id]
    }
    for (const id of Object.keys(loadingByKey)) {
      if (!live.has(id)) delete loadingByKey[id]
    }
  },
)

/** 分组标题 = 名称（空则域名） */
function groupTitle(providerId: string): string {
  const p = getProviderById(providerId)
  if (!p) return '未命名提供商'
  return providerDisplayName(p)
}

function monitorOf(row: KeyRow): KeyMonitor | undefined {
  return monitorByKeyId[row.id]
}

function itemTitle(row: KeyRow): string {
  const label = row.slot.label || 'Key'
  const m = monitorOf(row)
  // 智谱档位进副标题（MAX · 5h…）；DeepSeek 余额不足仍挂标题
  if (m?.kind === 'deepseek' && !m.error && !m.isAvailable) {
    return `${label} · 余额不足`
  }
  return label
}

/** 副标题片段（按 monitor.kind 分派）；无 monitor 走 muted 兜底。 */
function usageSegments(row: KeyRow): UsageSegment[] {
  const m = monitorOf(row)
  if (loadingByKey[row.id] && !m) {
    return [
      { text: maskKey(row.slot.apiKey) || '无 Key', tone: 'muted' },
      { text: '获取用量信息中…', tone: 'muted' },
    ]
  }
  if (m?.kind === 'deepseek') return buildDeepseekUsageSegments(row.slot.apiKey, m)
  if (m?.kind === 'zhipu') return buildZhipuUsageSegments(row.slot.apiKey, m, nowMs.value)
  return buildZhipuUsageSegments(row.slot.apiKey, undefined, nowMs.value)
}

/**
 * 片段 tone → CSS color；走 theme.css 变量，accent 用字面值对齐 SparkLine（SVG var 不可靠）。
 * 与 theme.css / uno.config.ts 同源，改 accent 色时同步。
 */
const TONE_COLOR: Record<UsageTone, string> = {
  muted: 'var(--color-text-muted)',
  secondary: 'var(--color-text-secondary)',
  primary: 'var(--color-text-primary)',
  accent: '#3d82f0',
  warning: 'var(--color-warning)',
  danger: 'var(--color-danger)',
  success: 'var(--color-success)',
}

function sparkSeries(row: KeyRow): number[] {
  const m = monitorOf(row)
  if (m?.kind !== 'zhipu') return []
  const s = m.tokensSeries
  if (!s || s.length === 0) return []
  // 全 0 仍画一条底线可见；过滤 NaN
  return s.map((v) => (Number.isFinite(v) ? v : 0))
}

/** 多 Key sparkline 共用 max，便于对比（仅智谱序列） */
const sparkMax = computed(() => {
  let max = 1
  for (const m of Object.values(monitorByKeyId)) {
    if (m.kind !== 'zhipu') continue
    for (const v of m.tokensSeries ?? []) {
      if (v > max) max = v
    }
  }
  return max
})

// ─── 粘贴 ───────────────────────────────────────────────────

async function pasteField(text: string, label: string) {
  const t = text.trim()
  if (!t) {
    showToast(`${label} 为空`, { kind: 'error' })
    return
  }
  try {
    await pasteOut(t)
  } catch (e) {
    showToast(toErrorMessage(e, '粘贴失败'), { kind: 'error' })
  }
}

function onExecute(row: KeyRow) {
  openEditKey(row)
}

// ─── Cmd+Enter 动作面板 ─────────────────────────────────────

const panelRef = ref<HTMLElement>()
const actionTarget = ref<KeyRow | null>(null)

const actionMenuItems = computed<PanelItem[]>(() => {
  const row = actionTarget.value
  if (!row) return []
  const p = getProviderById(row.providerId)
  const models = p?.models.filter((m) => m.trim()) ?? []
  // 统一「粘贴 + 对象」；多模型时每条模型名本身即对象
  const items: PanelItem[] = [
    {
      type: 'item',
      key: 'paste-key',
      label: '粘贴 Key',
      icon: 'i-ri-key-2-line',
      disabled: !row.slot.apiKey.trim(),
    },
    {
      type: 'item',
      key: 'paste-url',
      label: '粘贴 URL',
      icon: 'i-ri-links-line',
      disabled: !p?.endpoint.trim(),
    },
  ]
  if (models.length === 0) {
    items.push({
      type: 'item',
      key: 'paste-model',
      label: '粘贴 模型',
      icon: 'i-ri-cpu-line',
      disabled: true,
    })
  } else {
    for (const m of models) {
      items.push({
        type: 'item',
        key: `paste-model:${m}`,
        label: `粘贴 ${m}`,
        icon: 'i-ri-cpu-line',
      })
    }
  }
  if ((p?.keys.length ?? 0) > 1) {
    items.push(
      { type: 'divider' },
      {
        type: 'item',
        key: 'delete-key',
        label: '删除 Key',
        icon: 'i-ri-delete-bin-line',
        danger: true,
      },
    )
  }
  return items
})

const {
  open: panelOpen,
  menuIndex,
  close: closePanel,
  onMenuClick,
} = useActionPanel({
  panelRef: panelRef as Ref<HTMLElement | undefined>,
  getItems: () => actionMenuItems.value,
  shouldOpen: (e) => {
    if (showProviderModal.value || showKeyModal.value) return false
    if (keyRows.value.length === 0) return false
    e.preventDefault()
    e.stopPropagation()
    return true
  },
  beforeOpen: () => {
    actionTarget.value = keyRows.value[selectedIndex.value] ?? null
  },
  onSelect: (key) => {
    const row = actionTarget.value
    closePanel()
    if (!row || key == null) return
    const k = String(key)
    const p = getProviderById(row.providerId)
    if (k === 'paste-key') void pasteField(row.slot.apiKey, 'Key')
    else if (k === 'paste-url' && p) void pasteField(p.endpoint, 'URL')
    else if (k.startsWith('paste-model:')) void pasteField(k.slice('paste-model:'.length), '模型')
    else if (k === 'delete-key') {
      removeKeyFromProvider(row.providerId, row.slot.id)
      showToast('已删除 Key')
    }
  },
})

// ─── 提供商弹窗 ─────────────────────────────────────────────

const showProviderModal = ref(false)
const providerModalMode = ref<'create' | 'edit'>('create')
const editingProviderId = ref('')
const createKeyVisible = ref(false)
const providerForm = ref({
  name: '',
  endpoint: '',
  models: [''] as string[],
  firstKeyLabel: '默认',
  firstKey: '',
})

/** 与列表展示一致：名称留空时 = URL 推导域名（空 URL 用示例 endpoint → OPENAI） */
const EXAMPLE_ENDPOINT = 'https://api.openai.com/v1'
const nameFieldPlaceholder = computed(() =>
  providerLabelFromUrl(providerForm.value.endpoint.trim() || EXAMPLE_ENDPOINT, 'OPENAI'),
)

function openCreateProvider() {
  providerModalMode.value = 'create'
  editingProviderId.value = ''
  createKeyVisible.value = false
  providerForm.value = {
    name: '',
    endpoint: '',
    models: [''],
    firstKeyLabel: '默认',
    firstKey: '',
  }
  showProviderModal.value = true
}

watch(createProviderTick, () => {
  openCreateProvider()
})

function defaultProviderName(endpoint: string): string {
  return providerLabelFromUrl(endpoint.trim(), '')
}

function openEditProvider(providerId: string) {
  const p = getProviderById(providerId)
  if (!p) return
  providerModalMode.value = 'edit'
  editingProviderId.value = p.id
  // 已配置但未写 name：表单直接填域名默认值
  providerForm.value = {
    name: p.name.trim() || defaultProviderName(p.endpoint),
    endpoint: p.endpoint,
    models: p.models.length ? [...p.models] : [''],
    firstKeyLabel: '',
    firstKey: '',
  }
  showProviderModal.value = true
}

function saveProvider() {
  const models = providerForm.value.models.map((m) => m.trim()).filter(Boolean)
  const endpoint = providerForm.value.endpoint.trim()
  // 有 URL 时名称空则落盘域名默认，避免一直「虚」占位
  const name = providerForm.value.name.trim() || defaultProviderName(endpoint)

  if (providerModalMode.value === 'create') {
    const apiKey = providerForm.value.firstKey.trim()
    if (!endpoint) {
      showToast('请填写 API URL', { kind: 'error' })
      return
    }
    if (!apiKey) {
      showToast('请填写 API Key', { kind: 'error' })
      return
    }
    const label = providerForm.value.firstKeyLabel.trim() || '默认'
    const slot = newKeySlot(label, apiKey)
    addAiProvider({
      name,
      endpoint,
      models,
      keys: [slot],
    })
  } else if (editingProviderId.value) {
    if (!endpoint) {
      showToast('请填写 API URL', { kind: 'error' })
      return
    }
    updateAiProvider(editingProviderId.value, { name, endpoint, models })
  }
  showProviderModal.value = false
}

function removeProviderAndClose() {
  if (!editingProviderId.value) return
  const id = editingProviderId.value
  showProviderModal.value = false
  removeAiProvider(id)
}

// ─── Key 弹窗 ───────────────────────────────────────────────

const showKeyModal = ref(false)
const keyModalMode = ref<'create' | 'edit'>('create')
const keyEditVisible = ref(false)
const keyFormProviderId = ref('')
const keyFormKeyId = ref('')
const keyForm = ref({ label: '', apiKey: '' })

const canDeleteKey = computed(() => {
  const p = getProviderById(keyFormProviderId.value)
  return (p?.keys.length ?? 0) > 1
})

function openCreateKey(providerId: string) {
  if (!providerId) return
  keyModalMode.value = 'create'
  keyFormProviderId.value = providerId
  keyFormKeyId.value = ''
  keyEditVisible.value = false
  const p = getProviderById(providerId)
  const n = (p?.keys.length ?? 0) + 1
  keyForm.value = { label: `Key ${n}`, apiKey: '' }
  showKeyModal.value = true
}

function openEditKey(row: KeyRow) {
  keyModalMode.value = 'edit'
  keyFormProviderId.value = row.providerId
  keyFormKeyId.value = row.slot.id
  keyEditVisible.value = false
  keyForm.value = { label: row.slot.label, apiKey: row.slot.apiKey }
  showKeyModal.value = true
}

function saveKey() {
  const pid = keyFormProviderId.value
  const p = getProviderById(pid)
  if (!p) {
    showKeyModal.value = false
    return
  }
  const label = keyForm.value.label.trim() || 'Key'
  const apiKey = keyForm.value.apiKey

  if (keyModalMode.value === 'create') {
    const id = addKeyToProvider(pid, label)
    const slot = p.keys.find((k) => k.id === id)
    if (slot) slot.apiKey = apiKey
  } else {
    const slot = p.keys.find((k) => k.id === keyFormKeyId.value)
    if (slot) {
      slot.label = label
      slot.apiKey = apiKey
    }
  }
  showKeyModal.value = false
  void refreshAllMonitors()
}

function removeKeyAndClose() {
  if (!canDeleteKey.value) return
  removeKeyFromProvider(keyFormProviderId.value, keyFormKeyId.value)
  showKeyModal.value = false
}

// ─── 列表项额度/余额拉取 ────────────────────────────────────

async function fetchZhipuForSlot(cacheKey: string, apiKey: string, gen: number) {
  if (!apiKey.trim()) return
  loadingByKey[cacheKey] = true
  try {
    const raw = await invoke<Record<string, unknown>>(CMD.aiProvidersZhipuQuota, {
      apiKey: apiKey.trim(),
    })
    if (gen !== monitorFetchGen) return
    const mon = normalizeZhipuMonitor(raw)
    mon.tokensSeries = Array.isArray(mon.tokensSeries)
      ? mon.tokensSeries.map((n) => (Number.isFinite(n) ? n : 0))
      : []
    monitorByKeyId[cacheKey] = mon
  } catch (e) {
    if (gen !== monitorFetchGen) return
    monitorByKeyId[cacheKey] = {
      kind: 'zhipu',
      level: 'unknown',
      expired: false,
      totalCalls: 0,
      totalTokens: 0,
      tokensSeries: [],
      error: e instanceof Error ? e.message : String(e),
    }
  } finally {
    if (gen === monitorFetchGen) loadingByKey[cacheKey] = false
  }
}

async function fetchDeepseekForSlot(
  cacheKey: string,
  apiKey: string,
  endpoint: string,
  gen: number,
) {
  if (!apiKey.trim()) return
  loadingByKey[cacheKey] = true
  try {
    const raw = await invoke<Record<string, unknown>>(CMD.aiProvidersDeepseekBalance, {
      apiKey: apiKey.trim(),
      endpoint: endpoint.trim(),
    })
    if (gen !== monitorFetchGen) return
    monitorByKeyId[cacheKey] = normalizeDeepseekBalance(raw)
  } catch (e) {
    if (gen !== monitorFetchGen) return
    monitorByKeyId[cacheKey] = {
      kind: 'deepseek',
      isAvailable: false,
      balanceInfos: [],
      error: e instanceof Error ? e.message : String(e),
    }
  } finally {
    if (gen === monitorFetchGen) loadingByKey[cacheKey] = false
  }
}

async function refreshAllMonitors() {
  const gen = ++monitorFetchGen
  // 指纹变化时清掉无监控类型的旧缓存（如 DeepSeek→OpenAI）
  const liveKeys = new Set<string>()
  const tasks: Promise<void>[] = []
  for (const p of config.providers) {
    const kind = resolveUsageKind(p)
    for (const slot of p.keys ?? []) {
      if (!slot.apiKey.trim()) continue
      const ck = monitorCacheKey(p.id, slot.id)
      if (kind === 'zhipu-coding-plan') {
        liveKeys.add(ck)
        tasks.push(fetchZhipuForSlot(ck, slot.apiKey, gen))
      } else if (kind === 'deepseek-balance') {
        liveKeys.add(ck)
        tasks.push(fetchDeepseekForSlot(ck, slot.apiKey, p.endpoint, gen))
      }
    }
  }
  // 非监控端点：立刻清掉该 key 的旧 monitor（不依赖 key 删除）
  if (gen === monitorFetchGen) {
    for (const id of Object.keys(monitorByKeyId)) {
      if (!liveKeys.has(id)) delete monitorByKeyId[id]
    }
    for (const id of Object.keys(loadingByKey)) {
      if (!liveKeys.has(id)) delete loadingByKey[id]
    }
  }
  await Promise.all(tasks)
}

watch(
  () =>
    config.providers
      .map((p) => {
        const kind = resolveUsageKind(p)
        const keys = (p.keys ?? []).map((k) => `${k.id}:${k.apiKey}`).join(',')
        return `${p.id}|${p.endpoint}|${kind}|${keys}`
      })
      .join('\n'),
  () => {
    void refreshAllMonitors()
  },
  { immediate: true },
)

watch(
  () => keyRows.value.length,
  (n) => {
    if (selectedIndex.value >= n) selectedIndex.value = Math.max(0, n - 1)
  },
)

onMounted(() => {
  startCountdown()
})
onActivated(() => {
  startCountdown()
})
onDeactivated(() => {
  stopCountdown()
})
onUnmounted(() => {
  stopCountdown()
})

function startCountdown() {
  if (countdownTimer) return
  countdownTimer = setInterval(() => {
    nowMs.value = Date.now()
  }, 30_000)
}

function stopCountdown() {
  if (countdownTimer) {
    clearInterval(countdownTimer)
    countdownTimer = null
  }
}
</script>
