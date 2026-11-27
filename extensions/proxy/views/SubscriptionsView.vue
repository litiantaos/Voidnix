<template>
  <div flex="~ col">
    <BaseList
      :items="items"
      v-model:selected-index="selectedIndex"
      :group-field="groupField"
      :group-title="(g: string) => g"
      @execute="onExecute"
    >
      <template #item="{ item }">
        <!-- 添加订阅：独立动作行（与其他项同样可键盘选中回车），置于列表末尾 -->
        <BaseListItem
          v-if="item.type === 'add'"
          :title="t('proxy.addSubscription')"
          icon="i-ri-add-line"
        />

        <!-- 激活订阅 accent 强调；点击行切换激活，编辑按钮进编辑 -->
        <BaseListItem
          v-else
          :title="item.sub.name || t('proxy.unnamedSubscription')"
          :tone="item.sub.id === config.activeSubscriptionId ? 'accent' : undefined"
        >
          <template #subtitle>
            <template v-if="item.sub.proxyCount">
              <span truncate>{{
                t('proxy.subscriptionCount', { count: item.sub.proxyCount })
              }}</span>
              <!-- 到期时间取自订阅响应头 subscription-userinfo；未提供则省略，过期标 danger -->
              <template v-if="formatSubExpiry(item.sub.expiresAt)">
                <span text="muted" shrink="0" ml="3">·</span>
                <span
                  :text="isSubExpired(item.sub.expiresAt) ? 'danger' : 'muted'"
                  shrink="0"
                  ml="2"
                  whitespace="nowrap"
                  >{{
                    t('proxy.subscriptionExpires', { date: formatSubExpiry(item.sub.expiresAt) })
                  }}</span
                >
              </template>
            </template>
            <span v-else truncate>{{ item.sub.url || t('proxy.notConfigured') }}</span>
            <!-- 自动更新失败标记（Rust 拉取失败的持久化原因，悬停可见）：后台静默路径
                 曾只进 console，用户无从发现订阅已陈旧；成功拉取即清除 -->
            <template v-if="item.sub.lastError">
              <span text="muted" shrink="0" ml="3">·</span>
              <span text="danger" shrink="0" whitespace="nowrap" :title="item.sub.lastError">{{
                t('proxy.autoUpdateFailed')
              }}</span>
            </template>
          </template>
          <template #trailing>
            <div flex gap="2">
              <BaseButton
                icon="i-ri-refresh-line"
                :loading="updatingSubId === item.sub.id"
                :disabled="!item.sub.url || updatingSubId !== ''"
                :title="t('proxy.updateSubscription')"
                @click.stop="refreshSub(item.sub)"
              />
              <BaseButton
                icon="i-ri-pencil-line"
                :title="t('proxy.editSubscription')"
                @click.stop="openEditModal(item.sub)"
              />
            </div>
          </template>
        </BaseListItem>
      </template>
    </BaseList>

    <BaseEmptyState v-if="items.length === 0" icon="i-ri-rss-line" :title="t('common.noResults')" />

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
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import { t } from '@/runtime/i18n'
import { formatSubExpiry, isSubExpired } from '../logic'
import { useSubscriptions } from '../useSubscriptions'

const {
  items,
  selectedIndex,
  config,
  onExecute,
  openEditModal,
  refreshSub,
  updatingSubId,
  showEditModal,
  isCreating,
  closeEditModal,
  saveSub,
  editForm,
  confirmRemoveFromModal,
  deletingSub,
  doRemoveSub,
} = useSubscriptions()

/// 单分组列表：组标题即「订阅」（页内语境），「添加订阅」作为组内最后一行
const groupField = () => t('proxy.group.subscription')
</script>
