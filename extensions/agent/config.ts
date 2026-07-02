import { computed } from 'vue'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

export interface AiProviderConfig {
  id: string
  endpoint: string
  apiKey: string
  models: string[]
}

export interface SearchProviderConfig {
  type: 'tavily'
  apiKey: string
}

/// agent 扩展自管配置（持久化至 extensions/agent/config.json）。
/// 含资源上限、systemPrompt、搜索提供商、AI Provider（多 provider + 激活选择）。
export const config = defineConfig('extensions/agent/config', {
  // 资源上限（Rust 端强制 clamp，config.json 越界无效；BOUNDS 见下方）
  maxCpuSeconds: 30,
  maxMemoryMb: 512,
  maxOpenFiles: 64,
  executionTimeout: 30,
  maxOutputBytes: 1048576,
  maxTurns: 10,
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
  // 不变量：aiProviders 始终 ≥1 项（removeAiProvider 删空时补默认项），
  // activeProviderConfig 的非空断言依赖此不变量。
  aiProviders: [
    { id: generateRequestId(), endpoint: '', apiKey: '', models: [] },
  ] as AiProviderConfig[],
  activeProviderModelKey: '',
})

/// 资源上限 UI 镜像（权威在 native/policy.rs，须手动同步）。
/// 仅用于 check:agent-bounds CI 校验（BOUNDS ↔ policy.rs 双向一致）；Rust 端 clamp 兜底，不信任此值。
export const BOUNDS = {
  maxTurns: { floor: 1, cap: 50 },
  maxCpuSeconds: { floor: 1, cap: 300 },
  maxMemoryMb: { floor: 64, cap: 4096 },
  maxOpenFiles: { floor: 8, cap: 1024 },
  executionTimeout: { floor: 1, cap: 300 },
  maxOutputBytes: { floor: 1024, cap: 10485760 },
} as const

// ─── AI Provider helpers ─────────────────────────────────────

function parseActiveConfig<T>(
  key: string,
  configs: T[],
  matchFallback?: (configs: T[]) => T | undefined,
): T | undefined {
  const sep = key.indexOf('::')
  if (sep !== -1) {
    const id = key.substring(0, sep)
    const found = (configs as Array<{ id: string } & T>).find((c) => c.id === id)
    if (found) return found
  }
  return matchFallback?.(configs)
}

/// 当前激活的 provider（未匹配 key 时回退第一项；aiProviders ≥1 不变量保证非空）。
export const activeProviderConfig = computed<AiProviderConfig>(
  () => parseActiveConfig(config.activeProviderModelKey, config.aiProviders, (c) => c[0])!,
)

export function setActiveProviderModelKey(key: string) {
  config.activeProviderModelKey = key
}

/// 新增空 provider 并切为激活。返回新 id。
export function addAiProvider(): string {
  const id = generateRequestId()
  config.aiProviders.push({ id, endpoint: '', apiKey: '', models: [] })
  config.activeProviderModelKey = `${id}::`
  return id
}

/// 删除指定 provider；删空时补默认项维持 ≥1 不变量；激活项被删时回退第一项。
export function removeAiProvider(id: string) {
  const idx = config.aiProviders.findIndex((c) => c.id === id)
  if (idx === -1) return
  config.aiProviders.splice(idx, 1)
  if (config.aiProviders.length === 0) {
    config.aiProviders.push({ id: generateRequestId(), endpoint: '', apiKey: '', models: [] })
  }
  if (config.activeProviderModelKey.startsWith(`${id}::`)) {
    config.activeProviderModelKey = `${config.aiProviders[0].id}::`
  }
}

/// 部分更新指定 provider。
export function updateAiProvider(id: string, partial: Partial<AiProviderConfig>) {
  const target = config.aiProviders.find((c) => c.id === id)
  if (!target) return
  Object.assign(target, partial)
}

// ─── 搜索提供商 helpers ──────────────────────────────────────

/// 更新搜索提供商配置（单对象，直接 Object.assign）。
export function updateSearchProvider(partial: Partial<SearchProviderConfig>) {
  Object.assign(config.searchProvider, partial)
}
