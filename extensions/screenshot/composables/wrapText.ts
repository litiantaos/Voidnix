/**
 * 基于 Canvas measureText 的文本换行，行为模拟
 * white-space: pre-wrap; overflow-wrap: break-word;
 */
const sharedCanvas = document.createElement('canvas')

export function wrapText(text: string, maxWidth: number, font: string): string[] {
  const ctx = sharedCanvas.getContext('2d')!
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

/// 逐行实测最大行宽（DOM 呈现层与画布底色块共用同一宽度来源）
export function textMaxLineWidth(lines: string[], font: string): number {
  const ctx = sharedCanvas.getContext('2d')!
  ctx.font = font
  let maxW = 0
  for (const line of lines) maxW = Math.max(maxW, ctx.measureText(line).width)
  return maxW
}
