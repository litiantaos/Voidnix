const MAX_SIZE = 200

const iconCache = new Map<string, string>()

export function getCachedIcon(path: string): string | undefined {
  const value = iconCache.get(path)
  if (value !== undefined) {
    iconCache.delete(path)
    iconCache.set(path, value)
  }
  return value
}

export function setCachedIcon(path: string, icon: string): void {
  if (iconCache.has(path)) {
    iconCache.delete(path)
  } else if (iconCache.size >= MAX_SIZE) {
    const lruKey = iconCache.keys().next().value
    if (lruKey) iconCache.delete(lruKey)
  }
  iconCache.set(path, icon)
}
