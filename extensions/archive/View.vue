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

    <!-- ── 压缩模式：BaseSettingsList 表单（源行 + 名称/格式/级别/密码 + 过滤开关） ── -->
    <template v-if="mode === 'compress'">
      <BaseSettingsList :items="compressItems" @execute="onCompressExecute">
        <!-- 源行尾：主操作按钮组（busy 状态机 / 压缩） -->
        <template #trailing-source>
          <div flex gap="2">
            <template v-if="job.busy">
              <BaseButton disabled>{{ jobLabel }}</BaseButton>
              <BaseButton variant="ghost" @click.stop="cancelJob">{{
                t('archive.cancel')
              }}</BaseButton>
            </template>
            <template v-else-if="job.done">
              <BaseButton icon="i-ri-external-link-line" @click.stop="reveal(job.donePath)">
                {{ t('archive.reveal') }}
              </BaseButton>
            </template>
            <BaseButton
              v-else
              variant="primary"
              :disabled="!outputName.trim()"
              @click.stop="runCompress"
            >
              {{ t('archive.compress') }}
            </BaseButton>
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

    <!-- ── 解压模式：BaseSettingsList 表单（压缩包 / 解压目录 / 查看文件） ── -->
    <template v-if="mode === 'extract' && info">
      <BaseSettingsList :items="extractItems" @execute="onExtractExecute">
        <!-- 压缩包行尾：主操作按钮组（busy 状态机 / 解压） -->
        <template #trailing-archive>
          <div flex gap="2">
            <template v-if="job.busy">
              <BaseButton disabled>{{ jobLabel }}</BaseButton>
              <BaseButton variant="ghost" @click.stop="cancelJob">
                {{ t('archive.cancel') }}
              </BaseButton>
            </template>
            <BaseButton
              v-else-if="job.done"
              icon="i-ri-external-link-line"
              @click.stop="reveal(job.donePath)"
            >
              {{ t('archive.reveal') }}
            </BaseButton>
            <BaseButton
              v-else
              variant="primary"
              :disabled="info.encrypted && !archivePassword"
              @click.stop="runExtract"
            >
              {{ t('archive.extract') }}
            </BaseButton>
          </div>
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
        <!-- 加密包密码行尾：内联输入（回车即解压；解密输入非设置场景，不走弹窗） -->
        <template #trailing-password>
          <BaseInput
            v-model="archivePassword"
            type="password"
            class="w-48"
            :disabled="job.busy"
            @keydown.enter.stop.prevent="runExtract"
          />
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
import { t } from '@/runtime/i18n'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import { config } from './config'
import { archiveInfo, archivePassword, pendingInputPaths } from './index'
import {
  ARCHIVE_EXTENSIONS,
  baseName,
  defaultArchiveName,
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

/** 任务状态（无进度条：busy / done / error 三态 + 按钮 loading 文案） */
const job = ref({ busy: false, error: '', done: false, donePath: '' })
/** 任务代次：新输入 / 复位时递增，旧任务的迟到 Channel 事件按代次丢弃 */
let jobSeq = 0
const jobLabel = computed(() =>
  mode.value === 'compress' ? t('archive.compressing') : t('archive.extracting'),
)

/** 解压输出目录：null = 压缩包同目录（默认），可另选可重置（会话级不落盘） */
const destDir = ref<string | null>(null)

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
    group: '',
  },
  {
    id: 'open_archive',
    title: t('archive.file'),
    icon: idleMode.value === 'extract' ? 'i-ri-folder-zip-line' : 'i-ri-archive-line',
    type: 'custom',
    group: '',
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
  const group = ''
  const meta = `${FORMAT_LABELS[info.value.format]} · ${info.value.entries.length} ${t('archive.entries')}${
    info.value.totalSize ? ` · ${formatBytes(info.value.totalSize)}` : ''
  }${info.value.truncated ? ` · ${t('archive.truncatedShort')}` : ''}`
  const list: SettingItemT[] = [
    {
      id: 'archive',
      title: baseName(info.value.path),
      subtitle: meta,
      icon: 'i-ri-file-zip-line',
      type: 'custom',
      group,
    },
    {
      id: 'destDir',
      title: t('archive.extractTo'),
      subtitle: destDir.value ?? t('archive.sameDir'),
      icon: 'i-ri-folder-line',
      type: 'custom',
      group,
    },
  ]
  if (info.value.encrypted) {
    list.push({
      id: 'password',
      title: `${t('archive.password')} · AES-256`,
      type: 'custom',
      group,
    })
  }
  list.push({
    id: 'browse',
    title: t('archive.browseFiles'),
    icon: 'i-ri-folder-open-line',
    type: 'action',
    action: () => appStore.openSubview('entries'),
    group,
  })
  return list
})

const compressItems = computed<SettingItemT[]>(() => {
  const group = ''
  const list: SettingItemT[] = [
    {
      id: 'source',
      title: t('archive.compressN', { n: inputs.value.length }),
      subtitle: sourceSubtitle.value,
      type: 'custom',
      group,
    },
    {
      id: 'name',
      title: t('archive.outputName'),
      type: 'custom',
      group,
    },
    {
      id: 'format',
      title: t('archive.format'),
      type: 'select',
      value: format.value,
      options: formatOptions,
      update: (v) => setFormat(v as CompressFormat),
      group,
    },
    {
      id: 'level',
      title: t('archive.level'),
      type: 'select',
      value: preset.value,
      options: LEVEL_PRESETS.map((p) => ({ label: t(p.key), value: p.value })),
      update: (v) => {
        preset.value = Number(v)
        config.level = preset.value
      },
      group,
    },
  ]
  // 密码（zip / 7z：AES-256，经弹窗两次输入设置；行尾按钮显状态）
  list.push({
    id: 'password',
    title: t('archive.password'),
    subtitle: t('archive.passwordHint'),
    type: 'custom',
    group,
  })
  list.push(
    {
      id: 'junkFilter',
      title: t('archive.junkFilter'),
      subtitle: t('archive.junkFilterHint'),
      type: 'toggle',
      value: junkFilter.value,
      update: (v) => {
        junkFilter.value = Boolean(v)
      },
      group,
    },
    {
      id: 'hiddenFilter',
      title: t('archive.hiddenFilter'),
      subtitle: t('archive.hiddenFilterHint'),
      type: 'toggle',
      value: hiddenFilter.value,
      update: (v) => {
        hiddenFilter.value = Boolean(v)
      },
      group,
    },
  )
  if (job.value.error) {
    list.push({
      id: 'error',
      title: job.value.error,
      tone: 'danger',
      type: 'custom',
      group,
    })
  }
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
  if (ev.type === 'done') {
    job.value.busy = false
    job.value.done = true
    job.value.donePath = ev.outputPath
  } else if (ev.type === 'error') {
    job.value.busy = false
    job.value.error = ev.message === '已取消' ? '' : ev.message
    if (ev.message === '已取消') appStore.showStatus(t('archive.cancelled'))
    else appStore.showStatus(ev.message, { kind: 'error' })
  }
  // progress 事件忽略（无进度条设计）
}

async function runCompress() {
  if (job.value.busy || !inputs.value.length || !outputName.value.trim()) return
  let dir = config.compressDir
  if (!dir) {
    dir = await withSuppressBlur(() => invoke<string>(CMD.pickDirectory))
    if (!dir) return
  }
  const name = normalizeOutputName(outputName.value.trim(), format.value)
  outputName.value = name
  job.value = { busy: true, error: '', done: false, donePath: '' }
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
  } catch (e) {
    if (seq !== jobSeq) return
    job.value.busy = false
    // 「已取消」已由事件通道 toast，不落成错误行
    job.value.error = String(e) === '已取消' ? '' : String(e)
  }
}

async function runExtract() {
  if (job.value.busy || !info.value) return
  // 默认压缩包同目录；destDir 为用户显式另选（可重置回同目录）
  const path = info.value.path
  const dest = destDir.value ?? path.slice(0, path.lastIndexOf('/'))
  job.value = { busy: true, error: '', done: false, donePath: '' }
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
  } catch (e) {
    if (seq !== jobSeq) return
    job.value.busy = false
    job.value.error = String(e) === '已取消' ? '' : String(e)
  }
}

/** 解压表单行回车：压缩包行 = 解压（busy/done/缺密码守卫与按钮一致）；目录行 = 另选目录 */
function onExtractExecute(item: SettingItemT) {
  if (item.id === 'archive') {
    if (!job.value.busy && !job.value.done && !(info.value?.encrypted && !archivePassword.value)) {
      void runExtract()
    }
  } else if (item.id === 'destDir' && !job.value.busy) void pickDestDir()
}

/** 另选解压目录（镜像 video 输出目录 picker 范式） */
async function pickDestDir() {
  await withSuppressBlur(async () => {
    const selected = await invoke<string>(CMD.pickDirectory)
    if (selected) destDir.value = selected
  })
}

function onCompressExecute(item: SettingItemT) {
  // 密码行回车 = 打开密码弹窗（与行尾按钮同动作）
  if (item.id === 'password') {
    openPwdDialog()
    return
  }
  // 源行回车 = 执行主按钮动作（与按钮组一致：busy/done/空名守卫）
  if (item.id === 'source' && !job.value.busy && !job.value.done && outputName.value.trim()) {
    void runCompress()
  }
}

async function cancelJob() {
  await invoke(CMD.archiveCancel)
}

function reveal(path: string) {
  if (path) void invoke(CMD.revealInFinder, { path })
}

function reset() {
  // 新输入 / 复位：作废旧任务代次并取消仍在跑的任务，迟到的 Channel 事件不再污染新状态
  jobSeq++
  if (job.value.busy) void invoke(CMD.archiveCancel).catch(() => {})
  pwdDialog.value = false
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
  job.value = { busy: false, error: '', done: false, donePath: '' }
}
</script>
