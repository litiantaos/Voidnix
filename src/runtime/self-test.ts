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
import { resolveLocalized } from './i18n'
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

/** 单次测试执行器：捕获异常转为 pass/fail/skip，记录耗时。
 *  抛带 `SKIP:` 前缀的 Error 时标 skip（用于网络依赖项等环境条件不满足）。 */
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
    const message = e instanceof Error ? e.message : String(e)
    const status: TestStatus = message.startsWith('SKIP:') ? 'skip' : 'fail'
    return {
      category,
      name,
      status,
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
    const label = ext ? resolveLocalized(ext.meta.name) : extId

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

// ── E. 窗口管理运行时启用 ──────────────────────────────────────────────────────

/// 覆盖回归：运行时 toggle WM 时，configure_snap_panel → apply_cached_appearance
/// 在主题已初始化（WINDOW_APPEARANCE = Some）下触发 Mutex 重入死锁，主线程永久卡死。
/// 启动期创建路径因时序侥幸逃逸（configure 先于 setWindowAppearance 执行，缓存仍 None）。
async function testWindowManagerRuntimeEnable(): Promise<TestResult[]> {
  const results: TestResult[] = []
  const timeout = (ms: number, msg: string) =>
    new Promise<never>((_, reject) => setTimeout(() => reject(new Error(msg)), ms))

  results.push(
    await runTest('window-manager', '运行时启用 (主题已初始化)', async () => {
      // 禁用→启用 toggle：configure_snap_panel 重跑，apply_cached_appearance 读
      // WINDOW_APPEARANCE（Some）。若 Mutex 重入死锁存在，invoke 永久阻塞，超时即判失败。
      // 同时验证禁用（停 drag monitor）不崩溃——窗口无法安全销毁，保持存活。
      await invoke<void>(CMD.setWindowManagerEnabled, { enabled: false }).catch(() => {})
      await Promise.race([
        invoke<void>(CMD.setWindowManagerEnabled, { enabled: true }),
        timeout(8000, 'invoke 超时(>8s)，疑似主线程死锁'),
      ])
    }),
  )

  return results
}

// ── F. 扩展功能正确性 ─────────────────────────────────────────────────────────

/// 验证扩展的核心命令/搜索行为返回正确结构——不只是「视图能渲染」。
/// 所有探测均无副作用（只读命令 + 搜索引擎调用）。
/// 网络依赖项失败标 skip 不标 fail（CI/无网环境不应阻断）。
async function testExtensionFunctional(): Promise<TestResult[]> {
  const results: TestResult[] = []
  const timeout = (ms: number) =>
    new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), ms))

  // ── invoke 命令验证（只读查询命令 + 字段级断言）──

  results.push(
    await runTest('extension-func', 'clipboard 历史查询结构', async () => {
      const r = await Promise.race([invoke(CMD.getClipboardHistory, { limit: 5 }), timeout(5000)])
      assert(Array.isArray(r), '应返回数组')
      const items = r as Record<string, unknown>[]
      if (items.length > 0) {
        const item = items[0]
        assert('id' in item, 'item 缺 id 字段')
        assert('content_type' in item, 'item 缺 content_type 字段')
        assert(
          typeof item.id === 'string' || typeof item.id === 'number',
          `id 应为 string|number，实际 ${typeof item.id}`,
        )
      }
    }),
  )

  results.push(
    await runTest('extension-func', 'system-status 实时快照字段', async () => {
      const r = await Promise.race([invoke(CMD.systemSnapshot), timeout(5000)])
      const s = r as Record<string, unknown>
      assert(typeof s === 'object' && s !== null, '应返回对象')
      assert(typeof s.cpu_usage === 'number', '缺 cpu_usage 数值字段')
      assert(
        typeof s.total_memory === 'number' && (s.total_memory as number) > 0,
        'total_memory 应 > 0',
      )
      assert(Array.isArray(s.cpu_cores_usage), 'cpu_cores_usage 应为数组')
      assert(typeof s.local_ip === 'string', '缺 local_ip 字段')
    }),
  )

  results.push(
    await runTest('extension-func', 'awake 状态查询', async () => {
      const r = await Promise.race([invoke(CMD.isAwakeEnabled), timeout(5000)])
      assert(typeof r === 'boolean', `应返回 boolean，实际 ${typeof r}`)
    }),
  )

  results.push(
    await runTest('extension-func', 'clean-mode 状态查询', async () => {
      const r = await Promise.race([invoke(CMD.isCleanModeEnabled), timeout(5000)])
      assert(typeof r === 'boolean', `应返回 boolean，实际 ${typeof r}`)
    }),
  )

  results.push(
    await runTest('extension-func', 'proxy 核心状态字段', async () => {
      const r = await Promise.race([invoke(CMD.proxyCoreStatus), timeout(5000)])
      const s = r as Record<string, unknown>
      assert(typeof s === 'object' && s !== null, '应返回对象')
      assert('downloaded' in s, '缺 downloaded 字段')
      assert(typeof s.downloaded === 'boolean', 'downloaded 应为 boolean')
    }),
  )

  results.push(
    await runTest('extension-func', 'homebrew 状态查询字段', async () => {
      const r = await Promise.race([invoke(CMD.brewStatus), timeout(8000)])
      const s = r as Record<string, unknown>
      assert(typeof s === 'object' && s !== null, '应返回对象')
      assert('version' in s, '缺 version 字段')
      assert(Array.isArray(s.packages), 'packages 应为数组')
    }),
  )

  results.push(
    await runTest('extension-func', 'video 核心状态字段', async () => {
      const r = await Promise.race([invoke(CMD.videoCoreStatus), timeout(5000)])
      const s = r as Record<string, unknown>
      assert(typeof s === 'object' && s !== null, '应返回对象')
      assert('available' in s, '缺 available 字段')
      assert(typeof s.available === 'boolean', 'available 应为 boolean')
    }),
  )

  results.push(
    await runTest('extension-func', 'finder-ext 选中路径查询', async () => {
      // finder_selected_paths 依赖 Finder 为前台应用，自测时 Finder 可能未激活
      // 验证命令可达不崩溃——Err（「请先切换到访达」）视为正常行为
      try {
        const r = await Promise.race([invoke(CMD.finderSelectedPaths), timeout(5000)])
        assert(Array.isArray(r), '应返回数组（无选中时为空数组）')
      } catch {
        // Finder 非前台时返回 Err 是预期行为
      }
    }),
  )

  results.push(
    await runTest('extension-func', 'translate 选中文本查询可达', async () => {
      const r = await Promise.race([invoke(CMD.getSelectedText), timeout(5000)])
      assert(typeof r === 'string', `应返回 string，实际 ${typeof r}`)
    }),
  )

  results.push(
    await runTest('extension-func', 'search 文件搜索可达', async () => {
      const r = await Promise.race([invoke(CMD.searchFiles, { query: 'test' }), timeout(8000)])
      assert(Array.isArray(r), '应返回数组')
    }),
  )

  // ── 工作流验证（命令协作链路）──

  results.push(
    await runTest('extension-func', 'clipboard 写入粘贴板可达', async () => {
      // pasteboardWriteText 写入系统剪贴板（带 source marker）
      // clipboard monitor 会捕获变化入库——验证写入不崩溃即可
      // （入库需要 monitor 异步触发，不在此等待）
      const marker = `voidnix-test-${Date.now()}`
      await invoke(CMD.pasteboardWriteText, { text: marker })
      // 无异常即通过——写入是 pasteClipboardItem 的核心依赖
    }),
  )

  results.push(
    await runTest('extension-func', 'translate 输入翻译流程', async () => {
      const { translateText, translateResults, isTranslating } =
        await import('@ext/translate/index')
      translateResults.value = []
      const { config: translateConfig } = await import('@ext/translate/config')
      const hasYoudao = translateConfig.configs.some(
        (c) => c.type === 'youdao' && c.appKey && c.appSecret,
      )
      if (!hasYoudao) {
        throw new Error('SKIP:未配置有道翻译')
      }
      await translateText('hello')
      const deadline = Date.now() + 8000
      while (isTranslating.value && Date.now() < deadline) {
        await sleep(100)
      }
      assert(!isTranslating.value, '翻译应在 8s 内完成')
      assert(translateResults.value.length > 0, '期望至少一条翻译结果')
      const first = translateResults.value[0]
      assert(!!first.translation && first.translation.length > 0, '翻译结果不应为空')
      // API 失败时 translation 填入错误文案——校验翻译结果含实际译文特征
      // （至少含英文字母或 CJK 字符，排除纯错误提示如 "Network request failed"）
      assert(
        /[a-zA-Z\u4e00-\u9fff]/.test(first.translation),
        `翻译结果疑似错误信息: "${first.translation}"`,
      )
    }),
  )

  // ── 搜索引擎即时答案验证 ──

  searchEngine.setActiveExtension(undefined)

  results.push(
    await runTest('extension-func', 'time 时间戳即时答案', async () => {
      // time 扩展仅扩展内转换（全局避免时间戳形态误触）
      searchEngine.setActiveExtension('time')
      try {
        const r = await searchEngine.search('1700000000')
        assert(r.length > 0, '期望有时间转换结果')
        const timeResult = r.find((x) => x.extId === 'time')
        assert(!!timeResult, '结果中无 time 扩展结果')
      } finally {
        searchEngine.setActiveExtension(undefined)
      }
    }),
  )

  results.push(
    await runTest('extension-func', 'uuid 入口可达', async () => {
      const r = await searchEngine.search('uuid')
      const entry = r.find((x) => x.extId === 'uuid')
      assert(!!entry, '结果中无 uuid 扩展入口')
    }),
  )

  // 网络依赖项：仅 timeout 标 skip，assert 失败标 fail
  results.push(
    await runTest('extension-func', 'ip 地址查询即时答案', async () => {
      // ip 扩展仅扩展内响应（全局模式返回空）
      searchEngine.setActiveExtension('ip')
      try {
        const r = await Promise.race([searchEngine.search('8.8.8.8'), timeout(8000)])
        const ipResult = r.find((x) => x.extId === 'ip')
        assert(!!ipResult, '结果中无 ip 扩展结果')
      } catch (e) {
        if (e instanceof Error && e.message === 'timeout') throw new Error('SKIP:网络不可用')
        throw e
      } finally {
        searchEngine.setActiveExtension(undefined)
      }
    }),
  )

  results.push(
    await runTest('extension-func', 'currency 汇率即时答案', async () => {
      try {
        const r = await Promise.race([searchEngine.search('100 usd'), timeout(8000)])
        const curResult = r.find((x) => x.extId === 'currency')
        assert(!!curResult, '结果中无 currency 扩展结果')
      } catch (e) {
        if (e instanceof Error && e.message === 'timeout') throw new Error('SKIP:网络不可用')
        throw e
      }
    }),
  )

  return results
}

// ── G. 搜索延迟基线 ───────────────────────────────────────────────────────────

/// 测量代表性 query 的 search() 端到端耗时，检测性能回归。
/// 阈值宽松（含 rAF + microtask），耗时记录入报告供人工对比。
const LATENCY_QUERIES: { query: string; threshold: number }[] = [
  { query: '', threshold: 80 }, // 空查询（默认列表，纯内存索引）
  { query: 'calc', threshold: 100 }, // keyword 匹配
  { query: '1+2', threshold: 60 }, // calculator 即时答案
  { query: 'SGVsbG8=', threshold: 60 }, // base64 即时答案
  { query: 'safa', threshold: 1000 }, // 应用搜索（Spotlight 首次查询可能慢）
]

async function testSearchLatency(): Promise<TestResult[]> {
  const results: TestResult[] = []
  searchEngine.setActiveExtension(undefined)

  for (const { query, threshold } of LATENCY_QUERIES) {
    const label = query === '' ? '空查询' : `'${query}'`
    results.push(
      await runTest('latency', `搜索延迟 ${label}`, async () => {
        const start = performance.now()
        await searchEngine.search(query)
        const elapsed = performance.now() - start
        assert(elapsed <= threshold, `${elapsed.toFixed(0)}ms 超过阈值 ${threshold}ms`)
      }),
    )
  }

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

  // E. 窗口管理运行时启用（回归：Mutex 重入死锁）
  await diag('E. 窗口管理运行时启用...')
  allResults.push(...(await testWindowManagerRuntimeEnable()))
  await diag(`E 完成: ${allResults.length} 用例`)

  // F. 扩展功能正确性
  await diag('F. 扩展功能正确性...')
  allResults.push(...(await testExtensionFunctional()))
  await diag(`F 完成: ${allResults.length} 用例`)

  // G. 搜索延迟基线
  await diag('G. 搜索延迟基线...')
  allResults.push(...(await testSearchLatency()))
  await diag(`G 完成: ${allResults.length} 用例`)

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
