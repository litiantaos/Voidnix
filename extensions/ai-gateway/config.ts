import { defineConfig } from '@/runtime/storage'

/**
 * 别名选用:`providerId::model`(网关按模型名路由,Key 由轮换决定,无需 keyId)。
 * CC 的 opus/sonnet/haiku 三档别名经 ANTHROPIC_DEFAULT_*_MODEL 解析为实际模型 ID 后到达网关。
 */
export interface AliasSelection {
  providerId: string
  model: string
}

/// ai-gateway 扩展自管配置(持久化至 extensions/ai-gateway/config.json)。
export const config = defineConfig('extensions/ai-gateway/config', {
  /** 总开关:开 = 起网关供所有工具接入;关 = 停网关(接管态随之还原) */
  enabled: false,
  /** CC 接管开关:与 enabled 独立,两者都开才改写 CC settings.json 自有键;关 = 按快照还原 */
  ccTakeover: false,
  /** 新会话默认模型(CC sonnet 档别名;空 = 不写该键,启用时自动兜底首个可路由模型) */
  sonnet: null as AliasSelection | null,
  /** 后台任务模型(CC haiku 档别名,标题生成等小流量;空 = 不写该键,后台走主模型) */
  haiku: null as AliasSelection | null,
})
