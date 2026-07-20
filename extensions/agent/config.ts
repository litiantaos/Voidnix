import { computed } from 'vue'
import { load } from '@tauri-apps/plugin-store'
import { defineConfig, whenConfigReady } from '@/runtime/storage'
import { isTauri } from '@/utils/tauri'
import {
  config as aiProvidersConfig,
  modelSelectOptions,
  parseSelectionKey,
  formatSelectionKey,
  getProviderById,
  getKeySlot,
  resolveCredentials,
  resolveRuntimeCredentials,
  hasAnyConfiguredProvider,
  providerDisplayName,
  isCredentialSelectionValid,
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
    '- 简单问题直接回答，不要为了“用工具”而用工具',
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
    '- 禁用 emoji',
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

function selectionRefValid(providerId: string, keyId: string, model: string): boolean {
  return isCredentialSelectionValid({
    providerId,
    keyId: keyId || undefined,
    model,
  })
}

/**
 * 合法选用 → 规范三段串 `providerId::keyId::model`（旧式两段补 first keyId）。
 * 无效返回 null。
 */
function canonicalizeSelectionRaw(raw: string): string | null {
  const sel = parseSelectionKey(raw)
  if (!selectionRefValid(sel.providerId, sel.keyId, sel.model)) return null
  const p = getProviderById(sel.providerId)
  if (!p) return null
  const slot = getKeySlot(p, sel.keyId || undefined)
  if (!slot) return null
  return formatSelectionKey(sel.providerId, slot.id, sel.model)
}

export function setProviderModelKey(key: string) {
  const t = String(key).trim()
  if (!t) {
    config.providerModelKey = ''
    return
  }
  // 写入时只接受与中枢一致的选用，并规范为三段（冷路径干净）
  const canon = canonicalizeSelectionRaw(t)
  if (!canon) {
    console.warn('[agent] ignore invalid providerModelKey:', t)
    return
  }
  config.providerModelKey = canon
}

/** 兼容旧调用名 */
export const setActiveProviderModelKey = setProviderModelKey

/** 原始落盘串；UI/解析请用 effectiveProviderModelKey。 */
export function agentSelection() {
  return parseSelectionKey(config.providerModelKey)
}

/**
 * 读时有效选用串：与中枢不一致则视为未选；合法则规范为三段（热路径不写回）。
 * 依赖 hub providers，中枢变更后下拉/就绪态立刻正确。
 */
export const effectiveProviderModelKey = computed(() => {
  void aiProvidersConfig.providers
  const raw = config.providerModelKey.trim()
  if (!raw) return ''
  return canonicalizeSelectionRaw(raw) ?? ''
})

export function resolveAgentCredentials(): ResolvedAiCredentials | null {
  const raw = effectiveProviderModelKey.value
  if (!raw) return null
  const { providerId, keyId, model } = parseSelectionKey(raw)
  return resolveCredentials({ providerId, keyId, model })
}

/**
 * 运行时凭证：有有效选用则按选用解析（缺项 env 补全）；
 * 无选用时走纯 env（OPENAI_* / ai.env），与 View「env 兜底可用即可对话」一致。
 */
export async function resolveAgentRuntimeCredentials(): Promise<ResolvedAiCredentials | null> {
  const raw = effectiveProviderModelKey.value
  if (!raw) return resolveRuntimeCredentials({})
  const { providerId, keyId, model } = parseSelectionKey(raw)
  return resolveRuntimeCredentials({ providerId, keyId, model })
}

/** 当前选用在配置中可完整解析（不含 env 兜底）。 */
export const isAgentProviderReady = computed(() => !!resolveAgentCredentials())

/** @deprecated 用 isAgentProviderReady */
export const isConfigProviderReady = isAgentProviderReady
export const isProviderReady = isAgentProviderReady

/**
 * 冷路径：悬空清空；合法旧式两段写回三段。热路径用 effectiveProviderModelKey。
 */
export function pruneAgentSelection() {
  const raw = config.providerModelKey.trim()
  if (!raw) return
  const canon = canonicalizeSelectionRaw(raw)
  if (!canon) {
    config.providerModelKey = ''
    return
  }
  if (canon !== raw) config.providerModelKey = canon
}

// 一次性：旧 activeProviderModelKey → providerModelKey；加载后冷 prune
void Promise.all([
  whenConfigReady('extensions/agent/config'),
  whenConfigReady('config/ai-providers'),
]).then(async () => {
  if (!config.providerModelKey.trim() && isTauri) {
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
  }
  pruneAgentSelection()
})
