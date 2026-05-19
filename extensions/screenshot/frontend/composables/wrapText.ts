/**
 * 基于 Canvas measureText 的文本换行，行为模拟
 * white-space: pre-wrap; overflow-wrap: break-word;
 */
export function wrapText(text: string, maxWidth: number, font: string): string[] {
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  ctx.font = font

  const lines: string[] = []
  const paragraphs = text.split('\n')

  for (const paragraph of paragraphs) {
    let currentLine = ''
    // 按字符迭代以支持 CJK（无空格）和 break-word 行为
    const chars = Array.from(paragraph)

    for (const char of chars) {
      const testLine = currentLine + char
      if (ctx.measureText(testLine).width > maxWidth && currentLine !== '') {
        lines.push(currentLine)
        currentLine = char
      } else {
        currentLine = testLine
      }
    }
    if (currentLine !== '') {
      lines.push(currentLine)
    }
  }

  return lines
}

export interface TextMetrics {
  fontSize: number
  lineHeight: number
  ascent: number
  descent: number
  halfLeading: number
}

/**
 * 精确测量字体 metrics，计算 CSS half-leading。
 * Canvas textBaseline='top' 的 y 坐标 + halfLeading = CSS line-box 内的 ascent 顶部。
 */
export function measureTextMetrics(
  fontSize: number,
  lineHeight: number,
  font: string,
): TextMetrics {
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  ctx.font = font

  // 用包含升部/降部的字符测量，fallback 到 actualBoundingBox
  const m = ctx.measureText('Äg')
  const ascent =
    (m as unknown as { fontBoundingBoxAscent?: number }).fontBoundingBoxAscent ??
    m.actualBoundingBoxAscent
  const descent =
    (m as unknown as { fontBoundingBoxDescent?: number }).fontBoundingBoxDescent ??
    m.actualBoundingBoxDescent

  return {
    fontSize,
    lineHeight,
    ascent,
    descent,
    halfLeading: (lineHeight - ascent - descent) / 2,
  }
}
