/**
 * 测试价值报告 Reporter
 *
 * 挂在 vitest.config.ts 的 reporters 上，跑完测试自动写 .test-report.md 到项目根。
 * 不刷终端（default reporter 照常输出），文件想看就看。
 *
 * 价值分层基于用例标题（it/describe 的 fullName）关键词启发式判定，非精确 AST：
 *   回归（修正/旧逻辑/regression）> 并发（abort/快照/signal/超时）> 边界（空/null/非法/不匹配）> 正向
 */
import type { Reporter } from 'vitest/reporters'
import { writeFileSync } from 'node:fs'
import { relative, resolve } from 'node:path'

const CWD = process.cwd()
const REPORT_PATH = resolve(CWD, '.test-report.md')

// 价值关键词（优先级从高到低，逐级短路）
const REGRESSION = /修正|旧逻辑|regression|@regression|\bbug\b/i
const CONCURRENCY = /abort|快照|signal|超时|timeout|竞态|isolation|snapshot/i
const EDGE =
  /空|null|undefined|零|除零|非法|不匹配|invalid|unbalanced|overflow|missing|衰减|优先级|嵌套|negative|decimal|边界|zero|parenthes|edge|whitespace|empty|lone/i

type Category = 'regression' | 'concurrency' | 'edge' | 'normal'

interface TestInfo {
  name: string
  state: string
  duration: number
  errors: string[]
  category: Category
}

interface FileStat {
  relPath: string
  tests: TestInfo[]
  duration: number
}

function classify(name: string): Category {
  if (REGRESSION.test(name)) return 'regression'
  if (CONCURRENCY.test(name)) return 'concurrency'
  if (EDGE.test(name)) return 'edge'
  return 'normal'
}

function shorten(absPath: string): string {
  return relative(CWD, absPath).replace(/\.test\.ts$/, '')
}

function pct(n: number, total: number): string {
  return total ? `${((n / total) * 100).toFixed(1)}%` : '0%'
}

function bar10(p: number): string {
  const filled = Math.round(p * 10)
  return '█'.repeat(filled) + '░'.repeat(10 - filled)
}

function truncate(s: string, n: number): string {
  const one = s.replace(/\s+/g, ' ').trim()
  return one.length > n ? one.slice(0, n) + '…' : one
}

class TestReportReporter implements Reporter {
  async onTestRunEnd(testModules, unhandledErrors) {
    const files: FileStat[] = []
    let total = 0
    let failed = 0
    let skipped = 0

    for (const mod of testModules) {
      const tests: TestInfo[] = []
      let fileDuration = 0
      for (const test of mod.children.allTests()) {
        const result = test.result()
        const name = test.fullName
        const state = result.state
        const duration = test.diagnostic()?.duration ?? 0
        const errors =
          state === 'failed'
            ? (result.errors ?? []).map((e) => (e as { message?: string }).message ?? String(e))
            : []
        tests.push({ name, state, duration, errors, category: classify(name) })
        total++
        if (state === 'failed') failed++
        else if (state === 'skipped') skipped++
        fileDuration += duration
      }
      files.push({ relPath: shorten(mod.moduleId), tests, duration: fileDuration })
    }

    const allTests = files.flatMap((f) => f.tests)
    const totalDuration = files.reduce((s, f) => s + f.duration, 0)
    const catCount = (c: Category) => allTests.filter((t) => t.category === c).length
    const regressions = allTests.filter((t) => t.category === 'regression')
    const failures = allTests.filter((t) => t.state === 'failed')

    // 文件级非正向密度（降序）
    const density = files
      .map((f) => {
        const non = f.tests.filter((t) => t.category !== 'normal').length
        return { f, non, pctVal: f.tests.length ? non / f.tests.length : 0 }
      })
      .filter((x) => x.non > 0)
      .sort((a, b) => b.pctVal - a.pctVal || b.non - a.non)

    const fileOf = (t: TestInfo) => files.find((f) => f.tests.includes(t))!.relPath

    const L: string[] = []
    L.push('# 测试报告', '')
    const status = failed === 0 ? '全绿' : `${failed} 失败`
    L.push(
      `总计 ${total} 用例 · ${status} · ${(totalDuration / 1000).toFixed(2)}s　生成于 ${new Date().toLocaleString('zh-CN')}`,
      '',
    )
    if (skipped) L.push(`（跳过 ${skipped}）`, '')

    // 价值分层
    L.push('## 价值分层', '')
    L.push(
      `- 回归测试 ${catCount('regression')} (${pct(catCount('regression'), total)})　记录过真实 bug`,
    )
    L.push(
      `- 并发时序 ${catCount('concurrency')} (${pct(catCount('concurrency'), total)})　abort/快照/超时/竞态`,
    )
    L.push(
      `- 边界防御 ${catCount('edge')} (${pct(catCount('edge'), total)})　空值/非法/不匹配/溢出`,
    )
    L.push(`- 正向逻辑 ${catCount('normal')} (${pct(catCount('normal'), total)})`, '')

    // 价值密度 Top
    if (density.length) {
      L.push('## 价值密度（非正向占比）', '')
      for (const { f, pctVal } of density) {
        const parts: string[] = []
        const c = (cat: Category) => f.tests.filter((t) => t.category === cat).length
        if (c('regression')) parts.push(`回归 ${c('regression')}`)
        if (c('concurrency')) parts.push(`并发 ${c('concurrency')}`)
        if (c('edge')) parts.push(`边界 ${c('edge')}`)
        L.push(
          `- \`${f.relPath}\` ${f.tests.length} 用例 ${bar10(pctVal)} ${Math.round(pctVal * 100)}%${parts.length ? ' · ' + parts.join(' · ') : ''}`,
        )
      }
      L.push('')
    }

    // 回归明细
    if (regressions.length) {
      L.push('## 回归测试（记录过真实 bug）', '')
      for (const r of regressions) L.push(`- \`${fileOf(r)}\` · ${r.name}`)
      L.push('')
    }

    // 失败明细
    if (failures.length) {
      L.push('## 失败用例', '')
      for (const ftest of failures) {
        L.push(`- \`${fileOf(ftest)}\` · ${ftest.name}`)
        for (const e of ftest.errors) L.push(`  > ${truncate(e, 200)}`)
      }
      L.push('')
    }

    // 收集期错误
    if (unhandledErrors.length) {
      L.push('## 收集期错误', '')
      for (const e of unhandledErrors)
        L.push(`- ${truncate((e as { message?: string }).message ?? String(e), 200)}`)
      L.push('')
    }

    L.push('## Rust', '')
    L.push('Rust 单测请 `cd src-tauri && cargo test --lib`', '')

    writeFileSync(REPORT_PATH, L.join('\n') + '\n', 'utf-8')
  }
}

export default TestReportReporter
