<template>
  <!-- 无根级 pb：三模式均以 BaseList（自带 pb-3）收尾，根级再叠 pb 会双倍底部留白（24px） -->
  <div class="archive-view flex flex-col">
    <!-- ── 空态：仅解压入口（压缩入口 = 访达多选 ⌥F） ── -->
    <template v-if="mode === 'idle'">
      <BaseSettingsList :items="entryItems" @execute="onIdleExecute">
        <template #trailing-open_archive>
          <BaseButton @click.stop="void pickIdle()">
            {{ t('archive.selectFiles') }}
          </BaseButton>
        </template>
      </BaseSettingsList>
    </template>

    <!-- ── 压缩模式：BaseSettingsList 表单（源行 + 目标目录 + 名称/格式/级别/密码 + 过滤开关） ── -->
    <template v-if="mode === 'compress'">
      <BaseSettingsList :items="compressItems" @execute="onCompressExecute">
        <!-- 任务行 = 处理态任务行（「任务」分组，第一组、常驻）：左侧与其它行同构
             （图标井 + 标题），右侧紧凑进度组（进度条 + 紧贴条尾的百分比 + 取消） -->
        <template #trailing-operations>
          <div v-if="job.busy || jobDone" flex items="center" gap="3">
            <!-- 条与百分比同组紧贴；百分比与操作按钮之间留宽一档（gap-3） -->
            <div flex items="center" gap="2">
              <div class="w-40" shrink="0"><BaseProgress :value="taskProgress" /></div>
              <span v-if="taskPercent != null" class="tabular-nums" text="xs secondary" shrink="0"
                >{{ taskPercent }}%</span
              >
            </div>
            <BaseButton v-if="job.busy && !jobDone" @click.stop="cancelJob">{{
              t('archive.cancel')
            }}</BaseButton>
          </div>
          <BaseButton
            v-else
            variant="primary"
            :disabled="!outputName.trim()"
            @click.stop="runCompress"
            >{{ t('common.start') }}</BaseButton
          >
        </template>
        <!-- 目标目录行尾：源文件同目录重置 + 选择（与解压目录行同范式） -->
        <template #trailing-destDir>
          <div flex gap="2">
            <BaseButton
              v-if="compressDestDir"
              :disabled="job.busy"
              @click.stop="compressDestDir = null"
              >{{ t('archive.sourceDir') }}</BaseButton
            >
            <BaseButton :disabled="job.busy" @click.stop="pickCompressDestDir">{{
              t('archive.chooseDir')
            }}</BaseButton>
          </div>
        </template>
        <template #trailing-name>
          <BaseInput
            v-model="outputName"
            class="w-48"
            :disabled="job.busy"
            @keydown.enter.stop.prevent="runCompress"
          />
        </template>
        <template #trailing-password>
          <BaseButton :disabled="job.busy" @click.stop="openPwdDialog">
            {{ password ? t('archive.passwordSet') : t('archive.passwordUnset') }}
          </BaseButton>
        </template>
      </BaseSettingsList>
    </template>

    <!-- 密码弹窗：输入两次确认（均留空 = 清除密码不加密） -->
    <BaseDialog
      v-if="pwdDialog"
      :title="t('archive.password')"
      variant="form"
      size="sm"
      show-footer
      :close-on-confirm="false"
      :ok-label="t('common.confirm')"
      @confirm="confirmPwd"
      @cancel="pwdDialog = false"
    >
      <div flex="~ col" gap="3">
        <div class="form-field">
          <span class="form-label">{{ t('archive.password') }}</span>
          <BaseInput v-model="pwdDraft" type="password" />
        </div>
        <div class="form-field">
          <span class="form-label">{{ t('archive.passwordConfirm') }}</span>
          <BaseInput v-model="pwdDraft2" type="password" @keydown.enter.stop.prevent="confirmPwd" />
        </div>
        <div v-if="pwdError" class="text-xs text-danger">{{ pwdError }}</div>
        <div class="text-xs text-muted">{{ t('archive.passwordDialogHint') }}</div>
      </div>
    </BaseDialog>

    <!-- 解压密码弹窗：点击解压时加密包弹此输入（确定即直接解压；密码不落盘仅会话内复用） -->
    <BaseDialog
      v-if="extractPwdDialog"
      :title="t('archive.password')"
      variant="form"
      size="sm"
      show-footer
      :ok-label="t('common.confirm')"
      @confirm="confirmExtractPwd"
      @cancel="extractPwdDialog = false"
    >
      <div class="form-field">
        <span class="form-label">{{ t('archive.password') }}</span>
        <BaseInput
          v-model="extractPwdDraft"
          type="password"
          @keydown.enter.stop.prevent="confirmExtractPwd"
        />
      </div>
    </BaseDialog>

    <!-- ── 解压模式：BaseSettingsList 表单（压缩包 / 解压目录 / 查看文件） ── -->
    <template v-if="mode === 'extract' && info">
      <BaseSettingsList :items="extractItems" @execute="onExtractExecute">
        <!-- 任务行 = 处理态任务行（同压缩模式，第一组、常驻） -->
        <template #trailing-operations>
          <div v-if="job.busy || jobDone" flex items="center" gap="3">
            <!-- 条与百分比同组紧贴；百分比与操作按钮之间留宽一档（gap-3） -->
            <div flex items="center" gap="2">
              <div class="w-40" shrink="0"><BaseProgress :value="taskProgress" /></div>
              <span v-if="taskPercent != null" class="tabular-nums" text="xs secondary" shrink="0"
                >{{ taskPercent }}%</span
              >
            </div>
            <BaseButton v-if="job.busy && !jobDone" @click.stop="cancelJob">{{
              t('archive.cancel')
            }}</BaseButton>
          </div>
          <BaseButton v-else variant="primary" @click.stop="runExtract">{{
            t('common.start')
          }}</BaseButton>
        </template>
        <!-- 解压目录行尾：同目录重置 + 选择（镜像 video 输出目录行范式） -->
        <template #trailing-destDir>
          <div flex gap="2">
            <BaseButton v-if="destDir" :disabled="job.busy" @click.stop="destDir = null">{{
              t('archive.sameDir')
            }}</BaseButton>
            <BaseButton :disabled="job.busy" @click.stop="pickDestDir">{{
              t('archive.chooseDir')
            }}</BaseButton>
          </div>
        </template>
      </BaseSettingsList>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore, withSuppressBlur } from '@/stores/app'
import { hideWindow } from '@/utils/tauri'
import { t } from '@/runtime/i18n'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseProgress from '@/components/ui/BaseProgress.vue'
import { config } from './config'
import { archiveInfo, archivePassword, pendingInputPaths } from './index'
import {
  ARCHIVE_EXTENSIONS,
  baseName,
  defaultArchiveName,
  dirOf,
  formatBytes,
  isArchivePath,
  LEVEL_PRESETS,
  normalizeOutputName,
  type ArchiveEvent,
  type ArchiveInfo,
  type CompressFormat,
  type CompressResult,
  type ExtractResult,
  FORMAT_LABELS,
} from './logic'

const appStore = useAppStore()

type Mode = 'idle' | 'extract' | 'compress'
const mode = ref<Mode>('idle')
/** 当前压缩包信息（与 entries 二级界面共享） */
const info = archiveInfo
const inputs = ref<string[]>([])

/** 压缩密码（不落盘：仅请求体一次性传给命令；经弹窗两次输入设置） */
const password = ref('')

// ── 压缩密码弹窗（两次输入确认；均留空 = 清除） ──
const pwdDialog = ref(false)
const pwdDraft = ref('')
const pwdDraft2 = ref('')
const pwdError = ref('')

function openPwdDialog() {
  if (job.value.busy) return
  pwdDraft.value = ''
  pwdDraft2.value = ''
  pwdError.value = ''
  pwdDialog.value = true
}

function confirmPwd() {
  const a = pwdDraft.value
  const b = pwdDraft2.value
  if (!a && !b) {
    // 清除密码（不加密）
    password.value = ''
    pwdDialog.value = false
    return
  }
  if (a !== b) {
    pwdError.value = t('archive.passwordMismatch')
    return
  }
  password.value = a
  pwdDialog.value = false
}

// ── 解压密码弹窗（加密包点击解压时弹此输入；确定即直接解压） ──
const extractPwdDialog = ref(false)
const extractPwdDraft = ref('')

function confirmExtractPwd() {
  if (!extractPwdDraft.value) return
  archivePassword.value = extractPwdDraft.value
  extractPwdDialog.value = false
  void runExtract()
}

/** 任务状态：error 不再常驻列表行——失败统一 toast（与 video/image 一致） */
const job = ref({ busy: false })
/** 任务进度百分比（null = 引擎不产出分母——bsdtar / 7z / rar 无进度事件） */
const jobPercent = ref<number | null>(null)
/** 任务完成落位标志（满进度条 + 完成文案停留 600ms 后揭晓/关窗） */
const jobDone = ref(false)
/** 任务代次：新输入 / 复位时递增，旧任务的迟到 Channel 事件按代次丢弃 */
let jobSeq = 0
/** 最近一次失败 toast：Channel error 与 invoke reject 双通道同报同一错误，按消息去重 */
let lastErrorToast: { msg: string; at: number } | null = null

/** 处理态任务行（独立「任务」分组）：图标固定动作语义、标题/进度随任务切换，单行紧凑布局 */
const taskTitle = computed(() => {
  if (jobDone.value)
    return mode.value === 'compress'
      ? t('archive.compressDoneLabel')
      : t('archive.extractDoneLabel')
  if (job.value.busy)
    return mode.value === 'compress' ? t('archive.compressing') : t('archive.extracting')
  return mode.value === 'compress' ? t('archive.compressTask') : t('archive.extractTask')
})
/** 0 视作尚未产出分母：按不定进度呈现（不假装 0% ） */
const taskProgress = computed<number | null | undefined>(() => {
  if (jobDone.value) return 100
  if (!job.value.busy) return undefined
  return jobPercent.value != null && jobPercent.value > 0 ? jobPercent.value : null
})

/** 行尾百分比：完成闪现 100%；未产出分母不显示数字（进度条走不定进度） */
const taskPercent = computed<number | null>(() => {
  if (jobDone.value) return 100
  if (!job.value.busy || jobPercent.value == null || jobPercent.value <= 0) return null
  return Math.round(jobPercent.value)
})

/** 两种表单共用：操作行永远是第一项（独立「任务」分组，空闲 = 主操作入口） */
const operationsItem = computed<SettingItemT>(() => ({
  id: 'operations',
  title: taskTitle.value,
  icon: mode.value === 'compress' ? 'i-ri-inbox-archive-line' : 'i-ri-inbox-unarchive-line',
  type: 'custom',
  group: t('common.group.task'),
}))

/** 解压输出目录：null = 压缩包同目录（默认），可另选可重置（会话级不落盘） */
const destDir = ref<string | null>(null)
/** 压缩输出目录：null = 源文件同目录（默认），可另选可重置（会话级不落盘） */
const compressDestDir = ref<string | null>(null)

// 格式与过滤开关不落盘：每次进入（acceptPaths→reset）重置默认——zip / 垃圾过滤开 / 隐藏过滤关
const format = ref<CompressFormat>('zip')
const preset = ref(config.level <= 3 ? 1 : config.level >= 8 ? 9 : 6)
const junkFilter = ref(true)
const hiddenFilter = ref(false)
const outputName = ref('')

const formatOptions = [
  { label: 'ZIP', value: 'zip' },
  { label: '7Z', value: 'sevenZ' },
]

/** 切格式同步输出名（内容格式由请求体决定，扩展名必须跟随，否则 7z 内容会叫 .zip）。 */
function setFormat(v: CompressFormat) {
  format.value = v
  if (outputName.value) outputName.value = normalizeOutputName(outputName.value, v)
}

type SettingItemT = import('@/types/settings').SettingItem

// ── 空态：先选操作方式，再选文件（选完进入对应界面，与访达入口同构） ──

/** 空态操作方式：解压（默认）或压缩 */
const idleMode = ref<'extract' | 'compress'>('extract')

/** 空态选文件行回车 = 与右侧按钮同动作 */
function onIdleExecute(item: SettingItemT) {
  if (item.id === 'open_archive') void pickIdle()
}

// ── 输入路由 ──

function acceptPaths(paths: string[]) {
  reset()
  if (!paths.length) return
  if (paths.every(isArchivePath)) {
    void loadArchive(paths[0])
  } else {
    inputs.value = paths
    outputName.value = defaultArchiveName(paths, format.value)
    mode.value = 'compress'
  }
}

// 跨扩展进入：finder-ext 等同页投递（immediate：LRU 重挂载时 watch 晚于写入）
watch(
  pendingInputPaths,
  (ps) => {
    if (!ps.length) return
    acceptPaths(ps)
    pendingInputPaths.value = []
  },
  { immediate: true },
)

/** 空态选文件：解压 = 单选压缩包（扩展名过滤）；压缩 = 多选任意文件 */
async function pickIdle() {
  if (idleMode.value === 'extract') {
    await pickArchive()
    return
  }
  await withSuppressBlur(async () => {
    // allowedExtensions 必传（Rust 侧 Vec 参数无默认值，漏传即参数校验失败）
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: true,
      allowedExtensions: [],
    })
    if (paths.length) acceptPaths(paths)
  })
}

const entryItems = computed<SettingItemT[]>(() => [
  {
    id: 'mode',
    title: t('archive.mode'),
    // 图标随操作方式跟随（解包/打包语义）
    icon: idleMode.value === 'extract' ? 'i-ri-inbox-unarchive-line' : 'i-ri-inbox-archive-line',
    type: 'select',
    value: idleMode.value,
    options: [
      { label: t('archive.extract'), value: 'extract' },
      { label: t('archive.compress'), value: 'compress' },
    ],
    update: (v) => {
      idleMode.value = v as 'extract' | 'compress'
    },
    group: t('archive.group.common'),
  },
  {
    id: 'open_archive',
    title: t('archive.file'),
    icon: idleMode.value === 'extract' ? 'i-ri-folder-zip-line' : 'i-ri-archive-line',
    type: 'custom',
    group: t('archive.group.file'),
  },
])

// ── 压缩表单 ──

/** 源摘要：单选展示名字，多选「首名 + 等 N 项」 */
const sourceSubtitle = computed(() => {
  const names = inputs.value.map(baseName)
  if (names.length <= 1) return names[0] ?? ''
  return `${names[0]} · ${t('archive.andMore', { n: names.length })}`
})

// ── 解压表单（压缩包 / 解压目录 / 加密包密码 / 查看文件） ──

const extractItems = computed<SettingItemT[]>(() => {
  if (!info.value) return []
  const meta = `${FORMAT_LABELS[info.value.format]} · ${info.value.entries.length} ${t('archive.entries')}${
    info.value.totalSize ? ` · ${formatBytes(info.value.totalSize)}` : ''
  }${info.value.truncated ? ` · ${t('archive.truncatedShort')}` : ''}`
  const list: SettingItemT[] = [
    operationsItem.value,
    {
      id: 'archive',
      title: baseName(info.value.path),
      subtitle: meta,
      icon: 'i-ri-file-zip-line',
      type: 'custom',
      group: t('archive.group.file'),
    },
    {
      id: 'browse',
      title: t('archive.browseFiles'),
      icon: 'i-ri-folder-open-line',
      type: 'action',
      action: () => appStore.openSubview('entries'),
      group: t('archive.group.file'),
    },
    {
      id: 'destDir',
      title: t('archive.extractTo'),
      subtitle: destDir.value ?? t('archive.sameDir'),
      icon: 'i-ri-folder-line',
      type: 'custom',
      group: t('archive.group.output'),
    },
  ]
  return list
})

const compressItems = computed<SettingItemT[]>(() => {
  const list: SettingItemT[] = [
    operationsItem.value,
    {
      id: 'source',
      title: t('archive.compressN', { n: inputs.value.length }),
      subtitle: sourceSubtitle.value,
      icon: 'i-ri-file-list-3-line',
      type: 'custom',
      group: t('archive.group.file'),
    },
    {
      id: 'format',
      title: t('archive.format'),
      icon: 'i-ri-file-zip-line',
      type: 'select',
      value: format.value,
      options: formatOptions,
      update: (v) => setFormat(v as CompressFormat),
      group: t('archive.group.params'),
    },
    {
      id: 'level',
      title: t('archive.level'),
      icon: 'i-ri-dashboard-3-line',
      type: 'select',
      value: preset.value,
      options: LEVEL_PRESETS.map((p) => ({ label: t(p.key), value: p.value })),
      update: (v) => {
        preset.value = Number(v)
        config.level = preset.value
      },
      group: t('archive.group.params'),
    },
    {
      id: 'password',
      title: t('archive.password'),
      subtitle: t('archive.passwordHint'),
      icon: 'i-ri-lock-line',
      type: 'custom',
      group: t('archive.group.params'),
    },
    {
      id: 'destDir',
      title: t('archive.compressTo'),
      subtitle: compressDestDir.value ?? t('archive.sourceDir'),
      icon: 'i-ri-folder-line',
      type: 'custom',
      group: t('archive.group.output'),
    },
    {
      id: 'name',
      title: t('archive.outputName'),
      icon: 'i-ri-price-tag-3-line',
      type: 'custom',
      group: t('archive.group.output'),
    },
    {
      id: 'junkFilter',
      title: t('archive.junkFilter'),
      subtitle: t('archive.junkFilterHint'),
      icon: 'i-ri-delete-bin-line',
      type: 'toggle',
      value: junkFilter.value,
      update: (v) => {
        junkFilter.value = Boolean(v)
      },
      group: t('archive.group.filter'),
    },
    {
      id: 'hiddenFilter',
      title: t('archive.hiddenFilter'),
      subtitle: t('archive.hiddenFilterHint'),
      icon: 'i-ri-eye-off-line',
      type: 'toggle',
      value: hiddenFilter.value,
      update: (v) => {
        hiddenFilter.value = Boolean(v)
      },
      group: t('archive.group.filter'),
    },
  ]
  return list
})

async function pickArchive() {
  await withSuppressBlur(async () => {
    const paths = await invoke<string[]>(CMD.pickFiles, {
      allowsMultiple: false,
      allowedExtensions: [...ARCHIVE_EXTENSIONS],
    })
    if (paths[0]) await loadArchive(paths[0])
  })
}

async function loadArchive(path: string) {
  reset()
  try {
    info.value = await invoke<ArchiveInfo>(CMD.archiveInspect, { path })
    mode.value = 'extract'
  } catch (e) {
    appStore.showStatus(String(e), { kind: 'error' })
    mode.value = 'idle'
    info.value = null
  }
}

// ── 执行 ──

function onEvent(ev: ArchiveEvent, seq: number) {
  if (seq !== jobSeq) return
  // done 不在此处理：busy 保持到 finishJob 收尾（满进度 + 完成文案 → 揭晓 → 关窗）结束
  if (ev.type === 'progress') {
    // 完成收尾期间（满进度 + 完成文案）迟到的 progress 不再回退进度
    if (!jobDone.value) jobPercent.value = ev.percent
  } else if (ev.type === 'error') {
    job.value.busy = false
    jobPercent.value = null
    jobDone.value = false
    // 密码类失败清空会话密码：下次解压重新弹窗输入
    if (ev.message.includes('密码')) archivePassword.value = ''
    if (ev.message === '已取消') appStore.showStatus(t('archive.cancelled'))
    else toastFailure(ev.message)
  }
}

/** 失败 toast 去重：同一任务里 Channel error 先到、invoke reject 后到（或反之）只报一次 */
function toastFailure(msg: string) {
  const now = performance.now()
  if (lastErrorToast?.msg === msg && now - lastErrorToast.at < 1000) return
  lastErrorToast = { msg, at: now }
  appStore.showStatus(msg, { duration: 5000, kind: 'error' })
}

/** 失败兜底上报：invoke reject 与 Channel error 双通道同报同一错误（按消息去重） */
function reportFailure(e: unknown, seq: number) {
  if (seq !== jobSeq) return
  const msg = String(e)
  if (msg === '已取消') return
  if (msg.includes('密码')) archivePassword.value = ''
  toastFailure(msg)
}

/** 成功收尾：满进度 + 完成文案停留 600ms（统一处理态完成闪现）→ 打开/聚焦目标访达窗口
 *  并隐藏主窗（toast 为独立原生浮窗，主窗隐藏后继续展示至到期）。seq 守卫：中途新输入/复位即中止。 */
async function finishJob(outputPath: string, seq: number) {
  jobPercent.value = 100
  jobDone.value = true
  await sleep(600)
  if (seq !== jobSeq) return
  await invoke(CMD.revealInFinder, { path: outputPath }).catch(() => {})
  hideWindow()
  // 窗口已隐藏，静默复位状态供下次进入
  job.value = { busy: false }
  jobPercent.value = null
  jobDone.value = false
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms))
}

async function runCompress() {
  if (job.value.busy || !inputs.value.length || !outputName.value.trim()) return
  // 默认源文件同目录；compressDestDir 为用户显式另选（可重置回同目录），不再弹目录选择面板
  const dir = compressDestDir.value ?? dirOf(inputs.value[0])
  const name = normalizeOutputName(outputName.value.trim(), format.value)
  outputName.value = name
  job.value = { busy: true }
  jobPercent.value = null
  jobDone.value = false
  lastErrorToast = null
  const seq = ++jobSeq
  const channel = new Channel<ArchiveEvent>()
  channel.onmessage = (ev) => onEvent(ev, seq)
  try {
    const result = await invoke<CompressResult>(CMD.archiveCompress, {
      request: {
        inputPaths: inputs.value,
        outputPath: `${dir}/${name}`,
        format: format.value,
        level: preset.value,
        junkFilter: junkFilter.value,
        hiddenFilter: hiddenFilter.value,
        password: password.value || null,
      },
      onEvent: channel,
    })
    if (seq !== jobSeq) return
    appStore.showStatus(
      `${t('archive.compressDone', {
        n: result.fileCount,
        size: formatBytes(result.sizeBytes),
      })}${result.skippedJunk ? ` · ${t('archive.junkSkipped', { n: result.skippedJunk })}` : ''}`,
    )
    await finishJob(result.outputPath, seq)
  } catch (e) {
    if (seq !== jobSeq) return
    job.value.busy = false
    jobPercent.value = null
    reportFailure(e, seq)
  }
}

async function runExtract() {
  if (job.value.busy || !info.value) return
  // 加密包且未输过密码：弹窗输入，确定后经 confirmExtractPwd 续跑本函数直接解压
  if (info.value.encrypted && !archivePassword.value) {
    extractPwdDraft.value = ''
    extractPwdDialog.value = true
    return
  }
  // 默认压缩包同目录；destDir 为用户显式另选（可重置回同目录）
  const path = info.value.path
  const dest = destDir.value ?? dirOf(path)
  job.value = { busy: true }
  jobPercent.value = null
  jobDone.value = false
  lastErrorToast = null
  const seq = ++jobSeq
  const channel = new Channel<ArchiveEvent>()
  channel.onmessage = (ev) => onEvent(ev, seq)
  try {
    const result = await invoke<ExtractResult>(CMD.archiveExtract, {
      request: {
        archivePath: path,
        destDir: dest,
        selected: null,
        password: info.value.encrypted && archivePassword.value ? archivePassword.value : null,
      },
      onEvent: channel,
    })
    if (seq !== jobSeq) return
    appStore.showStatus(
      `${t('archive.extractDone', { n: result.fileCount })}${
        result.skippedUnsafe ? ` · ${t('archive.skippedUnsafe', { n: result.skippedUnsafe })}` : ''
      }`,
    )
    await finishJob(result.outputDir, seq)
  } catch (e) {
    if (seq !== jobSeq) return
    job.value.busy = false
    jobPercent.value = null
    reportFailure(e, seq)
  }
}

/** 解压表单行回车：操作行 = 主按钮动作（进行中取消，同 video）；目录行 = 另选目录 */
function onExtractExecute(item: SettingItemT) {
  if (item.id === 'operations') {
    if (jobDone.value) return
    if (job.value.busy) void cancelJob()
    else void runExtract()
    return
  }
  if (item.id === 'destDir' && !job.value.busy) void pickDestDir()
}

/** 另选解压目录（镜像 video 输出目录 picker 范式） */
async function pickDestDir() {
  await withSuppressBlur(async () => {
    const selected = await invoke<string>(CMD.pickDirectory)
    if (selected) destDir.value = selected
  })
}

/** 另选压缩目标目录（同解压目录 picker 范式；默认源文件同目录） */
async function pickCompressDestDir() {
  await withSuppressBlur(async () => {
    const selected = await invoke<string>(CMD.pickDirectory)
    if (selected) compressDestDir.value = selected
  })
}

function onCompressExecute(item: SettingItemT) {
  // 操作行回车 = 主按钮动作（进行中取消，同 video；空名/busy 守卫与按钮一致）
  if (item.id === 'operations') {
    if (jobDone.value) return
    if (job.value.busy) {
      void cancelJob()
      return
    }
    if (outputName.value.trim()) void runCompress()
    return
  }
  // 目标目录行回车 = 另选目录（与行尾按钮同动作）
  if (item.id === 'destDir') {
    if (!job.value.busy) void pickCompressDestDir()
    return
  }
  // 密码行回车 = 打开密码弹窗（与行尾按钮同动作）
  if (item.id === 'password') openPwdDialog()
}

async function cancelJob() {
  await invoke(CMD.archiveCancel)
}

function reset() {
  // 新输入 / 复位：作废旧任务代次并取消仍在跑的任务，迟到的 Channel 事件不再污染新状态
  jobSeq++
  if (job.value.busy) void invoke(CMD.archiveCancel).catch(() => {})
  pwdDialog.value = false
  extractPwdDialog.value = false
  // 格式/过滤开关重置默认（不落盘的会话级选择，见声明处注释）
  format.value = 'zip'
  junkFilter.value = true
  hiddenFilter.value = false
  mode.value = 'idle'
  info.value = null
  inputs.value = []
  password.value = ''
  archivePassword.value = ''
  destDir.value = null
  compressDestDir.value = null
  job.value = { busy: false }
  jobPercent.value = null
  jobDone.value = false
  lastErrorToast = null
}
</script>
