import type { Extension } from './types'

// 扩展注册中心。main.ts 通过 import.meta.glob eager 扫描 extensions/*/index.ts，
// 各 index.ts 顶层调 defineExtension({...}) 完成注册。

const extensions: Extension[] = []

/** 声明一个扩展并注册到注册中心。返回原对象便于 `export default defineExtension({...})`。 */
export function defineExtension(ext: Extension): Extension {
  extensions.push(ext)
  return ext
}

/** 所有已注册扩展（注册顺序）。 */
export function getAllExtensions(): Extension[] {
  return extensions
}

/** 按 meta.id 查找。 */
export function getExtension(id: string): Extension | undefined {
  return extensions.find((e) => e.meta.id === id)
}
