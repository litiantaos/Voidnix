#!/usr/bin/env bun
// check:agent-bounds —— agent 安全底线 Rust↔TS 双向一致校验（RV §3.4）。
// 权威源在 extensions/agent/native/policy.rs 的 const（floor/cap），config.ts 的 BOUNDS
// 仅 UI 镜像（⚠️ 须手动同步，config.ts:74）。本脚本把人工同步变成 CI 强制约束。
//
// 校验项（每项 Rust 与 TS 必须完全一致）：
//   - 6 个数值 tuple（MAX_TURNS / MAX_CPU_SECS / MAX_MEMORY_MB / MAX_OPEN_FILES /
//     EXECUTION_TIMEOUT_SECS / MAX_OUTPUT_BYTES）→ BOUNDS.<field>.{floor,cap}
//   - 2 个命令/参数数组（FORBIDDEN_FLOOR / DENIED_ARG_FLOOR）→ BOUNDS.<field>.floor
//     （排序后集合相等）
//
// 用法：bun run scripts/check-agent-bounds.ts
import { readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const ROOT = process.cwd()
const POLICY_RS = join(ROOT, 'extensions/agent/native/policy.rs')
const CONFIG_TS = join(ROOT, 'extensions/agent/config.ts')

const TAG = '[check:agent-bounds]'

// --- 字段映射：Rust const 名 ↔ TS BOUNDS 键名 ---
const TUPLE_FIELDS = [
  { rust: 'MAX_TURNS', ts: 'maxTurns' },
  { rust: 'MAX_CPU_SECS', ts: 'maxCpuSeconds' },
  { rust: 'MAX_MEMORY_MB', ts: 'maxMemoryMb' },
  { rust: 'MAX_OPEN_FILES', ts: 'maxOpenFiles' },
  { rust: 'EXECUTION_TIMEOUT_SECS', ts: 'executionTimeout' },
  { rust: 'MAX_OUTPUT_BYTES', ts: 'maxOutputBytes' },
] as const

const ARRAY_FIELDS = [
  { rust: 'FORBIDDEN_FLOOR', ts: 'forbiddenCommands', expect: 31 },
  { rust: 'DENIED_ARG_FLOOR', ts: 'blockedArgs', expect: 15 },
] as const

function fail(msg: string): never {
  console.error(`${TAG} ✗ ${msg}`)
  process.exit(1)
}

function load(path: string, label: string): string {
  if (!existsSync(path)) fail(`${label} 不存在：${path}`)
  return readFileSync(path, 'utf8')
}

const rustSrc = load(POLICY_RS, 'policy.rs')
const tsSrc = load(CONFIG_TS, 'config.ts')

// --- Rust 解析 ---
// tuple：`pub const NAME: (T, T) = (1, 50);`（数值含 _ 分隔符，如 10_485_760）
function parseRustTuple(name: string): [number, number] {
  const re = new RegExp(`${name}:\\s*\\([^)]*\\)\\s*=\\s*\\(([^)]+)\\)`)
  const m = rustSrc.match(re)
  if (!m) fail(`policy.rs 未找到 const ${name} tuple 声明`)
  const parts = m[1].split(',').map((s) => Number(s.trim().replace(/_/g, '')))
  if (parts.length !== 2 || parts.some((n) => !Number.isFinite(n))) {
    fail(`policy.rs ${name} tuple 解析失败：${m[1]}`)
  }
  return [parts[0], parts[1]]
}

// array：`pub const NAME: &[&str] = &[ "a", "b" ];`
function parseRustArray(name: string): string[] {
  const re = new RegExp(`${name}:\\s*&\\[&str\\]\\s*=\\s*&\\[([\\s\\S]*?)\\];`)
  const m = rustSrc.match(re)
  if (!m) fail(`policy.rs 未找到 const ${name} 数组声明`)
  const out: string[] = []
  const itemRe = /"([^"]+)"/g
  let im: RegExpExecArray | null
  while ((im = itemRe.exec(m[1])) !== null) out.push(im[1])
  return out
}

// --- TS 解析（BOUNDS 对象，`as const` 字面量）---
// tuple：`field: { floor: 1, cap: 50 },`
function parseTsTuple(field: string): [number, number] {
  const re = new RegExp(`${field}:\\s*\\{\\s*floor:\\s*(\\d+)\\s*,\\s*cap:\\s*(\\d+)\\s*\\}`)
  const m = tsSrc.match(re)
  if (!m) fail(`config.ts BOUNDS.${field} 未找到或结构不符 { floor, cap }`)
  return [Number(m[1]), Number(m[2])]
}

// array：`field: { floor: [ 'a', 'b' ] }`
function parseTsArray(field: string): string[] {
  const re = new RegExp(`${field}:\\s*\\{\\s*floor:\\s*\\[([\\s\\S]*?)\\]`)
  const m = tsSrc.match(re)
  if (!m) fail(`config.ts BOUNDS.${field}.floor 数组未找到`)
  const out: string[] = []
  const itemRe = /'([^']+)'/g
  let im: RegExpExecArray | null
  while ((im = itemRe.exec(m[1])) !== null) out.push(im[1])
  return out
}

// --- 比对 ---
const diffs: string[] = []

// 数值 tuple：floor / cap 逐项比对
for (const { rust, ts } of TUPLE_FIELDS) {
  const [rFloor, rCap] = parseRustTuple(rust)
  const [tFloor, tCap] = parseTsTuple(ts)
  if (rFloor !== tFloor) diffs.push(`${ts}.floor  Rust=${rFloor}  TS=${tFloor}`)
  if (rCap !== tCap) diffs.push(`${ts}.cap    Rust=${rCap}  TS=${tCap}`)
}

// 数组：排序后双向差集
for (const { rust, ts, expect } of ARRAY_FIELDS) {
  const r = parseRustArray(rust)
  const t = parseTsArray(ts)
  const rSet = new Set(r)
  const tSet = new Set(t)
  const onlyRust = [...new Set(r.filter((x) => !tSet.has(x)))].sort()
  const onlyTs = [...new Set(t.filter((x) => !rSet.has(x)))].sort()
  if (r.length !== expect)
    diffs.push(`${ts}.floor  Rust 项数=${r.length}（期望 ${expect}，policy.rs 注释/测试断言）`)
  if (onlyRust.length) diffs.push(`${ts}.floor  仅 Rust 有：[${onlyRust.join(', ')}]`)
  if (onlyTs.length) diffs.push(`${ts}.floor  仅 TS 有：[${onlyTs.join(', ')}]`)
}

if (diffs.length === 0) {
  const tuples = TUPLE_FIELDS.length
  const arrays = ARRAY_FIELDS.reduce((n, f) => n + parseRustArray(f.rust).length, 0)
  console.log(`${TAG} ✓ BOUNDS 与 policy.rs 一致（${tuples} tuple + ${arrays} 数组项）。`)
  process.exit(0)
}

console.error(`${TAG} ✗ BOUNDS 与 policy.rs 漂移（权威源 = policy.rs）：`)
for (const d of diffs) console.error(`    ! ${d}`)
console.error(`${TAG} 共 ${diffs.length} 处不一致。请同步 extensions/agent/config.ts 的 BOUNDS。`)
process.exit(1)
