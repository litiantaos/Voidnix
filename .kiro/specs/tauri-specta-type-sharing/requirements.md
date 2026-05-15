# 需求文档：Rust ↔ TypeScript 类型共享（tauri-specta）

## 简介

Voidnix 是 Tauri 2 + Rust + Vue 3 的 macOS 效率启动器。目前前端通过 `invoke<T>()` 调用 Rust 命令时，泛型参数 `T` 全部为手写类型断言，与 Rust 结构体靠人工同步，存在类型漂移风险。

本功能引入 `tauri-specta` + `specta`，从 Rust 结构体自动生成 TypeScript 类型定义和类型安全的命令调用层，消灭手写泛型断言，同时保证生产构建不引入 specta 运行时开销。

## 词汇表

- **Bindings**：由 `tauri-specta` 自动生成的 TypeScript 文件 `src/bindings.ts`，包含所有导出结构体的类型定义和类型安全的命令调用函数。
- **specta Feature**：Cargo feature `specta`，用于在 dev/test 路径下条件编译 specta 相关代码，生产构建不启用。
- **Type Generator**：`src-tauri/src/type_gen.rs` 中的代码生成入口，仅在 `specta` feature 启用时编译。
- **命令调用层**：`src/bindings.ts` 中导出的类型安全函数，替代前端直接调用 `invoke<T>()`。
- **TauriSearchResult**：`src/utils/tauri.ts` 中现有的手写接口，迁移完成后删除。

---

## 需求

### 需求 1：Cargo 依赖配置

**用户故事**：作为 Rust 开发者，我希望 specta 相关依赖仅在开发/测试路径下引入，以便生产构建不携带 specta 运行时开销。

#### 验收标准

1. THE `Cargo.toml` SHALL 在 `[features]` 中声明 `specta` feature，默认不启用。
2. THE `Cargo.toml` SHALL 在 `[dev-dependencies]` 中添加 `specta`、`specta-typescript`、`tauri-specta`（含 `derive` 和 `typescript` feature）。
3. WHEN `specta` feature 未启用时，THE 生产构建 SHALL 不包含任何 specta 相关代码路径。
4. WHERE `specta` feature 已启用，THE `Cargo.toml` SHALL 通过 `[dependencies]` 的可选依赖（`optional = true`）引入 specta，确保 `#[derive(specta::Type)]` 在 feature 门控下可用。

### 需求 2：Rust 结构体类型导出

**用户故事**：作为 Rust 开发者，我希望为所有需要跨边界共享的结构体添加 `specta::Type` derive，以便 tauri-specta 能自动生成对应的 TypeScript 类型。

#### 验收标准

1. THE `SearchResult`（`commands/search.rs`）SHALL 在 `specta` feature 启用时 derive `specta::Type`。
2. THE `ClipboardItem`（`commands/clipboard.rs`）SHALL 在 `specta` feature 启用时 derive `specta::Type`。
3. THE `TranslateResult`（`commands/translate.rs`）SHALL 在 `specta` feature 启用时 derive `specta::Type`。
4. THE `IpInfo`（`commands/ip.rs`）SHALL 在 `specta` feature 启用时 derive `specta::Type`。
5. THE `ScreenshotData`（`commands/screenshot.rs`）SHALL 在 `specta` feature 启用时 derive `specta::Type`。
6. WHEN 结构体字段类型为 `Option<T>` 时，THE 生成的 TypeScript 类型 SHALL 将该字段标注为可选（`T | null`）。
7. IF 结构体字段名使用 snake_case，THEN THE 生成的 TypeScript 类型 SHALL 保持 snake_case 字段名（与现有前端代码一致）。

### 需求 3：类型生成入口

**用户故事**：作为开发者，我希望有一个专用的代码生成入口，以便在开发时一键生成最新的 `src/bindings.ts`。

#### 验收标准

1. THE `Type Generator` SHALL 在 `src-tauri/src/type_gen.rs` 中实现，仅在 `specta` feature 启用时编译（`#[cfg(feature = "specta")]`）。
2. THE `Type Generator` SHALL 使用 `tauri_specta::collect_commands!` 收集以下命令：`search_files`、`search_apps`、`get_clipboard_history`、`translate_youdao`、`translate_ai`、`fetch_ip_info`、`score_items`、`is_app_active`、`get_selected_text_cached`、`get_selected_text`、`ocr_image`。
3. WHEN `Type Generator` 执行时，THE `Type Generator` SHALL 将生成结果写入 `src/bindings.ts`（相对于 workspace 根目录）。
4. THE `Type Generator` SHALL 作为 Rust 测试（`#[test]`）或独立 binary 运行，不依赖 Tauri 运行时。
5. WHEN 生成成功时，THE `Type Generator` SHALL 在控制台输出生成文件的绝对路径。

### 需求 4：生成的 TypeScript Bindings 文件

**用户故事**：作为前端开发者，我希望 `src/bindings.ts` 包含所有 Rust 结构体的 TypeScript 类型和类型安全的命令调用函数，以便替代手写的 `invoke<T>()` 调用。

#### 验收标准

1. THE `Bindings` SHALL 为 `SearchResult`、`ClipboardItem`、`TranslateResult`、`IpInfo`、`ScreenshotData` 各导出对应的 TypeScript 接口或类型别名。
2. THE `Bindings` SHALL 为需求 3.2 中列出的每个命令导出类型安全的调用函数，函数签名与 Rust 命令参数完全对应。
3. WHEN 命令返回 `Result<T, String>` 时，THE 生成的函数 SHALL 在 TypeScript 侧返回 `Promise<T>`（错误通过 Promise rejection 传递）。
4. THE `Bindings` 文件 SHALL 提交到 git，不在 `.gitignore` 中排除。
5. THE `Bindings` 文件 SHALL 包含文件头注释，说明该文件由 tauri-specta 自动生成，不应手动修改。

### 需求 5：前端迁移——替换手写 invoke 调用

**用户故事**：作为前端开发者，我希望将所有手写 `invoke<T>()` 调用替换为从 `bindings.ts` 导入的类型安全函数，以便消灭手写泛型断言。

#### 验收标准

1. THE `src/modules/search-files/index.ts` SHALL 从 `@/bindings` 导入 `SearchResult` 类型和 `searchFiles` 命令函数，替代 `invoke<TauriSearchResult[]>('search_files', ...)`。
2. THE `src/modules/search-apps/index.ts` SHALL 从 `@/bindings` 导入 `SearchResult` 类型和 `searchApps` 命令函数，替代 `invoke<TauriSearchResult[]>('search_apps', ...)`。
3. THE `src/modules/clipboard/index.ts` SHALL 从 `@/bindings` 导入 `ClipboardItem` 类型和 `getClipboardHistory` 命令函数，替代 `invoke<ClipboardItem[]>('get_clipboard_history', ...)`。
4. THE `src/modules/translate/index.ts` SHALL 从 `@/bindings` 导入 `TranslateResult` 类型和 `translateYoudao`、`translateAi` 命令函数，替代对应的 `invoke<TranslateResult>(...)` 调用。
5. THE `src/modules/ip/index.ts` SHALL 从 `@/bindings` 导入 `IpInfo` 类型和 `fetchIpInfo` 命令函数，替代 `invoke<{...}>('fetch_ip_info', ...)`。
6. WHEN 迁移完成后，THE `TauriSearchResult` 接口（`src/utils/tauri.ts`）SHALL 被删除，`toSearchResults` 函数 SHALL 改用从 `@/bindings` 导入的 `SearchResult` 类型。
7. WHEN 迁移完成后，THE `src/modules/translate/index.ts` 中的本地 `TranslateResult` 接口 SHALL 被删除，改用从 `@/bindings` 导入的类型。

### 需求 6：TypeScript 严格模式合规

**用户故事**：作为开发者，我希望迁移后的代码通过 TypeScript 严格模式检查，以便 `bun run build` 不因未使用变量报错。

#### 验收标准

1. WHEN 迁移完成后，THE 前端代码 SHALL 通过 `vue-tsc --noEmit` 检查，不产生类型错误。
2. THE 迁移后的代码 SHALL 不包含 `noUnusedLocals` 或 `noUnusedParameters` 违规（未使用的导入、变量、参数）。
3. IF 生成的 `bindings.ts` 包含 TypeScript 严格模式不兼容的代码，THEN THE `Type Generator` SHALL 配置 specta 输出选项以消除该问题。

### 需求 7：开发工作流集成

**用户故事**：作为开发者，我希望有清晰的命令来重新生成 bindings，以便在修改 Rust 结构体后能快速同步 TypeScript 类型。

#### 验收标准

1. THE `package.json` SHALL 添加 `"gen:bindings"` 脚本，执行 `cargo test --features specta export_bindings -- --nocapture`（或等效命令）以重新生成 `src/bindings.ts`。
2. WHEN 执行 `bun run gen:bindings` 时，THE 脚本 SHALL 在 `src-tauri` 目录下运行 cargo 命令并输出生成结果。
3. THE `README.md` 或 `AGENTS.md` SHALL 记录重新生成 bindings 的命令，供后续开发者参考。
