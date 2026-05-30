export function toErrorMessage(e: unknown, fallback = '未知错误'): string {
  return e instanceof Error ? e.message || fallback : fallback
}
