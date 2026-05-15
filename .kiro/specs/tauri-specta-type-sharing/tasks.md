# 实现计划：Rust ↔ TypeScript 类型共享（tauri-specta）

## 概述

分五个阶段实现：Cargo 依赖配置 → Rust 结构体标注 → 代码生成入口 → 生成并验证 bindings → 前端迁移。每个阶段结束后运行对应检查，确保增量可验证。

## 任务

- [ ] 1. 配置 Cargo.toml：添加 specta feature 和可选依赖
  - 在 `[features]` 中添加 `specta = ["dep:specta", "dep:specta-typescript", "dep:tauri-specta"]`，保留现有 `webkit_tuning_mock` feature
  - 在 `[dependencies]` 中添加三个可选依赖：
    - `specta = { version = "0.1", optional = true }`
    - `specta-typescript = { version = "0.1", optional = true }`
    - `tauri-specta = { version = "2", features = ["derive", "typescript"], optional = true }`
  - 确认 `specta` feature 不在 `[features]` 的 `default` 列表中（生产构建不启用）
  - 运行 `cargo check`（不带 feature）确认生产构建不受影响
  - 运行 `cargo check --features specta` 确认 feature 依赖可正常解析
  - _需求：1.1, 1.2, 1.3, 1.4_

- [ ] 2. 为五个 Rust 结构体添加 specta::Type derive
  - [ ] 2.1 修改 `commands/search.rs`：为 `SearchResult` 添加 `#[cfg_attr(feature = "specta", derive(specta::Type))]`
    - 在现有 `#[derive(Debug, Clone, Serialize, Deserialize)]` 下方追加该属性
    - _需求：2.1_
  - [ ] 2.2 修改 `commands/clipboard.rs`：为 `ClipboardItem` 添加 `#[cfg_attr(feature = "specta", derive(specta::Type))]`
    - _需求：2.2_
  - [ ] 2.3 修改 `commands/translate.rs`：为 `TranslateResult` 添加 `#[cfg_attr(feature = "specta", derive(specta::Type))]`
    - _需求：2.3_
  - [ ] 2.4 修改 `commands/ip.rs`：为 `IpInfo` 添加 `#[cfg_attr(feature = "specta", derive(specta::Type))]`
    - _需求：2.4_
  - [ ] 2.5 修改 `commands/screenshot.rs`：为 `ScreenshotData` 添加 `#[cfg_attr(feature = "specta", derive(specta::Type))]`
    - _需求：2.5_
  - 运行 `cargo check --features specta` 确认所有 derive 编译通过
  - _需求：2.1–2.7_

- [ ] 3. 实现 type_gen.rs 代码生成入口
  - 创建 `src-tauri/src/type_gen.rs`，整个文件用 `#![cfg(feature = "specta")]` 门控
  - 实现 `export_bindings` 测试函数：
    - 使用 `tauri_specta::Builder::<tauri::Wry>::new()` 创建 builder
    - 使用 `collect_commands!` 收集以下 11 个命令：`search_files`、`search_apps`、`score_items`、`get_clipboard_history`、`translate_youdao`、`translate_ai`、`get_selected_text`、`fetch_ip_info`、`is_app_active`、`get_selected_text_cached`、`ocr_image`
    - 输出路径：`PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts")`
    - 调用 `builder.export(Typescript::default(), &out_path)` 写入文件
    - 成功后 `println!` 输出绝对路径
  - 在 `src-tauri/src/lib.rs` 顶部追加 `#[cfg(feature = "specta")] mod type_gen;`
  - 运行 `cargo test --features specta export_bindings -- --nocapture` 生成 `src/bindings.ts`
  - 验证 `src/bindings.ts` 已生成且包含预期的类型定义和命令函数
  - _需求：3.1, 3.2, 3.3, 3.4, 3.5_

- [ ] 4. 检查点——验证生成的 bindings.ts 内容
  - 确认 `src/bindings.ts` 包含以下类型导出：`SearchResult`、`ClipboardItem`、`TranslateResult`、`IpInfo`、`ScreenshotData`
  - 确认 `src/bindings.ts` 包含以下命令函数（或等效的 `commands` 对象属性）：`searchFiles`、`searchApps`、`getClipboardHistory`、`translateYoudao`、`translateAi`、`fetchIpInfo`、`scoreItems`、`isAppActive`、`getSelectedTextCached`、`getSelectedText`、`ocrImage`
  - 确认 `Option<T>` 字段生成为 `T | null`（检查 `IpInfo` 的 `ip` 字段）
  - 确认字段名保持 snake_case（检查 `ClipboardItem.content_type`、`ScreenshotData.data_url`）
  - 如有问题，调整 `type_gen.rs` 中的 specta 配置选项后重新生成
  - _需求：4.1, 4.2, 4.3, 4.5, 2.6, 2.7_

- [ ] 5. 配置 package.json 脚本
  - 在 `package.json` 的 `scripts` 中添加：`"gen:bindings": "cd src-tauri && cargo test --features specta export_bindings -- --nocapture"`
  - _需求：7.1, 7.2_

- [ ] 6. 迁移前端模块——search-files 和 search-apps
  - [ ] 6.1 修改 `src/utils/tauri.ts`：
    - 从 `@/bindings` 导入 `SearchResult` 类型（即 tauri-specta 生成的类型）
    - 删除 `TauriSearchResult` 接口定义
    - 将 `toSearchResults` 函数的参数类型从 `TauriSearchResult[]` 改为从 bindings 导入的 `SearchResult[]`
    - 确保 `toSearchResults` 内部逻辑与新类型兼容（字段名一致，无需改动）
    - _需求：5.6_
  - [ ] 6.2 修改 `src/modules/search-files/index.ts`：
    - 删除 `import { invoke } from '@tauri-apps/api/core'`（如仅用于 search_files）
    - 删除 `import { ..., type TauriSearchResult } from '@/utils/tauri'`
    - 添加 `import { commands } from '@/bindings'`
    - 将 `invoke<TauriSearchResult[]>('search_files', { query })` 替换为 `commands.searchFiles(query)`
    - _需求：5.1_
  - [ ] 6.3 修改 `src/modules/search-apps/index.ts`：
    - 删除 `import { invoke } from '@tauri-apps/api/core'`（如仅用于 search_apps）
    - 删除 `type TauriSearchResult` 导入
    - 添加 `import { commands } from '@/bindings'`
    - 将 `invoke<TauriSearchResult[]>('search_apps', { query })` 替换为 `commands.searchApps(query)`
    - _需求：5.2_

- [ ] 7. 迁移前端模块——clipboard
  - 修改 `src/modules/clipboard/index.ts`：
    - 删除本地 `ClipboardItem` 接口定义
    - 添加 `import { commands, type ClipboardItem } from '@/bindings'`
    - 将两处 `invoke<ClipboardItem[]>('get_clipboard_history', {...})` 替换为 `commands.getClipboardHistory(query, filterFavorite, limit)`
    - 注意参数名映射：Rust 命令参数为 `filter_favorite`，tauri-specta 生成的函数参数名以实际生成结果为准（通常为 camelCase）
    - 保留 `invoke('paste_clipboard_item', { id })` 不变（该命令不在 bindings 中）
    - _需求：5.3_

- [ ] 8. 迁移前端模块——translate
  - 修改 `src/modules/translate/index.ts`：
    - 删除本地 `TranslateResult` 接口定义（`export interface TranslateResult { ... }`）
    - 添加 `import { commands, type TranslateResult } from '@/bindings'`
    - 将 `invoke<TranslateResult>('translate_youdao', {...})` 替换为 `commands.translateYoudao(text, appKey, appSecret, targetLang)`
    - 将 `invoke<TranslateResult>('translate_ai', {...})` 替换为 `commands.translateAi(text, endpoint, apiKey, model, targetLang, prompt)`
    - 将 `invoke<string>('get_selected_text')` 替换为 `commands.getSelectedText()`
    - 保留 `invoke('translate_ai_stream', ...)` 不变（流式命令不在 bindings 中）
    - _需求：5.4, 5.7_

- [ ] 9. 迁移前端模块——ip
  - 修改 `src/modules/ip/index.ts`：
    - 删除 `import { invoke } from '@tauri-apps/api/core'`
    - 添加 `import { commands, type IpInfo } from '@/bindings'`
    - 将 `invoke<{ success: boolean; ip?: string; ... }>('fetch_ip_info', { ip: trimmed || null })` 替换为 `commands.fetchIpInfo(trimmed || null)`
    - 更新后续代码使用 `IpInfo` 类型（字段名与现有一致，无需改动逻辑）
    - _需求：5.5_

- [ ] 10. 检查点——TypeScript 严格模式验证
  - 运行 `bun run build`（执行 `vue-tsc --noEmit && vite build`）
  - 修复所有类型错误和未使用变量/导入警告
  - 常见问题处理：
    - 如果 bindings.ts 中的类型与现有代码有字段名差异，在对应模块中做适配
    - 如果 `toSearchResults` 函数签名不兼容，调整参数类型
    - 如果有未使用的导入（`noUnusedLocals`），删除对应 import 语句
  - _需求：6.1, 6.2, 6.3_

- [ ] 11. 更新 AGENTS.md 文档
  - 在 `AGENTS.md` 的「开发命令」部分添加：
    ```
    bun run gen:bindings      # 重新生成 src/bindings.ts（修改 Rust 结构体后执行）
    ```
  - _需求：7.3_

- [ ] 12. 最终检查点——确认所有测试通过
  - 运行 `cargo check`（不带 feature）确认生产构建干净
  - 运行 `cargo check --features specta` 确认 specta feature 编译通过
  - 运行 `bun run build` 确认前端类型检查和构建通过
  - 运行 `bun run lint` 确认 ESLint 无报错
  - 确认 `src/bindings.ts` 未被 `.gitignore` 排除
  - 如有问题，逐一修复后重新运行检查

## 备注

- 任务标有 `*` 的为可选任务，可跳过以加快 MVP 进度
- 流式命令（`translate_ai_stream`、`chat_stream`）和截图控制命令（`enter_screenshot_mode`、`exit_screenshot_mode`）不纳入 bindings，继续使用原始 `invoke`
- tauri-specta 生成的函数参数名遵循 camelCase，与 Rust 的 snake_case 命令参数名自动映射
- 如果 tauri-specta v2 的 API 与设计文档中的示例有出入，以实际 crates.io 文档为准
