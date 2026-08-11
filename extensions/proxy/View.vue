<template>
  <div>
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
        <BaseButton
          v-if="group === t('proxy.group.subscription')"
          icon="i-ri-add-line"
          :title="t('proxy.addSubscription')"
          @click.stop="openCreateModal"
        />
        <template v-else-if="group === t('proxy.group.nodes')">
          <BaseButton
            icon="i-ri-focus-3-line"
            :disabled="!hasSelectedNode"
            :title="t('proxy.locateSelected')"
            @click.stop="locateSelected"
          />
          <BaseButton
            :icon="testing ? 'i-ri-loader-4-line animate-spin' : 'i-ri-flashlight-line'"
            :disabled="testing || nodes.length === 0"
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
              <span v-if="isEnabled && traffic" text="muted" shrink="0" ml="3">·</span>
              <span
                v-if="isEnabled && traffic"
                text="muted"
                shrink="0"
                ml="2"
                flex
                items="center"
                gap="3"
                ><span tabular="nums" whitespace="nowrap">↑ {{ fmtTrafficRate(traffic.up) }}</span>
                <span tabular="nums" whitespace="nowrap"
                  >↓ {{ fmtTrafficRate(traffic.down) }}</span
                ></span
              >
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
              <BaseButton :variant="isEnabled ? 'primary' : 'default'" @click.stop="toggleEnabled">
                {{ isEnabled ? t('common.enabled') : t('common.disabled') }}
              </BaseButton>
              <BaseButton v-if="isEnabled && coreError" @click.stop="reconnect">{{
                t('proxy.reconnect')
              }}</BaseButton>
              <BaseButton v-if="!isEnabled && updateInfo?.hasUpdate" @click.stop="updateCore">{{
                t('proxy.downloadUpdate')
              }}</BaseButton>
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

        <!-- 订阅项（active=激活订阅，accent 强调；点击行切换激活，编辑按钮进编辑） -->
        <BaseListItem
          v-else-if="item.type === 'subscription'"
          :title="item.sub.name || t('proxy.unnamedSubscription')"
          :tone="item.active ? 'accent' : undefined"
          :subtitle="
            item.sub.proxyCount
              ? t('proxy.subscriptionInfo', {
                  count: item.sub.proxyCount,
                  time: formatTime(item.sub.updatedAt),
                })
              : item.sub.url || t('proxy.notConfigured')
          "
        >
          <template #trailing>
            <BaseButton
              icon="i-ri-pencil-line"
              :title="t('proxy.editSubscription')"
              @click.stop="openEditModal(item.sub)"
            />
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

    <!-- 订阅编辑弹窗 -->
    <BaseDialog
      v-if="showEditModal"
      :title="isCreating ? t('proxy.addSubscription') : t('proxy.editSubscription')"
      variant="form"
      size="md"
      show-footer
      :ok-label="t('common.save')"
      @confirm="saveSub"
      @cancel="closeEditModal"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">{{ t('proxy.subscriptionName') }}</span>
          <BaseInput
            v-model="editForm.name"
            :placeholder="t('proxy.subscriptionNamePlaceholder')"
          />
        </div>
        <div class="form-field">
          <span class="form-label">{{ t('proxy.subscriptionUrl') }}</span>
          <BaseInput v-model="editForm.url" :placeholder="t('proxy.subscriptionUrlPlaceholder')" />
        </div>
      </div>
      <template #footer-start>
        <BaseButton
          v-if="!isCreating && config.subscriptions.length > 1"
          variant="danger"
          @click="confirmRemoveFromModal"
        >
          {{ t('common.delete') }}
        </BaseButton>
      </template>
    </BaseDialog>

    <!-- 删除订阅确认 -->
    <BaseDialog
      v-if="deletingSub"
      :title="t('proxy.deleteSubscription')"
      size="sm"
      show-footer
      :ok-label="t('common.delete')"
      @confirm="doRemoveSub"
      @cancel="deletingSub = null"
    >
      <div text="sm secondary">
        {{ t('proxy.deleteConfirm', { name: deletingSub.name || t('proxy.unnamedSubscription') }) }}
      </div>
    </BaseDialog>
  </div>
</template>

<script setup lang="ts">
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import { useProxyPanel } from './useProxyPanel'
import { t } from '@/runtime/i18n'

// 不解构 template ref：解构后 vue-tsc 不识别 ref="" 为使用；整对象绑定模板也更稳
const p = useProxyPanel()
const {
  items,
  selectedIndex,
  onExecute,
  openCreateModal,
  hasSelectedNode,
  locateSelected,
  testing,
  nodes,
  testAll,
  coreStatus,
  isDownloading,
  traffic,
  coreError,
  updateInfo,
  isEnabled,
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
  formatTime,
  showEditModal,
  isCreating,
  closeEditModal,
  saveSub,
  editForm,
  openEditModal,
  confirmRemoveFromModal,
  deletingSub,
  doRemoveSub,
  fmtTrafficRate,
} = p
// template ref 须保持与 ref 同名顶层绑定（locateSelected / onExecute 在 composable 内消费）
const baseListRef = p.baseListRef
const modeSelectRef = p.modeSelectRef
const groupSelectRef = p.groupSelectRef
// vue-tsc 不把 ref="" 计为读取；expose 钉死绑定且便于调试
defineExpose({ baseListRef, modeSelectRef, groupSelectRef })
</script>
