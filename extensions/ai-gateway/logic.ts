import type { AiProvider } from '@/runtime/ai-providers'
import { providerDisplayName } from '@/runtime/ai-providers'
import type { AliasSelection } from './config'

/**
 * 网关端口:release 8788(固定)/ dev 8789(dev 与 release 并存不互抢)。
 * 与 Rust `server.rs::PORT` 同源约定,双端手动同步。
 */
export const GATEWAY_PORT: number = import.meta.env.DEV ? 8789 : 8788

/// 与 Rust `GatewayRoute` 同构(camelCase 直接过 serde)
export interface GatewayKey {
  label: string
  apiKey: string
}

export interface GatewayRoute {
  providerId: string
  name: string
  anthropicUrl: string
  responsesUrl: string
  chatUrl: string
  models: string[]
  keys: GatewayKey[]
}

/** 路由表中一个可路由模型(经哪些协议可达) */
export interface RoutableModel {
  model: string
  providerId: string
  providerName: string
  anthropic: boolean
  responses: boolean
  chat: boolean
}

/**
 * 中枢 → 网关路由表:三种协议端点(chat = endpoint)全空才不参与,另需至少一把非空 Key
 * 与至少一个模型;模型与 Key 全量带入(轮换在网关侧,热生效)。
 */
export function buildRoutes(providers: AiProvider[]): GatewayRoute[] {
  const routes: GatewayRoute[] = []
  for (const p of providers) {
    const anthropicUrl = p.anthropicEndpoint?.trim() ?? ''
    const responsesUrl = p.responsesEndpoint?.trim() ?? ''
    const chatUrl = p.endpoint?.trim() ?? ''
    const keys = (p.keys ?? [])
      .filter((k) => k.apiKey.trim())
      .map((k) => ({ label: k.label?.trim() || 'Key', apiKey: k.apiKey.trim() }))
    const models = (p.models ?? []).map((m) => m.trim()).filter(Boolean)
    if ((!anthropicUrl && !responsesUrl && !chatUrl) || keys.length === 0 || models.length === 0) {
      continue
    }
    routes.push({
      providerId: p.id,
      name: providerDisplayName(p),
      anthropicUrl,
      responsesUrl,
      chatUrl,
      models,
      keys,
    })
  }
  return routes
}

/** 全部可路由模型(同提供商分组序);CC 侧面板与别名选择共用 */
export function routableModels(routes: GatewayRoute[]): RoutableModel[] {
  const out: RoutableModel[] = []
  for (const r of routes) {
    for (const m of r.models) {
      out.push({
        model: m,
        providerId: r.providerId,
        providerName: r.name,
        anthropic: !!r.anthropicUrl,
        responses: !!r.responsesUrl,
        chat: !!r.chatUrl,
      })
    }
  }
  return out
}

/** Anthropic 协议可路由模型(CC 只走 /v1/messages,别名与 modelPicker 仅取此集) */
export function anthropicModels(models: RoutableModel[]): RoutableModel[] {
  return models.filter((m) => m.anthropic)
}

/** 别名是否仍有效:所选提供商的模型仍在路由表且 Anthropic 可达 */
export function isAliasValid(models: RoutableModel[], sel: AliasSelection | null): boolean {
  if (!sel) return false
  return models.some(
    (m) => m.providerId === sel.providerId && m.model === sel.model.trim() && m.anthropic,
  )
}

/**
 * 别名有效性过滤(读时不写回):失效(提供商删/模型改名/端点摘除)返回 null,
 * 调用方在冷路径决定是否落盘兜底值。
 */
export function effectiveAlias(
  models: RoutableModel[],
  sel: AliasSelection | null,
): AliasSelection | null {
  return isAliasValid(models, sel) ? sel : null
}

/** CC 接线载荷(与 Rust `CcApplyPayload` 同构);无 Anthropic 可路由模型时返回 null */
export interface CcApplyPayload {
  port: number
  sonnetModel: string
  haikuModel: string
  pickerRows: { model: string; label: string }[]
}

export function buildCcPayload(
  routes: GatewayRoute[],
  aliases: {
    sonnet: AliasSelection | null
    haiku: AliasSelection | null
  },
  /** 网关实际端口(sync 响应携带,Rust 是权威源) */
  port: number,
): CcApplyPayload | null {
  const anthropic = anthropicModels(routableModels(routes))
  if (anthropic.length === 0) return null
  const modelOf = (sel: AliasSelection | null) =>
    sel && isAliasValid(anthropic, sel) ? sel.model.trim() : ''
  return {
    port,
    sonnetModel: modelOf(aliases.sonnet),
    haikuModel: modelOf(aliases.haiku),
    pickerRows: anthropic.map((m) => ({ model: m.model, label: m.model })),
  }
}

/** 启用网关时的别名兜底:两档全空 → 首个 Anthropic 模型(用户可改) */
export function fallbackAliases(models: RoutableModel[]): {
  sonnet: AliasSelection | null
  haiku: AliasSelection | null
} {
  const first = anthropicModels(models)[0]
  if (!first) {
    return { sonnet: null, haiku: null }
  }
  const sel: AliasSelection = { providerId: first.providerId, model: first.model }
  return { sonnet: sel, haiku: sel }
}
