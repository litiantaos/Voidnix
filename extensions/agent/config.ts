import { defineConfig } from '@/runtime/storage'

/// agent 扩展自管配置（持久化至 extensions/agent/config.json）。
/// 注：searchProviders 暂留 settings.ts，因使用 createConfigManager 管理。
export const config = defineConfig('agent', {
  trustedCommands: [
    'ls', 'cat', 'pwd', 'echo', 'head', 'tail', 'wc', 'file', 'stat', 'date',
    'which', 'whoami', 'uname', 'find', 'grep', 'rg', 'fd', 'ag', 'tree', 'diff',
    'comm', 'cmp', 'md5sum', 'shasum', 'mkdir', 'touch', 'cp', 'mv', 'ln', 'tee',
    'truncate', 'sed', 'awk', 'sort', 'uniq', 'cut', 'tr', 'paste', 'expand',
    'jq', 'yq', 'bat', 'git',
  ],
  systemPrompt: '',
})
