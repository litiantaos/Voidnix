#!/usr/bin/env bun
import { readdir, readFile, writeFile, stat } from 'node:fs/promises'
import { join } from 'node:path'

const ROOT = process.cwd()
const EXTENSIONS_DIR = join(ROOT, 'extensions')
const BACKEND_EXT_FILE = join(ROOT, 'src-tauri', 'src', 'extensions.rs')
const TYPE_GEN_FILE = join(ROOT, 'src-tauri', 'src', 'type_gen.rs')
const COMMANDS_DIR = join(ROOT, 'src-tauri', 'src', 'commands')
const CORE_DIR = join(ROOT, 'src-tauri', 'src')

const IS_CHECK_MODE = process.argv.includes('--check')

async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path)
    return true
  } catch {
    return false
  }
}

async function scanExtensions(): Promise<string[]> {
  if (!(await pathExists(EXTENSIONS_DIR))) return []
  const items = await readdir(EXTENSIONS_DIR, { withFileTypes: true })
  return items
    .filter((d) => d.isDirectory() && !d.name.startsWith('.') && d.name !== 'README.md')
    .map((d) => d.name)
    .sort()
}

// 匹配 #[tauri::command] 及后续可能的 #[cfg_attr(...)]，然后 pub (async)? fn name(
const COMMAND_REGEX = /#\[tauri::command\]\s*(?:#\[.*?\]\s*)*pub\s+(?:async\s+)?fn\s+(\w+)/g

// 匹配 specta 注解的命令（用于生成 TypeScript bindings）
const SPECTA_COMMAND_REGEX =
  /#\[tauri::command\]\s*#\[cfg_attr\(feature\s*=\s*"specta"[^)]*\)\]\s*pub\s+(?:async\s+)?fn\s+(\w+)/g

// 匹配 pub fn init(  而非 init_something(
const INIT_REGEX = /^pub\s+fn\s+init\s*\(/m

function extractCommands(content: string): string[] {
  const commands: string[] = []
  let match: RegExpExecArray | null
  while ((match = COMMAND_REGEX.exec(content)) !== null) {
    commands.push(match[1])
  }
  return commands
}

function extractSpectaCommands(content: string): string[] {
  const commands: string[] = []
  let match: RegExpExecArray | null
  while ((match = SPECTA_COMMAND_REGEX.exec(content)) !== null) {
    commands.push(match[1])
  }
  return commands
}

function hasInit(content: string): boolean {
  return INIT_REGEX.test(content)
}

interface ModuleMeta {
  module: string
  commands: string[]
  hasInit: boolean
  source: 'built-in' | 'extension' | 'core'
  backendPath?: string // relative path from src-tauri/src/extensions/ to mod.rs
  spectaCommands?: string[] // 需要生成 TypeScript bindings 的命令
}

// 核心模块：不在 extensions/ 下，而是 src-tauri/src/core/ 下的 .rs 文件
const CORE_MODULES = ['permission', 'shortcut', 'window']

async function scanBuiltInCommands(): Promise<ModuleMeta[]> {
  const results: ModuleMeta[] = []
  if (!(await pathExists(COMMANDS_DIR))) return results

  const files = await readdir(COMMANDS_DIR)

  for (const file of files) {
    if (!file.endsWith('.rs') || file === 'mod.rs') continue
    const module = file.replace('.rs', '')
    const content = await readFile(join(COMMANDS_DIR, file), 'utf-8')
    const commands = extractCommands(content)
    const spectaCommands = extractSpectaCommands(content)
    const init = hasInit(content)
    if (commands.length > 0 || init) {
      results.push({ module, commands, spectaCommands, hasInit: init, source: 'built-in' })
    }
  }

  return results
}

async function scanCoreModules(): Promise<ModuleMeta[]> {
  const results: ModuleMeta[] = []
  for (const name of CORE_MODULES) {
    const filePath = join(CORE_DIR, 'core', `${name}.rs`)
    if (!(await pathExists(filePath))) continue
    const content = await readFile(filePath, 'utf-8')
    const commands = extractCommands(content)
    const spectaCommands = extractSpectaCommands(content)
    const init = hasInit(content)
    if (commands.length > 0 || init) {
      results.push({ module: name, commands, spectaCommands, hasInit: init, source: 'core' })
    }
  }
  return results
}

async function scanRsFiles(dir: string): Promise<string[]> {
  const results: string[] = []
  if (!(await pathExists(dir))) return results
  const entries = await readdir(dir, { withFileTypes: true })
  for (const entry of entries) {
    const fullPath = join(dir, entry.name)
    if (entry.isDirectory()) {
      results.push(...(await scanRsFiles(fullPath)))
    } else if (entry.isFile() && entry.name.endsWith('.rs')) {
      results.push(fullPath)
    }
  }
  return results
}

function rsPathToModulePath(nativeDir: string, filePath: string): string {
  const relPath = filePath.slice(nativeDir.length + 1)
  return relPath
    .replace(/\.rs$/, '')
    .replace(/\/mod$/, '')
    .replace(/\//g, '::')
}

async function scanExtensionBackends(): Promise<ModuleMeta[]> {
  const results: ModuleMeta[] = []
  if (!(await pathExists(EXTENSIONS_DIR))) return results

  const dirs = await readdir(EXTENSIONS_DIR, { withFileTypes: true })
  for (const dir of dirs.filter((d) => d.isDirectory())) {
    const nativeDir = join(EXTENSIONS_DIR, dir.name, 'native')
    const modFile = join(nativeDir, 'mod.rs')
    if (!(await pathExists(modFile))) continue

    const allCommands: string[] = []
    const allSpectaCommands: string[] = []
    let hasInitFn = false

    const modContent = await readFile(modFile, 'utf-8')
    allCommands.push(...extractCommands(modContent))
    allSpectaCommands.push(...extractSpectaCommands(modContent))
    if (hasInit(modContent)) hasInitFn = true

    const rsFiles = await scanRsFiles(nativeDir)
    for (const filePath of rsFiles) {
      if (filePath === modFile) continue
      const modulePath = rsPathToModulePath(nativeDir, filePath)
      const content = await readFile(filePath, 'utf-8')
      for (const cmd of extractCommands(content)) {
        allCommands.push(`${modulePath}::${cmd}`)
      }
      for (const cmd of extractSpectaCommands(content)) {
        allSpectaCommands.push(`${modulePath}::${cmd}`)
      }
    }

    if (allCommands.length > 0 || hasInitFn) {
      results.push({
        module: dir.name,
        commands: allCommands,
        spectaCommands: allSpectaCommands,
        hasInit: hasInitFn,
        source: 'extension',
        backendPath: `../../extensions/${dir.name}/native/mod.rs`,
      })
    }
  }

  return results
}

function buildModContent(allModules: ModuleMeta[]): string {
  const extModules = allModules.filter((m) => m.source === 'extension')

  // mod declarations with #[path] for extension backends
  const rustModuleName = (name: string) => name.replace(/-/g, '_')

  const modLines: string[] = []
  for (const m of extModules) {
    if (m.backendPath) {
      modLines.push(`#[path = "${m.backendPath}"]`)
      modLines.push(`pub mod ${rustModuleName(m.module)};`)
    }
  }

  const commandPaths: string[] = []
  for (const m of allModules) {
    const modName = rustModuleName(m.module)
    const prefix = m.source === 'extension' ? `crate::extensions` : `crate::core`
    for (const cmd of m.commands) {
      commandPaths.push(`        ${prefix}::${modName}::${cmd},`)
    }
  }

  const initLines: string[] = []
  for (const m of allModules) {
    if (m.hasInit) {
      const modName = rustModuleName(m.module)
      const prefix = m.source === 'extension' ? `crate::extensions` : `crate::core`
      initLines.push(`        .plugin(${prefix}::${modName}::init())`)
    }
  }

  // 用 macro_rules! 在调用点展开，保留 lib.rs 顶层的具体类型推断
  return [
    `// Auto-generated by scripts/sync-extensions.ts`,
    `// Do not edit manually.`,
    ``,
    `macro_rules! configure_app {`,
    `    ($builder:expr) => {`,
    `        $builder`,
    `            .invoke_handler(tauri::generate_handler![`,
    ...commandPaths,
    `            ])`,
    ...initLines,
    `    };`,
    `}`,
    `pub(crate) use configure_app;`,
    ``,
    ...modLines,
    ``,
  ].join('\n')
}

function buildTypeGenContent(allModules: ModuleMeta[]): string {
  const rustModuleName = (name: string) => name.replace(/-/g, '_')

  const commandPaths: string[] = []
  for (const m of allModules) {
    if (m.spectaCommands && m.spectaCommands.length > 0) {
      const modName = rustModuleName(m.module)
      const prefix = m.source === 'extension' ? `crate::extensions` : `crate::core`
      for (const cmd of m.spectaCommands!) {
        commandPaths.push(`            ${prefix}::${modName}::${cmd},`)
      }
    }
  }

  return [
    `// Auto-generated by scripts/sync-extensions.ts`,
    `// Do not edit manually.`,
    ``,
    `// 此模块仅在 specta feature 启用时编译。`,
    `// 运行方式：cargo test --features specta export_bindings -- --nocapture`,
    `#![cfg(feature = "specta")]`,
    ``,
    `use specta_typescript::Typescript;`,
    `use tauri_specta::{collect_commands, Builder, ErrorHandlingMode};`,
    ``,
    `/// 生成 TypeScript bindings 并写入 src/bindings.ts。`,
    `#[test]`,
    `pub fn export_bindings() {`,
    `    let builder = Builder::<tauri::Wry>::new()`,
    `        .error_handling(ErrorHandlingMode::Throw)`,
    `        .commands(collect_commands![`,
    ...commandPaths,
    `        ]);`,
    ``,
    `    let out_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");`,
    ``,
    `    builder`,
    `        .export(Typescript::default(), &out_path)`,
    `        .expect("生成 TypeScript bindings 失败");`,
    ``,
    `    println!(`,
    `        "✅ bindings 已生成：{}",`,
    `        out_path.canonicalize().unwrap().display()`,
    `    );`,
    `}`,
    ``,
  ].join('\n')
}

async function generateRegistry(allModules: ModuleMeta[]) {
  const modContent = buildModContent(allModules)
  await writeFile(BACKEND_EXT_FILE, modContent)

  const typeGenContent = buildTypeGenContent(allModules)
  await writeFile(TYPE_GEN_FILE, typeGenContent)
}

async function checkRegistry(allModules: ModuleMeta[]): Promise<boolean> {
  const modContent = buildModContent(allModules)

  if (await pathExists(BACKEND_EXT_FILE)) {
    const existing = await readFile(BACKEND_EXT_FILE, 'utf-8')
    if (existing !== modContent) {
      console.error(`[sync-extensions] CHECK FAILED: ${BACKEND_EXT_FILE} is out of date.`)
      return false
    }
  } else {
    console.error(`[sync-extensions] CHECK FAILED: ${BACKEND_EXT_FILE} does not exist.`)
    return false
  }

  const typeGenContent = buildTypeGenContent(allModules)
  if (await pathExists(TYPE_GEN_FILE)) {
    const existing = await readFile(TYPE_GEN_FILE, 'utf-8')
    if (existing !== typeGenContent) {
      console.error(`[sync-extensions] CHECK FAILED: ${TYPE_GEN_FILE} is out of date.`)
      return false
    }
  } else {
    console.error(`[sync-extensions] CHECK FAILED: ${TYPE_GEN_FILE} does not exist.`)
    return false
  }

  return true
}

async function main() {
  console.log('[sync-extensions] Scanning extensions...')

  const extNames = await scanExtensions()
  console.log(
    `[sync-extensions] Found ${extNames.length} extension(s): ${extNames.join(', ') || 'none'}`,
  )

  const builtIns = await scanBuiltInCommands()
  const coreModules = await scanCoreModules()
  const extBackends = await scanExtensionBackends()
  const allModules = [...builtIns, ...coreModules, ...extBackends]

  if (IS_CHECK_MODE) {
    const ok = await checkRegistry(allModules)
    if (!ok) {
      console.error('[sync-extensions] Run `bun run sync:extensions` to regenerate.')
      process.exit(1)
    }
    console.log('[sync-extensions] Check passed.')
    return
  }

  await generateRegistry(allModules)

  const totalCommands = allModules.reduce((sum, m) => sum + m.commands.length, 0)
  const totalPlugins = allModules.filter((m) => m.hasInit).length

  console.log('[sync-extensions] Done.')
  console.log(
    `  Modules:  ${builtIns.length} built-in + ${coreModules.length} core + ${extBackends.length} extension`,
  )
  console.log(`  Commands: ${totalCommands}`)
  console.log(`  Plugins:  ${totalPlugins}`)
}

main().catch((e) => {
  console.error('[sync-extensions] Error:', e)
  process.exit(1)
})
