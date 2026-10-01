import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'ai-gateway.sectionGateway': { 'zh-CN': '网关', en: 'Gateway' },
  'ai-gateway.sectionCc': { 'zh-CN': 'Claude Code', en: 'Claude Code' },
  'ai-gateway.sectionRoutes': { 'zh-CN': '可路由模型', en: 'Routable Models' },
  'ai-gateway.enable': { 'zh-CN': '启用网关', en: 'Enable Gateway' },
  'ai-gateway.ccTakeover': { 'zh-CN': '接管 Claude Code', en: 'Take over Claude Code' },
  'ai-gateway.ccTakeoverHint': {
    'zh-CN': '改写 ~/.claude/settings.json 的接入键(需网关运行中),关闭自动还原原配置',
    en: 'Rewires ~/.claude/settings.json entry keys (gateway must be running); off restores the original',
  },
  'ai-gateway.ccError': {
    'zh-CN': '接管失败:{msg}',
    en: 'Takeover failed: {msg}',
  },
  'ai-gateway.cc1mContext': { 'zh-CN': '1M 上下文', en: '1M Context' },
  'ai-gateway.cc1mContextHint': {
    'zh-CN': '写入 CC 的模型 id 统一追加 [1m] 后缀,新会话启用百万 token 上下文(提供商需支持)',
    en: 'Appends the [1m] suffix to model ids written to CC, enabling 1M-token context in new sessions (provider support required)',
  },
  'ai-gateway.statusLoading': { 'zh-CN': '获取状态中…', en: 'Loading status…' },
  'ai-gateway.statusRunning': {
    'zh-CN': '运行中 · 127.0.0.1:{port} · {n} 提供商',
    en: 'Running · 127.0.0.1:{port} · {n} providers',
  },
  'ai-gateway.statusStopped': { 'zh-CN': '已停止', en: 'Stopped' },
  'ai-gateway.statusError': { 'zh-CN': '启动失败:{msg}', en: 'Failed to start: {msg}' },
  'ai-gateway.configGuide': { 'zh-CN': '配置说明', en: 'Config Guide' },
  'ai-gateway.usageGuide': { 'zh-CN': '使用说明', en: 'Usage Guide' },
  'ai-gateway.helpMarkdown': {
    'zh-CN': `开启后,配置好的所有提供商和模型统一从本地地址提供服务,多把 Key 自动轮换,额度满了无感换下一把。

**Claude Code**:需同时打开「接管 Claude Code」开关才会改写其配置(需网关运行中),重开会话生效;之后在 CC 里 **/model** 随时切换任意模型。关闭接管开关自动还原原配置。

**其它工具**:任何 OpenAI 或 Anthropic 兼容的工具,把 API 地址改为 \`http://127.0.0.1:{port}\`,Key 随便填即可。

提供商、Key 与模型的增删都在「AI 提供商」里进行,保存即生效,工具无需重启。`,
    en: `Once enabled, all your providers and models are served from a single local address; multiple keys rotate automatically, so exhausted quota switches keys seamlessly.

**Claude Code**: wiring only happens when "Take over Claude Code" is also on (gateway must be running); it takes effect in new sessions. Switch models anytime with **/model**. Turning takeover off restores your original config.

**Other tools**: point any OpenAI- or Anthropic-compatible tool at \`http://127.0.0.1:{port}\` with any placeholder API key.

Providers, keys and models are all managed in AI Providers; changes apply instantly with no restart.`,
  },
  'ai-gateway.defaultsHint': {
    'zh-CN': '低频设置:仅决定新会话开局与后台小流量用哪个模型,不限制 /model 可选范围',
    en: 'Low-frequency: only the initial model of new sessions and background traffic; /model range is unaffected',
  },
  'ai-gateway.aliasDefault': { 'zh-CN': '新会话默认模型', en: 'New-session default model' },
  'ai-gateway.aliasBackground': {
    'zh-CN': '后台任务模型',
    en: 'Background-task model',
  },
  'ai-gateway.noAnthropicModel': {
    'zh-CN': '无 Anthropic 可路由模型',
    en: 'No Anthropic-routable models',
  },
  'ai-gateway.keysCount': { 'zh-CN': '{n} 把 Key 轮换', en: '{n} keys rotating' },
  'ai-gateway.emptyRoute': {
    'zh-CN': '无可路由提供商:在 AI 提供商中配置 API URL 与 Key',
    en: 'No routable providers: configure the API URL and keys in AI Providers',
  },
})
