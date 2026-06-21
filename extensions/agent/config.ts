import { computed } from 'vue'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

export interface SearchProviderConfig {
  id: string
  type: 'tavily'
  apiKey: string
}

/// agent 扩展自管配置（持久化至 extensions/agent/config.json）。
export const config = defineConfig('extensions/agent/config', {
  trustedCommands: [
    'ls',
    'cat',
    'pwd',
    'echo',
    'head',
    'tail',
    'wc',
    'file',
    'stat',
    'date',
    'which',
    'whoami',
    'uname',
    'grep',
    'rg',
    'fd',
    'ag',
    'tree',
    'diff',
    'comm',
    'cmp',
    'md5sum',
    'shasum',
    'sort',
    'uniq',
    'cut',
    'tr',
    'paste',
    'expand',
    'jq',
    'yq',
    'bat',
  ],
  // 安全底线项（Rust 端强制 clamp/并集，config.json 越界无效；BOUNDS 见下方）
  forbiddenCommands: [] as string[], // 用户补充（与 Rust FORBIDDEN_FLOOR 取并集，只能加严）
  blockedArgs: [] as string[], // 用户补充（与 Rust DENIED_ARG_FLOOR 取并集）
  maxCpuSeconds: 30,
  maxMemoryMb: 512,
  maxOpenFiles: 64,
  executionTimeout: 30,
  maxOutputBytes: 1048576,
  maxTurns: 10,
  systemPrompt: '',
  searchProviders: [
    { id: generateRequestId(), type: 'tavily', apiKey: '' },
  ] as SearchProviderConfig[],
  activeSearchProviderId: '',
})

/// 安全底线 UI 镜像（权威在 native/policy.rs，⚠️ 须手动同步）。
/// 不变量：floor 必须 ⊇ Rust 端 FORBIDDEN_FLOOR / DENIED_ARG_FLOOR（迁移即取并集，禁止缩窄）。
/// 仅用于 Settings.vue 越界警告 + trusted∩forbidden 交集提示；Rust 端并集兜底，不信任此值。
export const BOUNDS = {
  maxTurns: { floor: 1, cap: 50 },
  maxCpuSeconds: { floor: 1, cap: 300 },
  maxMemoryMb: { floor: 64, cap: 4096 },
  maxOpenFiles: { floor: 8, cap: 1024 },
  executionTimeout: { floor: 1, cap: 300 },
  maxOutputBytes: { floor: 1024, cap: 10485760 },
  forbiddenCommands: {
    floor: [
      'sh',
      'bash',
      'zsh',
      'dash',
      'ksh',
      'fish',
      'csh',
      'tcsh',
      'osascript',
      'sudo',
      'open',
      'launchctl',
      'defaults',
      'networksetup',
      'scutil',
      'killall',
      'kill',
      'pkill',
      'curl',
      'wget',
      'nc',
      'socat',
      'telnet',
      'ssh',
      'su',
      'doas',
      'expect',
      'sqlite3',
      'ps',
      'top',
      'htop',
    ],
  },
  blockedArgs: {
    floor: [
      '--exec',
      '--exec-file',
      '--exec-rm',
      // C4：find 单连字符 exec 谓词族
      '-exec',
      '-execdir',
      '-ok',
      '-okdir',
      '--upload-pack',
      '--use-compress-program',
      '--config',
      '-C',
      '--output',
      '-o',
      '-O',
      '--write-out',
      '--eval',
      '-e',
      '--init-file',
      '--rcfile',
    ],
  },
} as const

/// 当前激活的搜索提供商（回退到第一个）。
export const activeSearchProvider = computed(
  () =>
    config.searchProviders.find((p) => p.id === config.activeSearchProviderId) ||
    config.searchProviders[0],
)

export function addSearchProvider(): string {
  const id = generateRequestId()
  config.searchProviders.push({ id, type: 'tavily', apiKey: '' })
  config.activeSearchProviderId = id
  return id
}

export function removeSearchProvider(id: string) {
  const idx = config.searchProviders.findIndex((c) => c.id === id)
  if (idx === -1 || config.searchProviders.length <= 1) return
  config.searchProviders.splice(idx, 1)
  if (config.activeSearchProviderId === id) {
    config.activeSearchProviderId = config.searchProviders[0]?.id || ''
  }
}

export function updateSearchProvider(id: string, partial: Partial<SearchProviderConfig>) {
  const p = config.searchProviders.find((c) => c.id === id)
  if (p) Object.assign(p, partial)
}
