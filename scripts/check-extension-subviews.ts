#!/usr/bin/env bun
// check:extension-subviews —— 子视图显示名完整性（阻塞项）。
// subviews 每个屏声明为 { view, title, height? }；title 是搜索框 placeholder 与
// 最近使用行标题的单一来源。TS 已保证 title 必填，本校验补类型表达不了的部分：
//   1. subviews / 屏定义必须是静态对象字面量（AST 可读；动态构造拒绝）；
//   2. 每个屏的 title 在 zh-CN / en 下解析非空（含 LocalizedText 回退）；
//   3. 同一扩展内不同屏的 title 唯一（最近行只显示「扩展名 · title」，重复会再次同名）。
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

/** 任意同名声明的存在性（简写属性 / getter / method 都算）。用于区分「缺失」与
 *  「存在但非静态字面量」，避免脚本静默跳过。 */
function hasProperty(obj: ts.ObjectLiteralExpression, key: string): boolean {
  for (const p of obj.properties) {
    if (!ts.isSpreadAssignment(p) && propKey(p.name) === key) return true
  }
  return false
}

/** 去掉不影响静态值的包裹层（as const / satisfies / 括号），保留可读的字面量。 */
function unwrapExpression(expr: ts.Expression): ts.Expression {
  let current = expr
  while (
    ts.isAsExpression(current) ||
    ts.isSatisfiesExpression(current) ||
    ts.isParenthesizedExpression(current)
  ) {
    current = current.expression
  }
  return current
}

/** subviews / 屏定义要求是静态对象字面量；动态构造无法做注册期校验。 */
function objectValue(prop: ts.PropertyAssignment): ts.ObjectLiteralExpression | null {
  const init = unwrapExpression(prop.initializer)
  return ts.isObjectLiteralExpression(init) ? init : null
}

/** 解析 LocalizedText 的展示值：字符串直用；对象按 locale → zh-CN → en → 首个值回退
 *  （与 runtime/i18n.ts::resolveLocalized 同语义）。 */
function resolveTitle(prop: ts.PropertyAssignment, locale: string): string {
  const init = unwrapExpression(prop.initializer)
  if (ts.isStringLiteral(init)) return init.text
  if (!ts.isObjectLiteralExpression(init)) return ''
  const map = new Map<string, string>()
  for (const p of init.properties) {
    if (!ts.isPropertyAssignment(p)) continue
    const key = propKey(p.name)
    const value = unwrapExpression(p.initializer)
    if (key && ts.isStringLiteral(value)) map.set(key, value.text)
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
  // 顶层 spread 可能注入任意 subviews（对象展开顺序还可能覆盖直接声明），
  // AST 无法静态确认——要求扩展元数据显式声明，避免校验静默漏检。
  if (extObj.properties.some(ts.isSpreadAssignment)) {
    errors.push(`${dir}: defineExtension 不接受顶层展开（子视图元数据须显式静态声明）`)
    continue
  }

  const subviewProp = findProperty(extObj, 'subviews')
  if (!subviewProp) {
    if (hasProperty(extObj, 'subviews')) {
      errors.push(`${dir}: subviews 必须是静态对象字面量属性（不支持简写/getter/spread）`)
    }
    continue
  }
  const subviews = objectValue(subviewProp)
  if (!subviews) {
    errors.push(`${dir}: subviews 必须是静态对象字面量（校验依赖静态键）`)
    continue
  }

  checkedExts++
  const seenTitles = new Map<string, Map<string, string>>()
  for (const locale of LOCALES) seenTitles.set(locale, new Map())

  for (const p of subviews.properties) {
    if (!ts.isPropertyAssignment(p)) {
      errors.push(`${dir}: subviews 必须使用静态属性声明（不支持展开/方法）`)
      continue
    }
    const key = propKey(p.name)
    if (!key) {
      errors.push(`${dir}: subviews 存在无法解析的 key`)
      continue
    }
    checkedSubviews++

    const def = objectValue(p)
    if (!def) {
      errors.push(`${dir}.${key}: 屏定义必须是 { view, title, height? } 对象字面量`)
      continue
    }
    if (!hasProperty(def, 'view')) errors.push(`${dir}.${key}: 缺少 view`)
    const titleProp = findProperty(def, 'title')
    if (!titleProp) {
      errors.push(
        hasProperty(def, 'title')
          ? `${dir}.${key}: title 必须是静态字符串或本地化对象字面量`
          : `${dir}.${key}: 缺少 title`,
      )
      continue
    }
    const titleInit = unwrapExpression(titleProp.initializer)
    if (!ts.isStringLiteral(titleInit) && !ts.isObjectLiteralExpression(titleInit)) {
      errors.push(`${dir}.${key}: title 必须是静态字符串或本地化对象字面量`)
      continue
    }

    for (const locale of LOCALES) {
      const name = resolveTitle(titleProp, locale)
      if (!name) {
        errors.push(`${dir}.${key}: title 在 ${locale} 下为空`)
        continue
      }
      const seen = seenTitles.get(locale)!
      const prev = seen.get(name)
      if (prev) {
        errors.push(`${dir}.${key}: title 与 ${prev} 重复（${locale}）：「${name}」`)
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
