export function getParentPath(path: unknown): string {
  if (typeof path !== 'string' || !path) return ''
  const lastSlashIndex = path.lastIndexOf('/')
  if (lastSlashIndex === -1) return path
  if (lastSlashIndex === 0) return '/' // 根目录
  return path.substring(0, lastSlashIndex)
}

export function formatPathParts(path: unknown): { head: string; tail: string } {
  if (typeof path !== 'string' || !path) return { head: '', tail: '' }

  // 将 macOS 主目录替换为 ~
  const displayPath = path.replace(/^\/Users\/[^/]+/, '~')

  const lastSlashIndex = displayPath.lastIndexOf('/')

  if (lastSlashIndex === -1 || lastSlashIndex === 0) {
    return { head: displayPath, tail: '' }
  }

  return {
    head: displayPath.substring(0, lastSlashIndex + 1),
    tail: displayPath.substring(lastSlashIndex + 1),
  }
}
