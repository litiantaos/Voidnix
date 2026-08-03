#!/usr/bin/env bun
// 扫描 extensions/*/native/mod.rs：
//   - 有 native/mod.rs 的扩展 → register_all() 生命周期注册 + mod 声明
//   - 各 native #[tauri::command] → 全局 generate_handler! 列表
// 生成 extensions.rs：configure_app! + register_all + mod 声明。
//
// 命令注册边界：
//   - 所有 #[tauri::command]（框架 + 扩展）在 configure_app! 的单一 generate_handler! 全局注册。
//     前端用裸名 invoke('cmd')。扩展命令必须全局注册（Tauri 2 插件命令需 'plugin:name|cmd'
//     格式，裸名只路由全局 invoke_handler）。
//   - 扩展无 plugin 空壳（旧 init() 纯 Builder::new().build()，对运行时零贡献，已消除）。
import { readdirSync, readFileSync, writeFileSync, existsSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'

const ROOT = process.cwd()
const EXT_DIR = join(ROOT, 'extensions')
const OUT_FILE = join(ROOT, 'src-tauri', 'src', 'extensions.rs')

const COMMAND_ATTR = /^#\[\s*tauri::command/
const FN_NAME = /(?:pub\s+)?(?:async\s+)?fn\s+(\w+)/
const rustName = (id: string) => id.replace(/-/g, '_')

/** 含 native/ 的扩展（按目录名排序）。
 *  扩展无需再声明 init()：plugin 空壳已消除，命令全局注册、State 经 Extension trait setup 管理。 */
function scanNativeExtensions(): string[] {
  const ids: string[] = []
  for (const dir of readdirSync(EXT_DIR)) {
    const modPath = join(EXT_DIR, dir, 'native', 'mod.rs')
    if (existsSync(modPath)) ids.push(dir)
  }
  return ids.sort()
}

/** 递归收集 .rs 文件。skip 匹配文件/目录名（跳过自动生成 / binary 入口等）。 */
function walkRs(dir: string, out: string[] = [], skip: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (skip.includes(name)) continue
    const full = join(dir, name)
    const st = statSync(full)
    if (st.isDirectory()) walkRs(full, out, skip)
    else if (full.endsWith('.rs')) out.push(full)
  }
  return out
}

/** 文件相对 native/ 的路径 → Rust 模块路径（mod.rs 折叠）。 */
function relToModule(rel: string): string {
  const parts = rel
    .replace(/\.rs$/, '')
    .split('/')
    .filter((p) => p !== 'mod')
  return parts.join('::')
}

/** 扫描文件内 #[tauri::command] fn 名（逐行，跳过中间属性与 doc 注释）。 */
function scanCommandsInFile(src: string): string[] {
  const cmds: string[] = []
  const lines = src.split('\n')
  let i = 0
  while (i < lines.length) {
    if (COMMAND_ATTR.test(lines[i].trim())) {
      let j = i + 1
      while (
        j < lines.length &&
        (/^\s*#\[/.test(lines[j]) || lines[j].trim() === '' || lines[j].trim().startsWith('///'))
      )
        j++
      const m = lines[j]?.match(FN_NAME)
      if (m) cmds.push(m[1])
      i = j + 1
    } else {
      i++
    }
  }
  return cmds
}

/** 扫描 src-tauri/src/ 下框架级 #[tauri::command]，按文件路径推断全局模块路径。
 * 取代硬编码 FRAMEWORK_COMMANDS：新增框架命令自动进 generate_handler!，无需手动维护清单。 */
function scanFrameworkCommands(): string[] {
  const srcDir = join(ROOT, 'src-tauri', 'src')
  // 排除自动生成产物（extensions.rs，会递归引用自身）与 binary 入口（main.rs）
  const skip = ['extensions.rs', 'main.rs']
  const cmds: string[] = []
  for (const file of walkRs(srcDir, [], skip)) {
    const src = readFileSync(file, 'utf8')
    const fns = scanCommandsInFile(src)
    if (fns.length === 0) continue
    const modPath = relToModule(relative(srcDir, file))
    const prefix = modPath ? `crate::${modPath}::` : 'crate::'
    for (const fn of fns) cmds.push(`${prefix}${fn}`)
  }
  return cmds.sort()
}

/** 扫描各扩展 native/ 下 #[tauri::command]，按文件路径推断全局模块路径。 */
function scanExtensionCommands(ids: string[]): string[] {
  const cmds: string[] = []
  for (const id of ids) {
    const nativeDir = join(EXT_DIR, id, 'native')
    // skip ['src']：排除 zsh-autosuggestions/native/src/（独立 binary，clap #[command] 非 tauri 命令）
    for (const file of walkRs(nativeDir, [], ['src'])) {
      const src = readFileSync(file, 'utf8')
      const modPath = relToModule(relative(nativeDir, file))
      const prefix = modPath ? `${modPath}::` : ''
      for (const fn of scanCommandsInFile(src)) {
        cmds.push(`crate::extensions::${rustName(id)}::${prefix}${fn}`)
      }
    }
  }
  return cmds.sort()
}

/** 扫描 native/mod.rs 中 `pub struct XxxExtension`（生命周期注册用）。 */
function scanExtensionStruct(id: string): string {
  const modPath = join(EXT_DIR, id, 'native', 'mod.rs')
  const src = readFileSync(modPath, 'utf8')
  const m = src.match(/pub\s+struct\s+(\w+Extension)\b/)
  if (!m) {
    throw new Error(
      `[sync-extensions] extensions/${id}/native/mod.rs 缺少 pub struct *Extension（bootstrap 注册需要）`,
    )
  }
  return m[1]
}

function buildExtensionsRs(ids: string[], frameworkCmds: string[], extCmds: string[]): string {
  const allCmds = [...frameworkCmds, ...extCmds]
  const cmdList = allCmds.map((c) => `            ${c},`).join('\n')
  const modDecls = ids
    .map((id) => `#[path = "../../extensions/${id}/native/mod.rs"]\npub mod ${rustName(id)};`)
    .join('\n\n')
  // 生命周期 register 列表：与 native 扫描同源，消灭 lib.rs 手写双轨
  // 首个 .register 与 reg 同行（对齐 rustfmt 链式格式：receiver 与首调用合并）
  const registerChain = ids
    .map((id, i) => {
      const structName = scanExtensionStruct(id)
      const target = `${rustName(id)}::${structName}`
      return i === 0 ? `    reg.register(${target})` : `        .register(${target})`
    })
    .join('\n')
  return `// AUTO-GENERATED by scripts/sync-extensions.ts. DO NOT EDIT.
// configure_app! 含全局 invoke_handler（框架 + 扩展命令）。
// register_all 注册全部 native Extension trait 实现（bootstrap 生命周期）。
// 扩展命令全局注册（裸名 invoke）；扩展无 plugin 空壳（命令走 generate_handler!，
// State 经 Extension trait setup 管理）。

macro_rules! configure_app {
    ($builder:expr) => {
        $builder.invoke_handler(tauri::generate_handler![
${cmdList}
        ])
    };
}
pub(crate) use configure_app;

/// 将全部 native 扩展注册进 registry（与 native 扫描同源，禁止手写清单）。
pub fn register_all(
    reg: crate::runtime::registry::ExtensionRegistry,
) -> crate::runtime::registry::ExtensionRegistry {
${registerChain}
}

${modDecls}
`
}

const ids = scanNativeExtensions()
const extCmds = scanExtensionCommands(ids)
const frameworkCmds = scanFrameworkCommands()
const content = buildExtensionsRs(ids, frameworkCmds, extCmds)

if (process.argv.includes('--check')) {
  const existing = existsSync(OUT_FILE) ? readFileSync(OUT_FILE, 'utf8') : ''
  if (existing !== content) {
    console.error(
      '[sync-extensions] CHECK FAILED: extensions.rs is out of date. Run `bun run sync:extensions`.',
    )
    process.exit(1)
  }
  console.log(
    `[sync-extensions] Check passed (${ids.length} extensions, ${frameworkCmds.length + extCmds.length} commands).`,
  )
} else {
  writeFileSync(OUT_FILE, content)
  console.log(
    `[sync-extensions] Synced ${ids.length} extensions, ${frameworkCmds.length + extCmds.length} commands (${extCmds.length} extension + ${frameworkCmds.length} framework, auto-discovered)`,
  )
}
