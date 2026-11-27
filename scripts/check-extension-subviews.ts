#!/usr/bin/env bun
// check:extension-subviews —— 扩展子视图显示名完整性（阻塞项）。
// 最近使用行按屏平铺，标题由扩展名 + subviewTitle 合成；subviewTitle 缺失 / 重复会让
// 同一扩展的多级界面重新变成同名项。校验三条不变量：
//   1. subviews 的每个 key 都有对应 subviewTitle（含有效显示名）；
//   2. subviewTitle 不得悬空（key 没有对应 subviews）；
//   3. 同一扩展内不同子视图的显示名不得重复（按语言解析，与 runtime/i18n 同回退链）。
// 用 TypeScript AST 读 defineExtension({...}) 的对象字面量，不依赖缩进/格式。
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import ts from 'typescript'

const EXT_DIR = join(process.cwd(), 'extensions')
const LOCALES = ['zh-CN', 'en'] as const

function propKey(name: ts.PropertyName): string | null {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) {
    return name.text
  }
  return null
}

function findProperty(obj: ts.ObjectLiteralExpression, key: string): ts.PropertyAssignment | null {
  for (const p of obj.properties) {
    if (ts.isPropertyAssignment(p) && propKey(p.name) === key) return p
  }
  return null
}

/** subviews / subviewTitle 要求是静态对象字面量；动态构造无法做注册期校验。 */
function objectValue(prop: ts.PropertyAssignment): ts.ObjectLiteralExpression | null {
  return ts.isObjectLiteralExpression(prop.initializer) ? prop.initializer : null
}

function keysOf(obj: ts.ObjectLiteralExpression): string[] {
  const keys: string[] = []
  for (const p of obj.properties) {
    if (
      !ts.isPropertyAssignment(p) &&
      !ts.isMethodDeclaration(p) &&
      !ts.isShorthandPropertyAssignment(p)
    ) {
      continue
    }
    const key = propKey(p.name)
    if (key) keys.push(key)
  }
  return keys
}

/** 解析 LocalizedText 的展示值：字符串直用；对象按 locale → zh-CN → en → 首个值回退
 *  （与 runtime/i18n.ts::resolveLocalized 同语义）。 */
function resolveTitle(prop: ts.PropertyAssignment, locale: string): string {
  const init = prop.initializer
  if (ts.isStringLiteral(init)) return init.text
  if (!ts.isObjectLiteralExpression(init)) return ''
  const map = new Map<string, string>()
  for (const p of init.properties) {
    if (!ts.isPropertyAssignment(p)) continue
    const key = propKey(p.name)
    if (key && ts.isStringLiteral(p.initializer)) map.set(key, p.initializer.text)
  }
  return map.get(locale) ?? map.get('zh-CN') ?? map.get('en') ?? [...map.values()][0] ?? ''
}

/** 找 defineExtension({...}) 的首个实参对象。用数组累积规避闭包赋值导致的类型收窄。 */
function findExtensionObject(sf: ts.SourceFile): ts.ObjectLiteralExpression | null {
  const found: ts.ObjectLiteralExpression[] = []
  const visit = (node: ts.Node): void => {
    if (found.length) return
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === 'defineExtension'
    ) {
      const arg = node.arguments[0]
      if (arg && ts.isObjectLiteralExpression(arg)) {
        found.push(arg)
        return
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(sf)
  return found[0] ?? null
}

const errors: string[] = []
let checkedExts = 0
let checkedSubviews = 0

for (const dir of readdirSync(EXT_DIR).sort()) {
  const indexPath = join(EXT_DIR, dir, 'index.ts')
  if (!existsSync(indexPath)) continue

  const src = readFileSync(indexPath, 'utf8')
  const sf = ts.createSourceFile(indexPath, src, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
  const extObj = findExtensionObject(sf)
  if (!extObj) {
    errors.push(`${dir}: 未找到 defineExtension({...})`)
    continue
  }

  const subviewProp = findProperty(extObj, 'subviews')
  const titleProp = findProperty(extObj, 'subviewTitle')
  const subviews = subviewProp ? objectValue(subviewProp) : null
  const titles = titleProp ? objectValue(titleProp) : null
  if (subviewProp && !subviews) {
    errors.push(`${dir}: subviews 必须是静态对象字面量（校验依赖静态键）`)
    continue
  }
  if (titleProp && !titles) {
    errors.push(`${dir}: subviewTitle 必须是静态对象字面量（校验依赖静态键）`)
    continue
  }

  const subviewKeys = subviews ? keysOf(subviews) : []
  const titleKeys = titles ? keysOf(titles) : []
  if (subviewKeys.length === 0) {
    if (titleKeys.length > 0) errors.push(`${dir}: subviewTitle 悬空（未声明 subviews）`)
    continue
  }

  checkedExts++
  checkedSubviews += subviewKeys.length

  for (const key of subviewKeys) {
    if (!titleKeys.includes(key)) errors.push(`${dir}: 子视图 ${key} 缺少 subviewTitle`)
  }
  for (const key of titleKeys) {
    if (!subviewKeys.includes(key)) errors.push(`${dir}: subviewTitle.${key} 悬空（无对应子视图）`)
  }

  // 同一扩展内的子视图显示名必须唯一：最近使用行只显示「扩展名 · 子视图名」，
  // 重复标题会让不同屏再次无法区分。按语言分别校验（含 LocalizedText 回退）。
  for (const locale of LOCALES) {
    const seen = new Map<string, string>()
    for (const key of subviewKeys) {
      const prop = titles ? findProperty(titles, key) : null
      if (!prop) continue
      const name = resolveTitle(prop, locale)
      if (!name) {
        errors.push(`${dir}: 子视图 ${key} 的 subviewTitle 在 ${locale} 下为空`)
        continue
      }
      const prev = seen.get(name)
      if (prev) {
        errors.push(`${dir}: subviewTitle 重复（${locale}）：「${name}」同时用于 ${prev} / ${key}`)
      } else {
        seen.set(name, key)
      }
    }
  }
}

if (errors.length > 0) {
  console.error('[check:extension-subviews] ✗ 子视图元数据校验失败：')
  for (const e of errors) console.error(`  ${e}`)
  process.exit(1)
}

console.log(
  `[check:extension-subviews] ✓ ${checkedExts} 个扩展 / ${checkedSubviews} 个子视图标题完整且唯一`,
)
