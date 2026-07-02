#!/usr/bin/env bun
// check:agent-bounds —— agent 资源上限 Rust↔TS 双向一致校验。
// 权威源在 extensions/agent/native/policy.rs 的 const（floor/cap），config.ts 的 BOUNDS
// 仅 UI 镜像（须手动同步）。本脚本把人工同步变成 CI 强制约束。
//
// 校验项（每项 Rust 与 TS 必须完全一致）：
//   - 6 个数值 tuple（MAX_TURNS / MAX_CPU_SECS / MAX_MEMORY_MB / MAX_OPEN_FILES /
//     EXECUTION_TIMEOUT_SECS / MAX_OUTPUT_BYTES）→ BOUNDS.<field>.{floor,cap}
//   - 6 个 DEFAULT_* 默认值（policy.rs fallback ↔ config.ts 初始值）
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

// DEFAULT_* 默认值比对（policy.rs fallback ↔ config.ts 初始值）
// Rust 端 None 时 fallback 到 DEFAULT_*；TS 端 defineConfig 的初始值须与之对齐，
// 否则 UI 显示与 Rust fallback 不一致（非安全问题，Rust clamp 兜底）。
const DEFAULT_FIELDS = [
  { rust: 'DEFAULT_MAX_TURNS', ts: 'maxTurns' },
  { rust: 'DEFAULT_MAX_CPU_SECS', ts: 'maxCpuSeconds' },
  { rust: 'DEFAULT_MAX_MEMORY_MB', ts: 'maxMemoryMb' },
  { rust: 'DEFAULT_MAX_OPEN_FILES', ts: 'maxOpenFiles' },
  { rust: 'DEFAULT_EXECUTION_TIMEOUT_SECS', ts: 'executionTimeout' },
  { rust: 'DEFAULT_MAX_OUTPUT_BYTES', ts: 'maxOutputBytes' },
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

// --- TS 解析（BOUNDS 对象，`as const` 字面量）---
// tuple：`field: { floor: 1, cap: 50 },`
function parseTsTuple(field: string): [number, number] {
  const re = new RegExp(`${field}:\\s*\\{\\s*floor:\\s*(\\d+)\\s*,\\s*cap:\\s*(\\d+)\\s*\\}`)
  const m = tsSrc.match(re)
  if (!m) fail(`config.ts BOUNDS.${field} 未找到或结构不符 { floor, cap }`)
  return [Number(m[1]), Number(m[2])]
}

// --- DEFAULT_* 默认值解析 ---
// Rust：`pub const NAME: TYPE = VALUE;`（VALUE 可能是 `1024 * 1024` 形式）
function parseRustDefault(name: string): number {
  const re = new RegExp(`${name}:\\s*(?:usize|u64)\\s*=\\s*([0-9_\\s*]+);`)
  const m = rustSrc.match(re)
  if (!m) fail(`policy.rs 未找到 const ${name} 默认值声明`)
  // 支持 `1024 * 1024` 形式：提取所有数字并相乘
  const nums = m[1].match(/\d[\d_]*/g)
  if (!nums || nums.length === 0) fail(`policy.rs ${name} 默认值解析失败：${m[1]}`)
  return nums.reduce((acc, n) => acc * Number(n.replace(/_/g, '')), 1)
}

// TS：`field: VALUE,`（在 config defineConfig 对象内，非 BOUNDS 内）
function parseTsDefault(field: string): number {
  // 排除 BOUNDS 块内的同名字段（BOUNDS 用 { floor, cap } 结构，值是对象）
  const re = new RegExp(`^\\s*${field}:\\s*(\\d[\\d_]*)\\s*,`, 'm')
  const m = tsSrc.match(re)
  if (!m) fail(`config.ts 未找到 ${field} 默认值（defineConfig 内）`)
  return Number(m[1].replace(/_/g, ''))
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

// DEFAULT_* 默认值逐项比对
for (const { rust, ts } of DEFAULT_FIELDS) {
  const rVal = parseRustDefault(rust)
  const tVal = parseTsDefault(ts)
  if (rVal !== tVal) diffs.push(`${ts} default  Rust=${rVal}  TS=${tVal}`)
}

if (diffs.length === 0) {
  const tuples = TUPLE_FIELDS.length
  const defaults = DEFAULT_FIELDS.length
  console.log(`${TAG} ✓ BOUNDS 与 policy.rs 一致（${tuples} tuple + ${defaults} 默认值）。`)
  process.exit(0)
}

console.error(`${TAG} ✗ BOUNDS 与 policy.rs 漂移（权威源 = policy.rs）：`)
for (const d of diffs) console.error(`    ! ${d}`)
console.error(`${TAG} 共 ${diffs.length} 处不一致。请同步 extensions/agent/config.ts 的 BOUNDS。`)
process.exit(1)
