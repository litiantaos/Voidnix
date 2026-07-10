#!/usr/bin/env bun
// 校验 extensions/*/index.ts 中 meta.order 在非 hidden 扩展间唯一。
import { readdirSync, readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const EXT_DIR = join(process.cwd(), 'extensions')
const orders = new Map<number, string[]>()
const hidden: string[] = []

for (const dir of readdirSync(EXT_DIR)) {
  const indexPath = join(EXT_DIR, dir, 'index.ts')
  if (!existsSync(indexPath)) continue
  const src = readFileSync(indexPath, 'utf8')
  const orderM = src.match(/order:\s*(\d+)/)
  if (!orderM) {
    console.error(`[check:extension-orders] ✗ ${dir}: 缺少 meta.order`)
    process.exit(1)
  }
  const order = Number(orderM[1])
  const isHidden = /hidden:\s*true/.test(src)
  if (isHidden) {
    hidden.push(`${dir}=${order}`)
    continue
  }
  const list = orders.get(order) ?? []
  list.push(dir)
  orders.set(order, list)
}

const dups = [...orders.entries()].filter(([, ids]) => ids.length > 1)
if (dups.length > 0) {
  console.error('[check:extension-orders] ✗ order 冲突：')
  for (const [o, ids] of dups) console.error(`  ${o}: ${ids.join(', ')}`)
  process.exit(1)
}

const sorted = [...orders.entries()].sort((a, b) => a[0] - b[0])
console.log(
  `[check:extension-orders] ✓ ${sorted.length} 可见扩展 order 唯一` +
    (hidden.length ? `（hidden: ${hidden.join(', ')}）` : ''),
)
