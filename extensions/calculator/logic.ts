interface Token {
  type: 'num' | 'op' | 'lp' | 'rp'
  value: string
}

const ALLOWED_CHARS = /^[0-9+\-*/().%\s=]*$/

export function evaluateMath(expr: string): string | null {
  try {
    const withExponent = expr.replace(/\^/g, '**')
    if (!ALLOWED_CHARS.test(withExponent)) return null
    // 去除末尾等号（计算器终止符，容忍受输入中态 "1+1="）
    const sanitized = withExponent.trim().replace(/=+$/, '')
    if (!sanitized) return null

    const tokens = tokenize(sanitized)
    if (!tokens) return null
    // 去除末尾悬空运算符（容忍输入中态 "1+1+"，不影响 "1+" 仍返回 null）
    while (tokens.length > 0 && tokens[tokens.length - 1].type === 'op') tokens.pop()
    if (tokens.length === 0) return null
    // 纯数字（含前导负号折叠成的单 num token）不算表达式——需至少一个运算符或括号
    if (!tokens.some((t) => t.type !== 'num')) return null
    const result = parseExpression(tokens)
    if (result === null) return null

    if (!isFinite(result)) return null
    if (Number.isInteger(result)) return result.toString()
    return parseFloat(result.toFixed(6)).toString()
  } catch {
    return null
  }
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
      while (i < expr.length && (('0' <= expr[i] && expr[i] <= '9') || expr[i] === '.'))
        num += expr[i++]
      if (isNaN(parseFloat(num))) return null
      tokens.push({ type: 'num', value: num })
    } else if (ch === '(') {
      tokens.push({ type: 'lp', value: '(' })
      i++
    } else if (ch === ')') {
      tokens.push({ type: 'rp', value: ')' })
      i++
    } else if (ch === '*' && i + 1 < expr.length && expr[i + 1] === '*') {
      tokens.push({ type: 'op', value: '**' })
      i += 2
    } else if ('+-*/%'.includes(ch)) {
      const isNegNum =
        ch === '-' &&
        (tokens.length === 0 ||
          tokens[tokens.length - 1].type === 'lp' ||
          tokens[tokens.length - 1].type === 'op') &&
        i + 1 < expr.length &&
        (('0' <= expr[i + 1] && expr[i + 1] <= '9') || expr[i + 1] === '.')
      if (isNegNum) {
        let num = '-'
        i++
        while (i < expr.length && (('0' <= expr[i] && expr[i] <= '9') || expr[i] === '.'))
          num += expr[i++]
        if (isNaN(parseFloat(num))) return null
        tokens.push({ type: 'num', value: num })
      } else {
        tokens.push({ type: 'op', value: ch })
        i++
      }
    } else return null
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
