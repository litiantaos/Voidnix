/// 应用自测模块：在真实 app 内部运行，直接调用 searchEngine / getAllExtensions / invoke
/// 等真实 API 验证功能正确性。结果经 plugin-store 写到 config/test-report.json 供外部编排器读取。
///
/// 触发：main.ts 在扩展 setup 完成后检查 is_self_test_mode 命令（环境变量驱动），true 则动态 import 本模块。
/// 覆盖：扩展注册完整性 / 搜索引擎正确性 / 扩展视图渲染冒烟 / Tauri 命令可达性。

import { invoke } from '@tauri-apps/api/core'
import { load } from '@tauri-apps/plugin-store'
import { nextTick } from 'vue'
import { CMD } from '@/commands'
import { getAllExtensions } from './extension-registry'
import { searchEngine } from './search-engine'
import { isTauri } from '@/utils/tauri'
import type { SearchResult } from './types'

// ── 类型 ─────────────────────────────────────────────────────────────────────

type TestStatus = 'pass' | 'fail' | 'skip'

interface TestResult {
  category: string
  name: string
  status: TestStatus
  message?: string
  duration_ms: number
}

interface SelfTestReport {
  timestamp: string
  duration_ms: number
  summary: { total: number; passed: number; failed: number; skipped: number }
  results: TestResult[]
}

// ── 辅助 ─────────────────────────────────────────────────────────────────────

/** 单次测试执行器：捕获异常转为 pass/fail，记录耗时。 */
async function runTest(
  category: string,
  name: string,
  fn: () => Promise<void>,
): Promise<TestResult> {
  const start = performance.now()
  try {
    await fn()
    return { category, name, status: 'pass', duration_ms: Math.round(performance.now() - start) }
  } catch (e) {
    const message = e instanceof Error ? `${e.name}: ${e.message}` : String(e)
    return {
      category,
      name,
      status: 'fail',
      message,
      duration_ms: Math.round(performance.now() - start),
    }
  }
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message)
}

/** console.error 收集器：测试期间拦截 console.error，捕获 Vue 渲染异常 / 扩展运行时错误。 */
function createErrorCollector() {
  const errors: string[] = []
  const original = console.error
  return {
    start() {
      console.error = (...args: unknown[]) => {
        errors.push(args.map((a) => (a instanceof Error ? a.message : String(a))).join(' '))
        original.apply(console, args as never[])
      }
    },
    stop() {
      console.error = original
    },
    drain(): string[] {
      const out = [...errors]
      errors.length = 0
      return out
    },
  }
}

/** 从结果列表中查找标题包含指定文本的项。 */
function findResultByTitle(results: SearchResult[], text: string): SearchResult | undefined {
  return results.find((r) => r.title.includes(text))
}

/** 等待指定毫秒。 */
function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms))
}

/** 诊断输出到 Rust stderr（WKWebView console 不可见）。仅自测模式输出。 */
async function diag(msg: string): Promise<void> {
  await invoke(CMD.selfTestDiag, { message: msg }).catch(() => {})
}

// ── A. 扩展注册完整性 ────────────────────────────────────────────────────────

async function testExtensionRegistration(): Promise<TestResult[]> {
  const results: TestResult[] = []
  const exts = getAllExtensions()

  results.push(
    await runTest('registration', '全部扩展已加载', async () => {
      assert(exts.length >= 20, `期望 ≥20 扩展，实际 ${exts.length}`)
    }),
  )

  results.push(
    await runTest('registration', 'meta.id 无重复', async () => {
      const ids = exts.map((e) => e.meta.id)
      const dupes = ids.filter((id, i) => ids.indexOf(id) !== i)
      assert(dupes.length === 0, `重复 id: ${dupes.join(', ')}`)
    }),
  )

  results.push(
    await runTest('registration', 'meta 字段完整', async () => {
      for (const ext of exts) {
        assert(!!ext.meta.id, `${ext.meta.id ?? '?'}: id 为空`)
        assert(!!ext.meta.name, `${ext.meta.id}: name 为空`)
        assert(!!ext.meta.icon, `${ext.meta.id}: icon 为空`)
        assert(typeof ext.meta.order === 'number', `${ext.meta.id}: order 非数字`)
      }
    }),
  )

  results.push(
    await runTest('registration', '非 hidden 扩展 order 唯一', async () => {
      const visible = exts.filter((e) => !e.meta.hidden)
      const orders = visible.map((e) => e.meta.order)
      const dupes = orders.filter((o, i) => orders.indexOf(o) !== i)
      assert(dupes.length === 0, `重复 order: ${dupes.join(', ')}`)
    }),
  )

  return results
}

// ── B. 搜索引擎正确性 ────────────────────────────────────────────────────────

async function testSearchCorrectness(): Promise<TestResult[]> {
  const results: TestResult[] = []

  // calculator
  results.push(
    await runTest('search', 'calculator 1+2=3', async () => {
      const r = await searchEngine.search('1+2')
      const calc = findResultByTitle(r, '3')
      assert(!!calc, `期望含 "3" 的计算器结果，实际: ${r.map((x) => x.title).join(', ')}`)
      assert(calc!.data?.kind === 'extension', '结果 kind 非 extension')
    }),
  )

  results.push(
    await runTest('search', 'calculator 优先级 1+2*3=7', async () => {
      const r = await searchEngine.search('1+2*3')
      const calc = findResultByTitle(r, '7')
      assert(!!calc, `期望含 "7" 的计算器结果，实际: ${r.map((x) => x.title).join(', ')}`)
    }),
  )

  results.push(
    await runTest('search', 'calculator 取模 10%3=1', async () => {
      const r = await searchEngine.search('10%3')
      const calc = findResultByTitle(r, '1')
      assert(!!calc, `期望含 "1" 的计算器结果，实际: ${r.map((x) => x.title).join(', ')}`)
    }),
  )

  // base64 解码（boost=1000 穿透）
  results.push(
    await runTest('search', 'base64 解码 SGVsbG8= → Hello', async () => {
      const r = await searchEngine.search('SGVsbG8=')
      const decoded = findResultByTitle(r, 'Hello')
      assert(
        !!decoded,
        `期望含 "Hello" 的 base64 解码结果，实际: ${r.map((x) => x.title).join(', ')}`,
      )
      assert(decoded!.data?.kind === 'extension', '结果 kind 非 extension')
    }),
  )

  // keyword 扩展入口
  results.push(
    await runTest('search', 'keyword calc → 计算器入口', async () => {
      const r = await searchEngine.search('calc')
      const entry = r.find((x) => x.data?.kind === 'extension' && x.extId === 'calculator')
      assert(
        !!entry,
        `期望含 calculator 扩展入口，实际: ${r.map((x) => `${x.extId}:${x.title}`).join(', ')}`,
      )
    }),
  )

  // 空查询不 crash
  results.push(
    await runTest('search', '空查询不 crash', async () => {
      const r = await searchEngine.search('')
      assert(Array.isArray(r), '空查询应返回数组')
    }),
  )

  // 无结果查询不 crash
  results.push(
    await runTest('search', '无结果查询不 crash', async () => {
      const r = await searchEngine.search('zzznotexist12345xyz')
      assert(Array.isArray(r), '无结果查询应返回数组')
    }),
  )

  return results
}

// ── C. 扩展视图渲染冒烟 ──────────────────────────────────────────────────────

/** 有 mainView 的扩展列表（动态发现，不硬编码）。 */
function getMainViewExtensions(): string[] {
  return getAllExtensions()
    .filter((e) => e.mainView)
    .map((e) => e.meta.id)
}

async function testExtensionViews(): Promise<TestResult[]> {
  const results: TestResult[] = []
  const extIds = getMainViewExtensions()
  const collector = createErrorCollector()

  // 动态导入 useAppStore（避免静态依赖 stores/——自测模块应最小化耦合）
  const { useAppStore } = await import('@/stores/app')
  const store = useAppStore()

  for (const extId of extIds) {
    const ext = getAllExtensions().find((e) => e.meta.id === extId)
    const label = ext ? ext.meta.name : extId

    await diag(`  C: 进入 ${extId}...`)
    results.push(
      await runTest('extension-view', `${extId} (${label}) 视图渲染`, async () => {
        collector.start()
        try {
          store.setActiveExtension(extId)
          // 等 Vue 批量更新 + 扩展 onActivated 异步操作（网络请求等）
          await nextTick()
          await sleep(300)

          const errors = collector.drain()
          // 过滤网络相关错误（proxy/translate/agent 等扩展在无网/无凭证时正常报错）
          const critical = errors.filter(
            (e) =>
              e.includes('TypeError') ||
              e.includes('ReferenceError') ||
              e.includes('SyntaxError') ||
              e.includes('is not a function') ||
              e.includes('is undefined') ||
              e.includes('Cannot read propert'),
          )
          if (critical.length > 0) {
            throw new Error(critical.join('; '))
          }
        } finally {
          collector.stop()
          store.setActiveExtension(null)
          await nextTick()
          await sleep(100)
        }
      }),
    )
  }

  return results
}

// ── D. Tauri 命令可达性 ──────────────────────────────────────────────────────

/** 安全探测：仅调用无副作用的查询命令，不触发任何修改操作。 */
async function testCommandAvailability(): Promise<TestResult[]> {
  const results: TestResult[] = []
  const timeout = (ms: number) =>
    new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), ms))

  interface Probe {
    name: string
    cmd: string
    args?: Record<string, unknown>
    validate?: (result: unknown) => void
  }

  const probes: Probe[] = [
    {
      name: 'is_app_active',
      cmd: CMD.isAppActive,
      validate: (r) => assert(typeof r === 'boolean', '应返回 bool'),
    },
    {
      name: 'get_home_dir',
      cmd: CMD.getHomeDir,
      validate: (r) => assert(typeof r === 'string' && r.length > 0, '应返回路径'),
    },
    {
      name: 'get_cached_appearance',
      cmd: CMD.getCachedAppearance,
      validate: (r) =>
        assert(
          ['light', 'dark', 'auto'].includes(r as string),
          `应返回 light/dark/auto，实际 ${r}`,
        ),
    },
    {
      name: 'is_autostart_enabled',
      cmd: CMD.isAutostartEnabled,
      validate: (r) => assert(typeof r === 'boolean', '应返回 bool'),
    },
    {
      name: 'system_static_info',
      cmd: CMD.systemStaticInfo,
      validate: (r) => assert(typeof r === 'object' && r !== null, '应返回对象'),
    },
  ]

  for (const probe of probes) {
    results.push(
      await runTest('command', `${probe.name} 可达`, async () => {
        const result = await Promise.race([invoke(probe.cmd, probe.args ?? {}), timeout(5000)])
        probe.validate?.(result)
      }),
    )
  }

  // search_apps 带参数探测（可能超时——Spotlight 索引首次查询慢）
  results.push(
    await runTest('command', 'search_apps(safari) 返回结果', async () => {
      const result = await Promise.race([
        invoke(CMD.searchApps, { query: 'safari' }),
        timeout(8000),
      ])
      const apps = result as unknown[]
      assert(Array.isArray(apps), '应返回数组')
      // Spotlight 索引可能未就绪，不强制要求非空
    }),
  )

  return results
}

// ── 报告写入 ─────────────────────────────────────────────────────────────────

async function writeReport(report: SelfTestReport): Promise<void> {
  try {
    const store = await load('config/test-report.json')
    await store.set('report', report)
    await store.save()
    await diag('报告写入成功')
  } catch (e) {
    // plugin-store 写入失败时不阻塞——stderr 输出供 Python 兜底捕获
    await diag(`报告写入失败: ${e}`)
  }
}

// ── 主入口 ───────────────────────────────────────────────────────────────────

export async function runSelfTest(): Promise<void> {
  if (!isTauri) return

  const start = performance.now()
  console.warn('[self-test] 开始自测...')

  const allResults: TestResult[] = []

  // A. 扩展注册
  await diag('A. 扩展注册完整性...')
  allResults.push(...(await testExtensionRegistration()))
  await diag(`A 完成: ${allResults.length} 用例`)

  // B. 搜索引擎
  await diag('B. 搜索引擎正确性...')
  searchEngine.setActiveExtension(undefined)
  allResults.push(...(await testSearchCorrectness()))
  await diag(`B 完成: ${allResults.length} 用例`)

  // C. 扩展视图（最慢，~10s）
  await diag('C. 扩展视图渲染冒烟...')
  allResults.push(...(await testExtensionViews()))
  await diag(`C 完成: ${allResults.length} 用例`)

  // D. 命令可达性
  await diag('D. Tauri 命令可达性...')
  allResults.push(...(await testCommandAvailability()))
  await diag(`D 完成: ${allResults.length} 用例`)

  // 汇总
  const summary = {
    total: allResults.length,
    passed: allResults.filter((r) => r.status === 'pass').length,
    failed: allResults.filter((r) => r.status === 'fail').length,
    skipped: allResults.filter((r) => r.status === 'skip').length,
  }

  const report: SelfTestReport = {
    timestamp: new Date().toISOString(),
    duration_ms: Math.round(performance.now() - start),
    summary,
    results: allResults,
  }

  await writeReport(report)

  console.warn(
    `[self-test] 完成: ${summary.passed}/${summary.total} 通过` +
      (summary.failed > 0 ? `, ${summary.failed} 失败` : '') +
      ` (${report.duration_ms}ms)`,
  )
}
