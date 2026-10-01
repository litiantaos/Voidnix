<template>
  <div class="flex-col-full-pb">
    <BaseEmptyState v-if="routable.length === 0" :title="t('ai-gateway.emptyRoute')">
      <template #action>
        <BaseButton variant="primary" icon="i-ri-key-2-line" @click="goAiProviders">
          {{ t('common.goConfigure') }}
        </BaseButton>
      </template>
    </BaseEmptyState>
    <BaseSettingsList v-else :items="items" />
  </div>
</template>

<script setup lang="ts">
import { computed, onActivated } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { t } from '@/runtime/i18n'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import type { SettingItem, SettingSelectOptions } from '@/types/settings'
import { config, type AliasSelection } from './config'
import { gatewayStatus, ccApplyError, currentRoutes, type GatewayStatus } from './sync'
import { routableModels, anthropicModels } from './logic'

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

/** 设置项三组:总开关(副标题即运行态,绑定失败标红) → CC(接管开关 + 默认模型) → 全工具状态展示;使用指引走搜索栏说明弹窗 */
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
        config.enabled = v
      },
    },
  ]

  // Claude Code(低频,CC 专属的接线细节):接管开关在前,开启才改写其 settings.json;
  // 别名两档仅接管开启时展示(关了无消费者),不限制 /model 范围
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
      config.ccTakeover = v
    },
  })
  if (config.ccTakeover) {
    out.push({
      id: 'cc-1m-context',
      group: defaultsGroup,
      type: 'toggle',
      title: t('ai-gateway.cc1mContext'),
      subtitle: t('ai-gateway.cc1mContextHint'),
      icon: 'i-ri-expand-width-line',
      value: config.cc1mContext,
      update: (v) => {
        config.cc1mContext = v
      },
    })
    const aliasDefs: { id: string; title: string; sel: AliasSelection | null }[] = [
      { id: 'sonnet', title: t('ai-gateway.aliasDefault'), sel: config.sonnet },
      { id: 'haiku', title: t('ai-gateway.aliasBackground'), sel: config.haiku },
    ]
    for (const def of aliasDefs) {
      out.push({
        id: `alias-${def.id}`,
        group: defaultsGroup,
        type: 'select',
        title: def.title,
        subtitle: def.id === 'sonnet' ? t('ai-gateway.defaultsHint') : undefined,
        icon: 'i-ri-cpu-line',
        value: aliasValue(def.sel),
        options:
          aliasOptions.value.length > 0
            ? aliasOptions.value
            : [{ label: t('ai-gateway.noAnthropicModel'), value: '' }],
        update: (v) => {
          const parsed = parseAlias(v)
          if (def.id === 'sonnet') config.sonnet = parsed
          else config.haiku = parsed
        },
      })
    }
  }

  // 可路由模型(状态展示,非操作)
  const routeGroup = t('ai-gateway.sectionRoutes')
  for (const m of routable.value) {
    const protocols = [
      m.anthropic ? 'Anthropic' : '',
      m.responses ? 'Responses' : '',
      m.chat ? 'Chat' : '',
    ]
      .filter(Boolean)
      .join(' / ')
    const keys = currentRoutes().find((r) => r.providerId === m.providerId)?.keys.length ?? 0
    out.push({
      id: `route-${m.providerId}-${m.model}`,
      group: routeGroup,
      type: 'custom',
      title: m.model,
      subtitle: `${m.providerName} · ${protocols} · ${t('ai-gateway.keysCount', { n: keys })}`,
      icon: 'i-ri-route-line',
    })
  }

  return out
})

async function refreshStatus() {
  try {
    gatewayStatus.value = await invoke<GatewayStatus>(CMD.aiGatewayStatus)
  } catch (e) {
    console.error('[ai-gateway] status failed:', e)
  }
}

function goAiProviders() {
  appStore.setActiveExtension('ai-providers')
}

// KeepAlive 内 onActivated 首挂载同样触发,无需 onMounted 双发
onActivated(() => void refreshStatus())
</script>
