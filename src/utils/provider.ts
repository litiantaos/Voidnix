export function providerLabelFromUrl(url: string, fallback: string): string {
  if (!url) return fallback
  try {
    const parts = new URL(url).hostname.split('.')
    if (parts.length >= 2) return parts[parts.length - 2].toUpperCase()
    return parts[0].toUpperCase()
  } catch {
    return fallback
  }
}
