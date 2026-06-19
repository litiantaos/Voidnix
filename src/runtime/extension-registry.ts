import type { Extension } from './types'

// 扩展注册中心。main.ts 通过 import.meta.glob eager 扫描 extensions/*/index.ts，
// 各 index.ts 顶层调 defineExtension({...}) 完成注册。

const extensions: Extension[] = []

/** 声明一个扩展并注册到注册中心。返回原对象便于 `export default defineExtension({...})`。 */
export function defineExtension(ext: Extension): Extension {
  if (extensions.some((e) => e.meta.id === ext.meta.id)) {
    console.warn(`[defineExtension] duplicate extension id '${ext.meta.id}', already registered`)
  }
  extensions.push(ext)
  return ext
}

/** 所有已注册扩展（注册顺序）。返回副本，防止外部篡改注册表状态。 */
export function getAllExtensions(): readonly Extension[] {
  return extensions
}

/** 按 meta.id 查找。 */
export function getExtension(id: string): Extension | undefined {
  return extensions.find((e) => e.meta.id === id)
}

/** 仅测试用：清空注册表，避免模块级状态跨用例污染。 */
export function _resetForTest(): void {
  extensions.length = 0
}
