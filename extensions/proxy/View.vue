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
          v-if="group === '订阅'"
          icon="i-ri-add-line"
          title="添加订阅"
          @click.stop="openCreateModal"
        />
        <template v-else-if="group === '节点'">
          <BaseButton
            icon="i-ri-focus-3-line"
            :disabled="!hasSelectedNode"
            title="定位到选中节点"
            @click.stop="locateSelected"
          />
          <BaseButton
            :icon="testing ? 'i-ri-loader-4-line animate-spin' : 'i-ri-flashlight-line'"
            :disabled="testing || nodes.length === 0"
            title="全部测速"
            @click.stop="testAll"
          />
        </template>
      </template>

      <template #item="{ item }">
        <!-- 启用代理（合并核心状态） -->
        <BaseListItem v-if="item.type === 'enabled'" title="开启代理">
          <template #subtitle>
            <template v-if="!coreStatus.downloaded">
              {{ isDownloading ? '正在下载核心…' : '功能依赖 mihomo 核心，请先下载' }}
            </template>
            <template v-else>
              <span truncate>核心版本：mihomo {{ coreStatus.version }}</span>
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
              <span v-else-if="updateInfo?.hasUpdate" text="success" shrink="0" ml="2"
                >有新核心 {{ updateInfo.latest
                }}{{ isEnabled ? '，请关闭代理后更新' : '，点击下载更新' }}</span
              >
            </template>
          </template>
          <template #trailing>
            <!-- 下载/更新进行中：进度按钮（disabled） -->
            <BaseButton v-if="isDownloading" class="min-w-12 tabular-nums" disabled>{{
              downloadText
            }}</BaseButton>
            <!-- 已下载：开关 + 更新入口（仅关闭代理时显示，开启时走副标题绿色提示） -->
            <div v-else-if="coreStatus.downloaded" flex gap="2">
              <BaseButton :variant="isEnabled ? 'primary' : 'default'" @click.stop="toggleEnabled">
                {{ isEnabled ? '已开启' : '已关闭' }}
              </BaseButton>
              <BaseButton v-if="isEnabled && coreError" @click.stop="reconnect">重连</BaseButton>
              <BaseButton v-if="!isEnabled && updateInfo?.hasUpdate" @click.stop="updateCore"
                >下载更新</BaseButton
              >
            </div>
            <!-- 未下载：下载入口 -->
            <BaseButton v-else @click.stop="downloadCore">下载核心</BaseButton>
          </template>
        </BaseListItem>

        <!-- 规则模式 -->
        <BaseListItem
          v-else-if="item.type === 'mode'"
          title="规则模式"
          subtitle="规则按分流策略，全局代理所有流量"
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

        <!-- 订阅项 -->
        <BaseListItem
          v-else-if="item.type === 'subscription'"
          :title="item.sub.name || '未命名订阅'"
          :subtitle="
            item.sub.proxyCount
              ? `${item.sub.proxyCount} 节点 · ${formatTime(item.sub.updatedAt)}`
              : item.sub.url || '未配置'
          "
        />

        <!-- 分组切换（多 selector 订阅） -->
        <BaseListItem
          v-else-if="item.type === 'groupSelector'"
          title="节点分组"
          subtitle="当前显示的 selector 分组"
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
        <BaseListItem v-else-if="item.type === 'node'">
          <template #title>
            <span :class="item.node.selected ? 'text-accent' : ''">
              {{ item.node.name }}
            </span>
          </template>
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
      :title="isCreating ? '添加订阅' : '编辑订阅'"
      variant="form"
      size="md"
      show-footer
      ok-label="保存"
      @confirm="saveSub"
      @cancel="closeEditModal"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">订阅名称</span>
          <BaseInput v-model="editForm.name" placeholder="默认为订阅链接域名" />
        </div>
        <div class="form-field">
          <span class="form-label">订阅链接</span>
          <BaseInput v-model="editForm.url" placeholder="订阅 URL 或 Clash YAML URL" />
        </div>
      </div>
      <template #footer-start>
        <BaseButton
          v-if="!isCreating && config.subscriptions.length > 1"
          variant="danger"
          @click="confirmRemoveFromModal"
        >
          删除
        </BaseButton>
      </template>
    </BaseDialog>

    <!-- 删除订阅确认 -->
    <BaseDialog
      v-if="deletingSub"
      title="删除订阅"
      size="sm"
      show-footer
      ok-label="删除"
      @confirm="doRemoveSub"
      @cancel="deletingSub = null"
    >
      <div text="sm secondary">确定删除「{{ deletingSub.name || '未命名订阅' }}」？</div>
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
