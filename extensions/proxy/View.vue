<template>
  <div>
    <!-- 实时流量：上下行速度从开启项副标题独立成面板（容器同 ai-gateway 拓扑图） -->
    <section
      v-if="isEnabled"
      m="x-3"
      mb="3"
      p="3"
      class="soft-card"
      style="border-radius: var(--radius-panel)"
    >
      <div grid="~ cols-2">
        <div flex="~ col" pr="3">
          <div flex items="center" gap="2">
            <span text="xs muted">{{ t('proxy.download') }}</span>
            <i class="i-ri-arrow-down-line text-sm text-accent" shrink="0" />
          </div>
          <div mt="1" text="lg primary" font="semibold" tabular="nums">
            {{ fmtTrafficRate(traffic?.down ?? 0) }}
          </div>
          <TrafficSparkline class="text-accent mt-4" :values="trafficDownHistory" />
        </div>
        <div flex="~ col" pl="3" class="border-l border-divider">
          <div flex items="center" gap="2">
            <span text="xs muted">{{ t('proxy.upload') }}</span>
            <i class="i-ri-arrow-up-line text-sm text-warning" shrink="0" />
          </div>
          <div mt="1" text="lg primary" font="semibold" tabular="nums">
            {{ fmtTrafficRate(traffic?.up ?? 0) }}
          </div>
          <TrafficSparkline class="text-warning mt-4" :values="trafficUpHistory" />
        </div>
      </div>
    </section>

    <BaseList
      ref="baseListRef"
      :items="items"
      v-model:selected-index="selectedIndex"
      :group-field="(item: { group: string }) => item.group"
      :group-title="(g: string) => g"
      @execute="onExecute"
    >
      <template #group-title="{ group }">
        <span class="flex-1 min-w-0 truncate">{{ group }}</span>
        <template v-if="group === t('proxy.group.nodes')">
          <BaseButton
            icon="i-ri-focus-3-line"
            :disabled="!hasSelectedNode"
            :title="t('proxy.locateSelected')"
            @click.stop="locateSelected"
          />
          <BaseButton
            icon="i-ri-flashlight-line"
            :loading="testing"
            :disabled="nodes.length === 0"
            :title="t('proxy.testAll')"
            @click.stop="testAll"
          />
        </template>
      </template>

      <template #item="{ item }">
        <!-- 启用代理（合并核心状态） -->
        <BaseListItem v-if="item.type === 'enabled'" :title="t('proxy.enableProxy')">
          <template #subtitle>
            <template v-if="!statusLoaded"></template>
            <template v-else-if="!coreStatus.downloaded">
              {{ isDownloading ? t('proxy.downloadingCore') : t('proxy.coreRequired') }}
            </template>
            <template v-else>
              <span truncate>{{ t('proxy.coreVersion', { version: coreStatus.version }) }}</span>
              <span v-if="coreError" text="danger" shrink="0" ml="2">{{ coreError }}</span>
              <span v-else-if="updateInfo?.hasUpdate" text="success" shrink="0" ml="2">{{
                t('proxy.newCoreAvailable', { version: updateInfo.latest }) +
                (isEnabled ? t('proxy.disableToUpdate') : t('proxy.clickToDownload'))
              }}</span>
            </template>
          </template>
          <template #trailing>
            <!-- 状态未确认：不渲染按钮（避免 下载核心→已关闭/已开启 的错误态闪烁） -->
            <template v-if="!statusLoaded"></template>
            <!-- 下载/更新进行中：进度按钮（disabled） -->
            <BaseButton v-else-if="isDownloading" class="min-w-12 tabular-nums" disabled>{{
              downloadText
            }}</BaseButton>
            <!-- 已下载：开关 + 更新入口（仅关闭代理时显示，开启时走副标题绿色提示） -->
            <div v-else-if="coreStatus.downloaded" flex gap="2">
              <BaseButton
                :variant="isEnabled ? 'primary' : 'default'"
                :loading="toggling"
                @click.stop="toggleEnabled"
              >
                {{ isEnabled ? t('common.enabled') : t('common.disabled') }}
              </BaseButton>
              <BaseButton v-if="isEnabled && coreError" @click.stop="reconnect">{{
                t('proxy.reconnect')
              }}</BaseButton>
              <BaseButton
                v-if="!isEnabled && updateInfo?.hasUpdate"
                icon="i-ri-download-line"
                :title="t('proxy.downloadUpdate')"
                @click.stop="updateCore"
              />
            </div>
            <!-- 未下载：下载入口 -->
            <BaseButton v-else @click.stop="downloadCore">{{ t('proxy.downloadCore') }}</BaseButton>
          </template>
        </BaseListItem>

        <!-- 规则模式 -->
        <BaseListItem
          v-else-if="item.type === 'mode'"
          :title="t('proxy.ruleMode')"
          :subtitle="t('proxy.ruleModeHint')"
        >
          <template #trailing>
            <BaseSelect
              ref="modeSelectRef"
              :model-value="config.mode"
              :options="MODE_OPTIONS"
              @update:model-value="onModeChange"
            />
          </template>
        </BaseListItem>

        <!-- 订阅入口（二级界面）：标题=当前激活订阅，副标题=节点数/到期；
             完整列表、添加、编辑、更新、激活切换均在 subscriptions 子视图 -->
        <BaseListItem v-else-if="item.type === 'subscriptions'" :title="activeSubscriptionTitle">
          <template v-if="activeSubscription" #subtitle>
            <span truncate>{{ activeSubscriptionSubtitle }}</span>
            <!-- 自动更新失败标记（Rust 拉取失败的持久化原因，悬停可见） -->
            <template v-if="activeSubscription.lastError">
              <span text="muted" shrink="0" ml="3">·</span>
              <span
                text="danger"
                shrink="0"
                ml="2"
                whitespace="nowrap"
                :title="activeSubscription.lastError"
                >{{ t('proxy.autoUpdateFailed') }}</span
              >
            </template>
          </template>
          <template #trailing>
            <BaseButton @click.stop="openSubscriptions">
              {{ t('proxy.switchSubscription') }}
            </BaseButton>
          </template>
        </BaseListItem>

        <!-- 分组切换（多 selector 订阅） -->
        <BaseListItem
          v-else-if="item.type === 'groupSelector'"
          :title="t('proxy.nodeGroup')"
          :subtitle="t('proxy.nodeGroupHint')"
        >
          <template #trailing>
            <BaseSelect
              ref="groupSelectRef"
              :model-value="activeGroupName"
              :options="groupOptions"
              @update:model-value="onGroupChange"
            />
          </template>
        </BaseListItem>

        <!-- 节点项 -->
        <BaseListItem
          v-else-if="item.type === 'node'"
          :title="item.node.name"
          :tone="item.node.selected ? 'accent' : undefined"
        >
          <template #trailing>
            <span :class="delayColor(item.node.delay)" class="text-xs font-medium">
              {{ formatDelay(item.node.delay) || '\u00A0' }}
            </span>
          </template>
        </BaseListItem>
      </template>
    </BaseList>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import { useProxyPanel } from './useProxyPanel'
import TrafficSparkline from './TrafficSparkline.vue'
import { t } from '@/runtime/i18n'
import { useAppStore } from '@/stores/app'

// 不解构 template ref：解构后 vue-tsc 不识别 ref="" 为使用；整对象绑定模板也更稳
const p = useProxyPanel()
const {
  items,
  selectedIndex,
  onExecute,
  activeSubscription,
  activeSubscriptionTitle,
  activeSubscriptionSubtitle,
  openSubscriptions,
  hasSelectedNode,
  locateSelected,
  testing,
  nodes,
  testAll,
  coreStatus,
  isDownloading,
  traffic,
  trafficDownHistory,
  trafficUpHistory,
  coreError,
  updateInfo,
  isEnabled,
  toggling,
  statusLoaded,
  reconnect,
  updateCore,
  downloadCore,
  downloadText,
  toggleEnabled,
  config,
  MODE_OPTIONS,
  onModeChange,
  groupOptions,
  activeGroupName,
  onGroupChange,
  delayColor,
  formatDelay,
  fmtTrafficRate,
} = p
// template ref 须保持与 ref 同名顶层绑定（locateSelected / onExecute 在 composable 内消费）
const baseListRef = p.baseListRef
const modeSelectRef = p.modeSelectRef
const groupSelectRef = p.groupSelectRef

const appStore = useAppStore()

/// 最近使用行副标题：当前激活订阅 + 连接状态（状态未就绪只显示订阅，避免「已关闭」误报）。
const summaryProvider = () => {
  const sub = activeSubscription.value
  if (!sub || (!sub.name && !sub.url)) return ''
  const name = activeSubscriptionTitle.value
  return statusLoaded.value
    ? `${name} · ${isEnabled.value ? t('common.enabled') : t('common.disabled')}`
    : name
}
onMounted(() => appStore.registerScreenSummary('proxy', null, summaryProvider))
onUnmounted(() => appStore.unregisterScreenSummary('proxy', null, summaryProvider))

// vue-tsc 不把 ref="" 计为读取；expose 钉死绑定且便于调试
defineExpose({ baseListRef, modeSelectRef, groupSelectRef })
</script>
