<template>
  <div class="flex-col-full-pb">
    <BaseEmptyState v-if="routable.length === 0" :title="t('ai-gateway.emptyRoute')">
      <template #action>
        <BaseButton variant="primary" icon="i-ri-key-2-line" @click="goAiProviders">
          {{ t('common.goConfigure') }}
        </BaseButton>
      </template>
    </BaseEmptyState>
    <template v-else>
      <!-- 活跃链路:提供商(模型)→ 网关 → 消费者,事件驱动粒子;内存事件流、重启清零;
           空态左列陈列路由表模型(待命),零请求也有信息量 -->
      <section m="x-3" p="3" class="soft-card" style="border-radius: var(--radius-panel)">
        <FlowStage
          :events="usageEvents"
          :port="gatewayStatus?.port ?? 8788"
          :running="gatewayStatus?.running ?? false"
          :models="routeModels"
        />
      </section>
      <BaseSettingsList :items="items" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { t } from '@/runtime/i18n'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import FlowStage from './FlowStage.vue'
import type { SettingItem, SettingSelectOptions } from '@/types/settings'
import { config, type AliasSelection, type ContextTier } from './config'
import { gatewayStatus, ccApplyError, currentRoutes, type GatewayStatus } from './sync'
import { routableModels, anthropicModels, stripContext } from './logic'

const appStore = useAppStore()

const routable = computed(() => routableModels(currentRoutes()))
const anthropic = computed(() => anthropicModels(routable.value))

/** 别名下拉:按提供商分组的 Anthropic 可路由模型;value = `providerId::model` */
const aliasOptions = computed<SettingSelectOptions>(() => {
  const groups = new Map<string, { label: string; value: string }[]>()
  for (const m of anthropic.value) {
    const list = groups.get(m.providerName) ?? []
    list.push({ label: m.model, value: `${m.providerId}::${m.model}` })
    groups.set(m.providerName, list)
  }
  return [...groups.entries()].map(([label, options]) => ({ label, options }))
})

function parseAlias(value: string | number): AliasSelection | null {
  const s = String(value)
  const [providerId, model] = s.split('::')
  return providerId && model ? { providerId, model } : null
}

function aliasValue(sel: AliasSelection | null): string {
  return sel ? `${sel.providerId}::${sel.model}` : ''
}

const statusTitle = computed(() => {
  const s = gatewayStatus.value
  if (!s) return t('ai-gateway.statusLoading')
  if (s.bindError) return t('ai-gateway.statusError', { msg: s.bindError })
  if (s.running) return t('ai-gateway.statusRunning', { port: s.port, n: s.routeCount })
  return t('ai-gateway.statusStopped')
})

/** 使用事件流(Rust 侧最新在前),驱动活跃链路拓扑 */
const usageEvents = computed(() => gatewayStatus.value?.usage ?? [])

/** 路由表模型(空态左列待命陈列);stripContext 归一与 Rust 记录侧(norm_model)
 * 同语义——中枢可带 [Nm] 后缀存储,不归一则空态/活跃节点同模型不同 key,切换时
 * 节点错位重挂 + 首事件粒子丢失 */
const routeModels = computed(() =>
  routable.value.map((m) => ({ model: stripContext(m.model), provider: m.providerName })),
)

/** 设置项两组:网关(总开关 + 提供商导航入口,副标题即运行态、绑定失败标红) → CC(接管开关 + 形态与三档映射);使用指引走搜索栏说明弹窗 */
const items = computed<SettingItem[]>(() => {
  const s = gatewayStatus.value
  const out: SettingItem[] = [
    {
      id: 'enabled',
      group: t('ai-gateway.sectionGateway'),
      type: 'toggle',
      title: t('ai-gateway.enable'),
      subtitle: statusTitle.value,
      tone: s?.bindError ? 'danger' : undefined,
      icon: s?.running ? 'i-ri-broadcast-line' : 'i-ri-stop-circle-line',
      value: config.enabled,
      update: (v) => {
        // 关网关 = 停整个网关功能,接管开关连带还原(不变式:ccTakeover 开 ⇒ enabled 开)
        config.enabled = v
        if (!v) config.ccTakeover = false
      },
    },
    {
      id: 'go-providers',
      group: t('ai-gateway.sectionGateway'),
      type: 'action',
      title: t('ai-gateway.goProviders'),
      subtitle: t('ai-gateway.goProvidersHint'),
      icon: 'i-ri-key-2-line',
      action: goAiProviders,
    },
  ]

  // Claude Code(低频,CC 专属的接线细节):接管开关在前,开启才改写其 settings.json;
  // 形态与两档映射仅接管开启时展示(关了无消费者),不限制 /model 范围
  const defaultsGroup = t('ai-gateway.sectionCc')
  out.push({
    id: 'cc-takeover',
    group: defaultsGroup,
    type: 'toggle',
    title: t('ai-gateway.ccTakeover'),
    subtitle: ccApplyError.value
      ? t('ai-gateway.ccError', { msg: ccApplyError.value })
      : t('ai-gateway.ccTakeoverHint'),
    tone: ccApplyError.value ? 'danger' : undefined,
    icon: 'i-ri-terminal-box-line',
    value: config.ccTakeover,
    update: (v) => {
      // 接管依赖网关运行:开启即连带启用,消除「接管开着而网关关着」的虚假态
      config.ccTakeover = v
      if (v) config.enabled = true
    },
  })
  if (config.ccTakeover) {
    out.push({
      id: 'cc-context',
      group: defaultsGroup,
      type: 'select',
      title: t('ai-gateway.ccContext'),
      subtitle: t('ai-gateway.ccContextHint'),
      icon: 'i-ri-expand-width-line',
      value: config.ccContext,
      options: [
        { label: t('ai-gateway.ccContextDefault'), value: 'default' },
        { label: '1M', value: '1m' },
      ],
      update: (v) => {
        config.ccContext = v as ContextTier
      },
    })
    const aliasDefs: { id: 'sonnet' | 'opus' | 'haiku'; title: string; hint: string }[] = [
      { id: 'sonnet', title: t('ai-gateway.aliasSonnet'), hint: t('ai-gateway.aliasSonnetHint') },
      { id: 'opus', title: t('ai-gateway.aliasOpus'), hint: t('ai-gateway.aliasOpusHint') },
      { id: 'haiku', title: t('ai-gateway.aliasHaiku'), hint: t('ai-gateway.aliasHaikuHint') },
    ]
    for (const def of aliasDefs) {
      out.push({
        id: `alias-${def.id}`,
        group: defaultsGroup,
        type: 'select',
        title: def.title,
        subtitle: def.hint,
        icon: 'i-ri-cpu-line',
        value: aliasValue(config[def.id]),
        options:
          aliasOptions.value.length > 0
            ? aliasOptions.value
            : [{ label: t('ai-gateway.noAnthropicModel'), value: '' }],
        update: (v) => {
          config[def.id] = parseAlias(v)
        },
      })
    }
  }

  return out
})

// ── 使用统计轮询(双门控:组件激活 AND 窗口聚焦,参照 system-status) ──
// 窗口隐藏时 onDeactivated 不触发(仅切扩展触发),需监听 focus 避免后台持续轮询

const POLL_INTERVAL = 3000
let pollTimer: number | undefined
let polling = false
let viewActivated = false
let windowFocused = false
let unlistenFocus: (() => void) | undefined

async function refreshStatus() {
  if (!isTauri || polling) return
  polling = true
  try {
    gatewayStatus.value = await invoke<GatewayStatus>(CMD.aiGatewayStatus)
  } catch (e) {
    console.error('[ai-gateway] status failed:', e)
  } finally {
    polling = false
  }
}

function syncPolling() {
  if (viewActivated && windowFocused) {
    if (pollTimer === undefined) {
      void refreshStatus()
      pollTimer = window.setInterval(() => void refreshStatus(), POLL_INTERVAL)
    }
  } else if (pollTimer !== undefined) {
    clearInterval(pollTimer)
    pollTimer = undefined
  }
}

/** 带 from 导航：providers 内 Esc 返回本扩展 */
function goAiProviders() {
  appStore.setActiveExtension('ai-providers', 'ai-gateway')
}

onMounted(() => {
  if (!isTauri) return
  // 实测页面焦点:navigate 重载后 View 可能在窗口隐藏态重挂载(app store 经
  // sessionStorage 恢复 activeExtId),硬编码 true 会让双门控在整个隐藏期失效
  windowFocused = document.hasFocus()
  getCurrentWindow()
    .onFocusChanged(({ payload: focused }) => {
      windowFocused = focused
      syncPolling()
    })
    .then((fn) => {
      unlistenFocus = fn
    })
})
// KeepAlive 内 onActivated 首挂载同样触发,无需 onMounted 双发
onActivated(() => {
  viewActivated = true
  syncPolling()
})
onDeactivated(() => {
  viewActivated = false
  syncPolling()
})
onBeforeUnmount(() => {
  if (pollTimer !== undefined) clearInterval(pollTimer)
  unlistenFocus?.()
})
</script>
