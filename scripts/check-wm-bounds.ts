#!/usr/bin/env bun
// check:wm-bounds —— window-manager 自定义尺寸 floor/cap Rust↔TS 双向一致校验。
// 权威源在 extensions/window-manager/native/mod.rs 的 const（WIDTH_BOUNDS / HEIGHT_BOUNDS），
// config.ts 的 BOUNDS 仅 UI 镜像（须手动同步）。本脚本把人工同步变成 CI 强制约束。
//
// 校验项（每项 Rust 与 TS 必须完全一致）：
//   - 2 个数值 tuple（WIDTH_BOUNDS / HEIGHT_BOUNDS）→ BOUNDS.<field>.{floor,cap}
//   - 2 个默认值（customWidth / customHeight defineConfig 初始值）
//
// 用法：bun run scripts/check-wm-bounds.ts
import { readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const ROOT = process.cwd()
const MOD_RS = join(ROOT, 'extensions/window-manager/native/mod.rs')
const CONFIG_TS = join(ROOT, 'extensions/window-manager/config.ts')

const TAG = '[check:wm-bounds]'

const TUPLE_FIELDS = [
  { rust: 'WIDTH_BOUNDS', ts: 'customWidth' },
  { rust: 'HEIGHT_BOUNDS', ts: 'customHeight' },
] as const

const DEFAULT_FIELDS = [
  { rust_name: 'customWidth', ts: 'customWidth' },
  { rust_name: 'customHeight', ts: 'customHeight' },
] as const

function fail(msg: string): never {
  console.error(`${TAG} ✗ ${msg}`)
  process.exit(1)
}

function load(path: string, label: string): string {
  if (!existsSync(path)) fail(`${label} 不存在：${path}`)
  return readFileSync(path, 'utf8')
}

const rustSrc = load(MOD_RS, 'mod.rs')
const tsSrc = load(CONFIG_TS, 'config.ts')

// --- Rust 解析 ---
// tuple：`const NAME: (f64, f64) = (200.0, 4096.0);`
function parseRustTuple(name: string): [number, number] {
  const re = new RegExp(`${name}:\\s*\\(f64,\\s*f64\\)\\s*=\\s*\\(([^)]+)\\)`)
  const m = rustSrc.match(re)
  if (!m) fail(`mod.rs 未找到 const ${name} tuple 声明`)
  const parts = m[1].split(',').map((s) => Number(s.trim()))
  if (parts.length !== 2 || parts.some((n) => !Number.isFinite(n))) {
    fail(`mod.rs ${name} tuple 解析失败：${m[1]}`)
  }
  return [parts[0], parts[1]]
}

// --- TS 解析 ---
// tuple：`field: { floor: 200, cap: 4096 },`
function parseTsTuple(field: string): [number, number] {
  const re = new RegExp(`${field}:\\s*\\{\\s*floor:\\s*(\\d+)\\s*,\\s*cap:\\s*(\\d+)\\s*\\}`)
  const m = tsSrc.match(re)
  if (!m) fail(`config.ts BOUNDS.${field} 未找到或结构不符 { floor, cap }`)
  return [Number(m[1]), Number(m[2])]
}

// TS 默认值：`field: VALUE,`（在 defineConfig 对象内，非 BOUNDS 内）
function parseTsDefault(field: string): number {
  const re = new RegExp(`^\\s*${field}:\\s*(\\d+)\\s*,`, 'm')
  const m = tsSrc.match(re)
  if (!m) fail(`config.ts 未找到 ${field} 默认值（defineConfig 内）`)
  return Number(m[1])
}

// --- 比对 ---
const diffs: string[] = []

for (const { rust, ts } of TUPLE_FIELDS) {
  const [rFloor, rCap] = parseRustTuple(rust)
  const [tFloor, tCap] = parseTsTuple(ts)
  if (rFloor !== tFloor) diffs.push(`${ts}.floor  Rust=${rFloor}  TS=${tFloor}`)
  if (rCap !== tCap) diffs.push(`${ts}.cap    Rust=${rCap}  TS=${tCap}`)
}

for (const { ts } of DEFAULT_FIELDS) {
  // Rust 端默认值是 STATE 初始化 + set_frontmost_window_layout fallback，
  // 与 TS defineConfig 初始值一致即可（非安全问题，Rust clamp 兜底）。
  const tVal = parseTsDefault(ts)
  // Rust 端无独立 DEFAULT_* 常量，STATE 初始值在 window_snap.rs，
  // fallback 在 mod.rs。此处仅校验 TS BOUNDS 内部一致性（floor ≤ default ≤ cap）。
  const [tFloor, tCap] = parseTsTuple(ts)
  if (tVal < tFloor || tVal > tCap) {
    diffs.push(`${ts} default=${tVal} 不在 [floor=${tFloor}, cap=${tCap}] 区间内`)
  }
}

if (diffs.length === 0) {
  const tuples = TUPLE_FIELDS.length
  console.log(`${TAG} ✓ BOUNDS 与 mod.rs 一致（${tuples} tuple + 默认值区间校验）。`)
  process.exit(0)
}

console.error(`${TAG} ✗ BOUNDS 与 mod.rs 漂移（权威源 = mod.rs）：`)
for (const d of diffs) console.error(`    ! ${d}`)
console.error(
  `${TAG} 共 ${diffs.length} 处不一致。请同步 extensions/window-manager/config.ts 的 BOUNDS。`,
)
process.exit(1)
