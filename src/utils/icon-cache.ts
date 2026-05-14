// 真正的 LRU 缓存：Map 的迭代顺序是插入顺序，每次访问时删除再重新插入，使其移到末尾
// 淘汰时删除 keys().next()（最久未访问的头部元素）
const MAX_SIZE = 200

const iconCache = new Map<string, string>()

export function getCachedIcon(path: string): string | undefined {
  const value = iconCache.get(path)
  if (value !== undefined) {
    // 访问时移到末尾，维持 LRU 语义
    iconCache.delete(path)
    iconCache.set(path, value)
  }
  return value
}

export function setCachedIcon(path: string, icon: string): void {
  if (iconCache.has(path)) {
    // 已存在则先删除，再插入到末尾
    iconCache.delete(path)
  } else if (iconCache.size >= MAX_SIZE) {
    // 淘汰最久未访问的条目（Map 头部）
    const lruKey = iconCache.keys().next().value
    if (lruKey) iconCache.delete(lruKey)
  }
  iconCache.set(path, icon)
}

export function clearIconCache(): void {
  iconCache.clear()
}
