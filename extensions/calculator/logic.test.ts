import { describe, it, expect } from 'vitest'
import { evaluateMath } from './logic'

describe('evaluateMath — basic arithmetic', () => {
  it('addition', () => {
    expect(evaluateMath('1+2')).toBe('3')
    expect(evaluateMath('10+20+30')).toBe('60')
  })

  it('subtraction', () => {
    expect(evaluateMath('10-3')).toBe('7')
    expect(evaluateMath('100-50-25')).toBe('25')
  })

  it('multiplication', () => {
    expect(evaluateMath('3*4')).toBe('12')
    expect(evaluateMath('2*3*4')).toBe('24')
  })

  it('division', () => {
    expect(evaluateMath('10/2')).toBe('5')
    expect(evaluateMath('100/4/5')).toBe('5')
  })

  it('modulo', () => {
    expect(evaluateMath('10%3')).toBe('1')
    expect(evaluateMath('20%7')).toBe('6')
  })
})

describe('evaluateMath — precedence', () => {
  it('mul/div before add/sub', () => {
    expect(evaluateMath('2+3*4')).toBe('14')
    expect(evaluateMath('10-6/2')).toBe('7')
    expect(evaluateMath('2*3+4*5')).toBe('26')
  })

  it('power before mul/div', () => {
    expect(evaluateMath('2*3**2')).toBe('18')
    expect(evaluateMath('2**3+1')).toBe('9')
  })

  it('right-associative power', () => {
    expect(evaluateMath('2**3**2')).toBe('512')
  })
})

describe('evaluateMath — parentheses', () => {
  it('grouping overrides precedence', () => {
    expect(evaluateMath('(2+3)*4')).toBe('20')
    expect(evaluateMath('(10-2)/2')).toBe('4')
  })

  it('nested parentheses', () => {
    expect(evaluateMath('((2+3))*((4-2))')).toBe('10')
    expect(evaluateMath('2*(3+(4*5))')).toBe('46')
  })
})

describe('evaluateMath — unary minus', () => {
  it('negative numbers', () => {
    expect(evaluateMath('-5')).toBeNull()
    expect(evaluateMath('-5+3')).toBe('-2')
    expect(evaluateMath('3+-5')).toBe('-2')
    expect(evaluateMath('3*-2')).toBe('-6')
  })

  it('unary in parentheses', () => {
    expect(evaluateMath('(-3)*2')).toBe('-6')
    expect(evaluateMath('-(2+3)')).toBe('-5')
  })
})

describe('evaluateMath — decimals', () => {
  it('decimal arithmetic', () => {
    expect(evaluateMath('0.5+0.5')).toBe('1')
    expect(evaluateMath('1.5*2')).toBe('3')
  })
})

describe('evaluateMath — pure number is not an expression', () => {
  it('整数 / 小数 / 负数裸值返回 null', () => {
    expect(evaluateMath('123')).toBeNull()
    expect(evaluateMath('3.14')).toBeNull()
    expect(evaluateMath('10.0')).toBeNull()
    expect(evaluateMath('5.000')).toBeNull()
    expect(evaluateMath('-5')).toBeNull()
  })

  it('含运算符或括号才求值', () => {
    expect(evaluateMath('1+2')).toBe('3')
    expect(evaluateMath('(5)')).toBe('5')
  })
})

describe('evaluateMath — power notation', () => {
  it('^ syntax', () => {
    expect(evaluateMath('2^3')).toBe('8')
    expect(evaluateMath('2^10')).toBe('1024')
  })
})

describe('evaluateMath — edge cases', () => {
  it('division by zero returns null', () => {
    expect(evaluateMath('1/0')).toBeNull()
    expect(evaluateMath('10/(5-5)')).toBeNull()
  })

  it('empty input returns null', () => {
    expect(evaluateMath('')).toBeNull()
    expect(evaluateMath('   ')).toBeNull()
  })

  it('invalid characters return null', () => {
    expect(evaluateMath('abc')).toBeNull()
    expect(evaluateMath('1+foo')).toBeNull()
    expect(evaluateMath('eval(1)')).toBeNull()
  })

  it('unbalanced parentheses return null', () => {
    expect(evaluateMath('(1+2')).toBeNull()
    expect(evaluateMath('1+2)')).toBeNull()
    expect(evaluateMath('((1+2)')).toBeNull()
  })

  it('lone operator returns null', () => {
    expect(evaluateMath('+')).toBeNull()
    expect(evaluateMath('*')).toBeNull()
    expect(evaluateMath('1+')).toBeNull()
  })

  it('whitespace ignored', () => {
    expect(evaluateMath('  1  +  2  ')).toBe('3')
    expect(evaluateMath('1 + 2 * 3')).toBe('7')
  })
})

describe('evaluateMath — complex expressions', () => {
  it('mixed operations', () => {
    expect(evaluateMath('2+3*4-6/2')).toBe('11')
    expect(evaluateMath('(2+3)*(4-1)**2')).toBe('45')
    expect(evaluateMath('100/4+3*2-1')).toBe('30')
  })
})
