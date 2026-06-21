import { defineConfig } from '@/runtime/storage'

export interface SearchProviderConfig {
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
    '- 不要尝试执行破坏性操作（如 `rm -rf /`、覆盖系统文件），这些会被硬拦',
    '- 不要读取或外泄用户敏感数据（API key、SSH key、密码等），输出会被自动打码',
    '- 危险命令需要用户审批，被拒后换方案而不是反复尝试',
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

/// 安全底线 UI 镜像（权威在 native/policy.rs，⚠️ 须手动同步）。
/// 不变量：floor 必须 ⊇ Rust 端 FORBIDDEN_FLOOR / DENIED_ARG_FLOOR（迁移即取并集，禁止缩窄）。
/// 仅用于 check:agent-bounds CI 校验（BOUNDS ↔ policy.rs 双向一致）；Rust 端并集兜底，不信任此值。
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

/// 更新搜索提供商配置（单对象，直接 Object.assign）。
export function updateSearchProvider(partial: Partial<SearchProviderConfig>) {
  Object.assign(config.searchProvider, partial)
}
