import { computed } from 'vue'
import { load } from '@tauri-apps/plugin-store'
import { defineConfig, whenConfigReady } from '@/runtime/storage'
import { isTauri } from '@/utils/tauri'
import {
  config as aiProvidersConfig,
  modelSelectOptions,
  parseSelectionKey,
  resolveCredentials,
  resolveRuntimeCredentials,
  hasAnyConfiguredProvider,
  providerDisplayName,
  onAiProvidersChange,
  type AiProvider as AiProviderConfig,
  type ResolvedAiCredentials,
} from '@/runtime/ai-providers'

export interface SearchProviderConfig {
  type: 'tavily'
  apiKey: string
}

// 再导出中枢工具；选用状态在 agent 本扩展 config
export {
  aiProvidersConfig,
  modelSelectOptions,
  providerDisplayName,
  hasAnyConfiguredProvider,
  type AiProviderConfig,
}

/// agent 扩展自管配置（持久化至 extensions/agent/config.json）。
/// AI 凭证条目在 `config/ai-providers`；**本扩展自选** provider/key/model。
export const config = defineConfig('extensions/agent/config', {
  // 资源上限（Rust 端强制 clamp，config.json 越界无效；BOUNDS 见下方）
  maxCpuSeconds: 30,
  maxMemoryMb: 512,
  maxOpenFiles: 64,
  executionTimeout: 30,
  maxOutputBytes: 1048576,
  maxTurns: 10,
  /**
   * 本扩展选用的模型：`providerId::keyId::model`（或旧式 `providerId::model`）。
   * 中枢不存 active；换消费者互不影响。
   */
  providerModelKey: '',
  systemPrompt: [
    '你是全能的 AI Agent，运行在用户的 macOS 上。你的职责是帮助用户完成日常任务：回答问题、查找信息、操作文件、执行命令。',
    '',
    '# 工具使用规则',
    '',
    '你有两个工具可用：',
    '- `web_search(query)`: 联网搜索。当用户问事实性/时事性问题、或需要外部知识时使用。不要对能从上下文推断答案的问题使用。',
    '- `run_command(cmd, args)`: 在用户 macOS 上执行 shell 命令（不经过 shell，参数数组传递）。可用于浏览文件、查询系统信息、编辑文件、git 操作等。',
    '',
    '工具调用原则：',
    '- 简单问题直接回答，不要为了"用工具"而用工具',
    '- 复杂任务可以连续多次调用工具（每次拿到结果后判断是否需要下一步）',
    '- 工具结果可能被净化（secret 替换为 [REDACTED]），这是正常的安全防护',
    '',
    '# 安全约束',
    '',
    '- 不要执行破坏性操作（如 `rm -rf /`），这类命令会被断路器拦截',
    '- 不要读取或外泄用户敏感数据（API key、SSH key、密码等），输出会被自动打码',
    '',
    '# 输出风格',
    '',
    '- 简洁直接，避免冗长铺垫',
    '- 代码/命令用 markdown 代码块包裹',
    '- 中文为主（除非用户用英文提问）',
    '- 少用 emoji',
  ].join('\n'),
  searchProvider: {
    type: 'tavily',
    apiKey: '',
  } as SearchProviderConfig,
})

/// 资源上限 floor/cap 镜像（权威在 native/policy.rs，须手动同步）。
/// 无 Settings UI：仅供 check:agent-bounds CI 文本校验；运行时由 Rust clamp，不信任前端/config 传值。
export const BOUNDS = {
  maxTurns: { floor: 1, cap: 50 },
  maxCpuSeconds: { floor: 1, cap: 300 },
  maxMemoryMb: { floor: 64, cap: 4096 },
  maxOpenFiles: { floor: 8, cap: 1024 },
  executionTimeout: { floor: 1, cap: 300 },
  maxOutputBytes: { floor: 1024, cap: 10485760 },
} as const

// ─── 搜索提供商 helpers ──────────────────────────────────────

/// 更新搜索提供商配置（单对象，直接 Object.assign）。
export function updateSearchProvider(partial: Partial<SearchProviderConfig>) {
  Object.assign(config.searchProvider, partial)
}

// ─── AI 选用（消费者侧）────────────────────────────────────

export function setProviderModelKey(key: string) {
  config.providerModelKey = key
}

/** 兼容旧调用名 */
export const setActiveProviderModelKey = setProviderModelKey

export function agentSelection() {
  return parseSelectionKey(config.providerModelKey)
}

export function resolveAgentCredentials(): ResolvedAiCredentials | null {
  const { providerId, keyId, model } = agentSelection()
  return resolveCredentials({ providerId, keyId, model })
}

export async function resolveAgentRuntimeCredentials(): Promise<ResolvedAiCredentials | null> {
  const { providerId, keyId, model } = agentSelection()
  return resolveRuntimeCredentials({ providerId, keyId, model })
}

/** 当前选用在配置中可完整解析（不含 env 兜底）。 */
export const isAgentProviderReady = computed(() => !!resolveAgentCredentials())

/** @deprecated 用 isAgentProviderReady */
export const isConfigProviderReady = isAgentProviderReady
export const isProviderReady = isAgentProviderReady

// 删提供商 / Key 时清悬空选用
onAiProvidersChange((e) => {
  const sel = parseSelectionKey(config.providerModelKey)
  if (!sel.providerId) return
  if (e.kind === 'remove-provider' && sel.providerId === e.providerId) {
    config.providerModelKey = ''
  } else if (
    e.kind === 'remove-key' &&
    sel.providerId === e.providerId &&
    sel.keyId === e.keyId
  ) {
    config.providerModelKey = ''
  }
})

// 一次性：旧 activeProviderModelKey → providerModelKey
void whenConfigReady('extensions/agent/config').then(async () => {
  if (config.providerModelKey.trim()) return
  if (!isTauri) return
  try {
    const store = await load('extensions/agent/config.json')
    const active = await store.get<unknown>('activeProviderModelKey')
    if (typeof active === 'string' && active.trim()) {
      config.providerModelKey = active.trim()
      try {
        await store.delete('activeProviderModelKey')
        await store.save()
      } catch {
        /* ignore */
      }
    }
  } catch (e) {
    console.warn('[agent] legacy selection migrate skipped:', e)
  }
})
