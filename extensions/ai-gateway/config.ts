import { defineConfig } from '@/runtime/storage'

/**
 * 别名选用:`providerId::model`(网关按模型名路由,Key 由轮换决定,无需 keyId)。
 * CC 的 opus/sonnet/haiku 三档别名经 ANTHROPIC_DEFAULT_*_MODEL 解析为实际模型 ID 后到达网关。
 */
export interface AliasSelection {
  providerId: string
  model: string
}

/**
 * CC 上下文档位(UI 下拉直存):default = 裸名;档值 = 模型 id 统一追加 `[<tier>]` 后缀
 * (档值即后缀内容,新增档位仅扩本联合 + UI 选项各一行)
 */
export type ContextTier = 'default' | '1m'

/// ai-gateway 扩展自管配置(持久化至 extensions/ai-gateway/config.json)。
export const config = defineConfig('extensions/ai-gateway/config', {
  /** 总开关:开 = 起网关供所有工具接入;关 = 停网关(接管态随之还原) */
  enabled: false,
  /** CC 接管开关:与 enabled 独立,两者都开才改写 CC settings.json 自有键;关 = 按快照还原 */
  ccTakeover: false,
  /** CC 上下文档位:写入 CC 的模型 id 形态(1m = 追加 [1m],CC 识别后发长上下文 beta 头) */
  ccContext: '1m' as ContextTier,
  /** Sonnet 档模型(CC 新会话开局默认;空 = 不写该键,启用时自动兜底首个可路由模型) */
  sonnet: null as AliasSelection | null,
  /** Opus 档模型(CC 旗舰档,手动切换;空 = 不写该键,启用时自动兜底) */
  opus: null as AliasSelection | null,
  /** Haiku 档模型(CC 后台小任务:标题生成等;空 = 不写该键,后台走主模型) */
  haiku: null as AliasSelection | null,
})
