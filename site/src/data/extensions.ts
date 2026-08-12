// 扩展矩阵数据已合并到 i18n 字典（zh/en 双语同构）。
// 本文件仅保留类型 + 便捷访问器，供 ExtensionMatrix 组件消费。

import { getDict, totalExtensions, type Lang } from '../i18n/translations'

export interface ExtItem {
  id: string
  name: string
  desc: string
  icon: string
}

export interface ExtCluster {
  title: string
  items: ExtItem[]
}

export function getClusters(lang: Lang): ExtCluster[] {
  return getDict(lang).extensions.clusters
}

export { totalExtensions }
