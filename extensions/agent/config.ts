import { computed } from 'vue'
import { load } from '@tauri-apps/plugin-store'
import { defineConfig, whenConfigReady } from '@/runtime/storage'
import { isTauri } from '@/utils/tauri'
import type { AgentMessage } from '@/types/agent'
import {
  config as aiProvidersConfig,
  modelSelectOptions,
  parseSelectionKey,
  formatSelectionKey,
  getProviderById,
  getKeySlot,
  resolveCredentials,
  isCredentialSelectionValid,
  type AiProvider as AiProviderConfig,
  type ResolvedAiCredentials,
} from '@/runtime/ai-providers'

export interface SearchProviderConfig {
  type: 'tavily'
  apiKey: string
}

/// 系统提示词默认值（Settings 重置按钮的单一真相源）
export const DEFAULT_SYSTEM_PROMPT = [
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
].join('\n')

// 再导出中枢工具；选用状态在 agent 本扩展 config
export { modelSelectOptions, type AiProviderConfig }

/// 扩展 config 存储路径（agent.ts 会话恢复依赖 whenConfigReady 回填时序）
export const AGENT_CONFIG_PATH = 'extensions/agent/config'

/// agent 扩展自管配置（持久化至 extensions/agent/config.json）。
/// AI 凭证条目在 `config/ai-providers`；**本扩展自选** provider/key/model。
export const config = defineConfig(AGENT_CONFIG_PATH, {
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
  systemPrompt: DEFAULT_SYSTEM_PROMPT,
  searchProvider: {
    type: 'tavily',
    apiKey: '',
  } as SearchProviderConfig,
  /** 对话消息（随会话持久化）：hide_window 触发的 WebContent 内存超阈值 navigate 重载会清零 JS 单例，boot 回填后由 agent.ts 恢复；新会话清空 */
  messages: [] as AgentMessage[],
  /** 进行中 run 的 sessionId：重载恢复时 abort Rust 侧孤儿 run（run 已结束则 no-op） */
  sessionId: '',
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

/**
 * 无显式选用时默认首个可用提供商（endpoint + 非空 key + 模型齐备）；列表无可用则空。
 * 读时推导，不写回 providerModelKey。
 */
const firstProviderSelection = computed(() => {
  void aiProvidersConfig.providers
  for (const p of aiProvidersConfig.providers) {
    if (!p.endpoint.trim()) continue
    const slot = p.keys.find((k) => k.apiKey.trim())
    const model = p.models.find((m) => m.trim())
    if (slot && model) return formatSelectionKey(p.id, slot.id, model.trim())
  }
  return ''
})

/**
 * 读时有效选用串：显式选用合法则规范三段；否则回退首个可用提供商（读时，不写回）。
 * 依赖 hub providers：中枢删选/清空后自动落到（新的）首个，下拉/就绪态即时正确。
 */
export const effectiveProviderModelKey = computed(() => {
  void aiProvidersConfig.providers
  const raw = config.providerModelKey.trim()
  const canon = raw ? canonicalizeSelectionRaw(raw) : ''
  return canon || firstProviderSelection.value
})

/** 凭证解析：按有效选用取值（无显式选用时即首个可用提供商）；中枢无可用则 null。 */
export function resolveAgentCredentials(): ResolvedAiCredentials | null {
  const sel = effectiveProviderModelKey.value
  if (!sel) return null
  const { providerId, keyId, model } = parseSelectionKey(sel)
  return resolveCredentials({ providerId, keyId, model })
}

/** 当前选用在配置中可完整解析（不含 env 兜底）。 */
export const isAgentProviderReady = computed(() => !!resolveAgentCredentials())

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
void Promise.all([whenConfigReady(AGENT_CONFIG_PATH), whenConfigReady('config/ai-providers')]).then(
  async () => {
    if (!config.providerModelKey.trim() && isTauri) {
      try {
        const store = await load(`${AGENT_CONFIG_PATH}.json`)
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
  },
)
