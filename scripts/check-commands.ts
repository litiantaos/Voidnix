#!/usr/bin/env bun
// check:commands —— 命令名漂移检测（阻塞项，RV §2.8）。
// 扫描 Rust #[tauri::command] 函数名集合 ↔ 前端 src/commands.ts 常量值集合，双向差集须为空。
//   - Rust 删/改名而前端没跟 → caught（前端常量悬空）
//   - Rust 新增命令而前端没注册 → caught（提醒登记）
// 用法：bun run scripts/check-commands.ts [--check]
//   --check：CI 模式，不写文件（本脚本只校验，无写盘逻辑，--check 仅作占位与 sync-extensions 对齐）
import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const ROOT = process.cwd()
const RUST_DIRS = [join(ROOT, 'src-tauri/src'), join(ROOT, 'extensions')]
const COMMANDS_TS = join(ROOT, 'src/commands.ts')

// --- 收集 Rust #[tauri::command] fn 名 ---
// 逐行扫描：命中 #[tauri::command] 后，向下跳过其它 #[...] 属性/空行，直到首个 fn 声明。
const COMMAND_ATTR_LINE = /^#\[tauri::command[^\]]*\]/
const FN_LINE = /(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(/

function walkRs(dir: string, out: string[]) {
  let entries: string[]
  try {
    entries = readdirSync(dir)
  } catch {
    return
  }
  for (const name of entries) {
    const full = join(dir, name)
    let st
    try {
      st = statSync(full)
    } catch {
      continue
    }
    if (st.isDirectory()) walkRs(full, out)
    else if (full.endsWith('.rs')) out.push(full)
  }
}

function collectRustCommands(): Set<string> {
  const files: string[] = []
  for (const d of RUST_DIRS) walkRs(d, files)
  const names = new Set<string>()
  for (const f of files) {
    const lines = readFileSync(f, 'utf8').split(/\r?\n/)
    for (let i = 0; i < lines.length; i++) {
      if (!COMMAND_ATTR_LINE.test(lines[i].trim())) continue
      // 向下找 fn 声明，跳过其它 #[...] 属性与空行
      for (let j = i + 1; j < Math.min(i + 6, lines.length); j++) {
        const t = lines[j].trim()
        if (t === '' || t.startsWith('#[')) continue
        const m = t.match(FN_LINE)
        if (m) names.add(m[1])
        break
      }
    }
  }
  return names
}

// --- 收集 commands.ts 常量值（'snake_case' 字面量）---
function collectFrontendCommands(): Set<string> {
  if (!existsSync(COMMANDS_TS)) {
    console.error('[check:commands] src/commands.ts 不存在')
    process.exit(1)
  }
  const src = readFileSync(COMMANDS_TS, 'utf8')
  const values = new Set<string>()
  const VALUE_RE = /:\s*'([a-z][a-z0-9_]*)'/g
  let m: RegExpExecArray | null
  while ((m = VALUE_RE.exec(src)) !== null) {
    values.add(m[1])
  }
  return values
}

const rust = collectRustCommands()
const frontend = collectFrontendCommands()

const onlyRust = [...rust].filter((n) => !frontend.has(n)).sort()
const onlyFrontend = [...frontend].filter((n) => !rust.has(n)).sort()

// --- 裸 invoke('xxx') 检测：禁止绕过 commands.ts 常量直字符串调用 ---
// AGENTS.md：禁止裸 invoke('xxx')，统一走 CMD.xxx 通道。
const BARE_INVOKE_RE = /(?<![.\w$])invoke(?:<[^>]*>)?\(\s*['"]([a-z_][a-z0-9_]*)['"]/g
const SCAN_ROOTS = [join(ROOT, 'src'), join(ROOT, 'extensions')]

function walkTsVue(dir: string, out: string[]) {
  let entries: string[]
  try {
    entries = readdirSync(dir)
  } catch {
    return
  }
  for (const name of entries) {
    const full = join(dir, name)
    let st
    try {
      st = statSync(full)
    } catch {
      continue
    }
    if (st.isDirectory()) walkTsVue(full, out)
    else if (full.endsWith('.ts') || full.endsWith('.vue')) out.push(full)
  }
}

function collectBareInvokes(): { file: string; name: string }[] {
  const files: string[] = []
  for (const d of SCAN_ROOTS) walkTsVue(d, files)
  const hits: { file: string; name: string }[] = []
  for (const f of files) {
    if (f === COMMANDS_TS) continue // 常量定义文件，注释含 invoke('xxx') 示例
    if (f.endsWith('.test.ts')) continue
    const src = readFileSync(f, 'utf8')
    let m: RegExpExecArray | null
    BARE_INVOKE_RE.lastIndex = 0
    while ((m = BARE_INVOKE_RE.exec(src)) !== null) {
      hits.push({ file: f, name: m[1] })
    }
  }
  return hits
}

const bareInvokes = collectBareInvokes()

if (onlyRust.length === 0 && onlyFrontend.length === 0 && bareInvokes.length === 0) {
  console.log(`[check:commands] ✓ ${rust.size} commands in sync, no bare invoke.`)
  process.exit(0)
}

if (onlyFrontend.length) {
  console.error(`[check:commands] ✗ 前端 commands.ts 有而 Rust 无（命令名漂移/拼写错）：`)
  for (const n of onlyFrontend) console.error(`    + ${n}`)
}
if (onlyRust.length) {
  console.error(`[check:commands] ✗ Rust 有 #[tauri::command] 而前端 commands.ts 未登记：`)
  for (const n of onlyRust) console.error(`    - ${n}`)
}
if (bareInvokes.length) {
  console.error(`[check:commands] ✗ 检测到裸 invoke('xxx')，须改用 CMD.xxx 常量：`)
  for (const h of bareInvokes)
    console.error(`    ! ${h.name}  ←  ${h.file.replace(ROOT + '/', '')}`)
}
const total = onlyFrontend.length + onlyRust.length + bareInvokes.length
console.error(`[check:commands] 共 ${total} 处不一致。请同步 src/commands.ts。`)
process.exit(1)
