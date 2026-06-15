import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { load } from '@tauri-apps/plugin-store'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { hideWindow } from '@/utils/tauri'

let historyCache: { expr: string; result: string }[] = []
let historyLoaded = false

const loadHistory = async () => {
  if (historyLoaded) return
  try {
    const store = await load('extensions/calculator/calc_history.json')
    const saved = await store.get<{ expr: string; result: string }[]>('history')
    if (saved && Array.isArray(saved)) {
      historyCache = saved
    }
    historyLoaded = true
  } catch (e) {
    console.error('Failed to load calc history:', e)
  }
}

const saveHistory = async (expr: string, result: string) => {
  try {
    if (historyCache.length > 0 && historyCache[0].expr === expr) {
      return
    }
    historyCache.unshift({ expr, result })
    if (historyCache.length > 10) {
      historyCache = historyCache.slice(0, 10)
    }
    const store = await load('extensions/calculator/calc_history.json')
    await store.set('history', historyCache)
    await store.save()
  } catch (e) {
    console.error('Failed to save calc history:', e)
  }
}

const ALLOWED_CHARS = /^[0-9+\-*/().%\s]*$/

const evaluateMath = (expr: string): string | null => {
  try {
    const withExponent = expr.replace(/\^/g, '**')
    if (!ALLOWED_CHARS.test(withExponent)) return null
    const sanitized = withExponent.trim()
    if (!sanitized) return null

    const tokens = tokenize(sanitized)
    if (!tokens) return null
    const result = parseExpression(tokens)
    if (result === null) return null

    if (!isFinite(result)) return null
    if (Number.isInteger(result)) return result.toString()
    return parseFloat(result.toFixed(6)).toString()
  } catch {
    return null
  }
}

interface Token {
  type: 'num' | 'op' | 'lp' | 'rp'
  value: string
}

function tokenize(expr: string): Token[] | null {
  const tokens: Token[] = []
  let i = 0
  while (i < expr.length) {
    const ch = expr[i]
    if (ch === ' ' || ch === '\t') {
      i++
      continue
    }
    if (('0' <= ch && ch <= '9') || ch === '.') {
      let num = ''
      while (i < expr.length && (('0' <= expr[i] && expr[i] <= '9') || expr[i] === '.')) {
        num += expr[i++]
      }
      const val = parseFloat(num)
      if (isNaN(val)) return null
      tokens.push({ type: 'num', value: num })
    } else if (ch === '(') {
      tokens.push({ type: 'lp', value: '(' })
      i++
    } else if (ch === ')') {
      tokens.push({ type: 'rp', value: ')' })
      i++
    } else if ('+-*/%'.includes(ch)) {
      if (
        ch === '-' &&
        (tokens.length === 0 ||
          tokens[tokens.length - 1].type === 'lp' ||
          tokens[tokens.length - 1].type === 'op')
      ) {
        let num = '-'
        i++
        while (i < expr.length && (('0' <= expr[i] && expr[i] <= '9') || expr[i] === '.')) {
          num += expr[i++]
        }
        const val = parseFloat(num)
        if (isNaN(val)) return null
        tokens.push({ type: 'num', value: num })
      } else {
        tokens.push({ type: 'op', value: ch })
        i++
      }
    } else if (ch === '*' && i + 1 < expr.length && expr[i + 1] === '*') {
      tokens.push({ type: 'op', value: '**' })
      i += 2
    } else {
      return null
    }
  }
  return tokens
}

function parseExpression(tokens: Token[]): number | null {
  let pos = 0

  function parseAddSub(): number | null {
    let left = parseMulDiv()
    if (left === null) return null
    while (
      pos < tokens.length &&
      tokens[pos].type === 'op' &&
      (tokens[pos].value === '+' || tokens[pos].value === '-')
    ) {
      const op = tokens[pos++].value
      const right = parseMulDiv()
      if (right === null) return null
      left = op === '+' ? left + right : left - right
    }
    return left
  }

  function parseMulDiv(): number | null {
    let left = parsePower()
    if (left === null) return null
    while (pos < tokens.length && tokens[pos].type === 'op' && '*/%'.includes(tokens[pos].value)) {
      const op = tokens[pos++].value
      const right = parsePower()
      if (right === null) return null
      if (op === '*') left *= right
      else if (op === '/') {
        if (right === 0) return null
        left /= right
      } else left %= right
    }
    return left
  }

  function parsePower(): number | null {
    const base = parseUnary()
    if (base === null) return null
    if (pos < tokens.length && tokens[pos].type === 'op' && tokens[pos].value === '**') {
      pos++
      const exp = parsePower()
      if (exp === null) return null
      return Math.pow(base, exp)
    }
    return base
  }

  function parseUnary(): number | null {
    if (pos < tokens.length && tokens[pos].type === 'op' && tokens[pos].value === '-') {
      pos++
      const val = parseAtom()
      if (val === null) return null
      return -val
    }
    return parseAtom()
  }

  function parseAtom(): number | null {
    if (pos >= tokens.length) return null
    const tok = tokens[pos]
    if (tok.type === 'num') {
      pos++
      return parseFloat(tok.value)
    }
    if (tok.type === 'lp') {
      pos++
      const val = parseAddSub()
      if (val === null) return null
      if (pos >= tokens.length || tokens[pos].type !== 'rp') return null
      pos++
      return val
    }
    return null
  }

  const result = parseAddSub()
  if (result === null || pos !== tokens.length) return null
  return result
}

const mod: AppModule = {
  id: 'calculator',
  name: '计算器',
  description: '数学表达式计算',
  icon: 'i-ri-calculator-line',
  keywords: ['calc', 'calculator', 'math', '计算器', '数学'],
  placeholder: '输入数学表达式',
  order: 2,
  enterHint: '复制',
  onInit: async () => {
    await loadHistory()
  },
  onSearch: async (query) => {
    if (!query.trim()) return []

    const withExponent = query.replace(/\^/g, '**')
    if (withExponent.trim() && ALLOWED_CHARS.test(withExponent) && /[+\-*/]/.test(withExponent)) {
      try {
        const result = evaluateMath(query)
        if (result !== null) {
          return [
            {
              id: 'calc-quick',
              title: `= ${result}`,
              description: `计算: ${query}`,
              module: 'calculator',
              icon: 'i-ri-calculator-line',
              score: 2000,
              data: { kind: 'module', expr: query, value: result },
            },
          ]
        }
      } catch {}
    }
    return []
  },
  onModuleSearch: async (query) => {
    await loadHistory()
    const results: SearchResult[] = []
    const trimmed = query.trim()

    if (trimmed) {
      const res = evaluateMath(trimmed)
      if (res !== null) {
        results.push({
          id: 'current',
          title: `= ${res}`,
          description: trimmed,
          module: 'calculator',
          icon: 'i-ri-calculator-line',
          data: {
            isHighlight: true,
            isHistory: false,
            expr: trimmed,
            value: res,
          },
        })
      }
    }

    historyCache.forEach((h, idx) => {
      results.push({
        id: `history-${idx}`,
        title: `= ${h.result}`,
        description: h.expr,
        module: 'calculator',
        icon: 'i-ri-history-line',
        data: { isHistory: true, expr: h.expr, value: h.result },
      })
    })

    return results
  },
  onExecute: async (result) => {
    try {
      if (result.data && !result.data.isHistory && result.data.expr && result.data.value) {
        await saveHistory(result.data.expr as string, result.data.value as string)
      }
      const value = result.data?.value ? String(result.data.value) : result.title.replace('= ', '')
      await writeText(value)
      hideWindow()
    } catch (e) {
      console.error('Failed to execute calc item:', e)
    }
  },
}

registerModule(mod)
