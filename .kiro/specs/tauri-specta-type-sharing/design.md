# 设计文档：Rust ↔ TypeScript 类型共享（tauri-specta）

## 概述

通过 `tauri-specta` + `specta` 从 Rust 结构体自动生成 TypeScript 类型定义和类型安全的命令调用层。整个方案分为三层：

1. **Rust 侧**：为结构体添加 `#[cfg_attr(feature = "specta", derive(specta::Type))]`，在 `type_gen.rs` 中实现代码生成入口。
2. **生成层**：运行 `cargo test --features specta` 触发生成，输出 `src/bindings.ts`。
3. **前端侧**：将所有 `invoke<T>('command_name', args)` 替换为从 `@/bindings` 导入的类型安全函数。

---

## 架构

```
src-tauri/
├── Cargo.toml                  # 新增 specta feature + 可选依赖
├── src/
│   ├── lib.rs                  # 不变（生产路径）
│   ├── type_gen.rs             # 新增：#[cfg(feature = "specta")] 代码生成入口
│   └── commands/
│       ├── search.rs           # SearchResult + #[cfg_attr(feature="specta", derive(specta::Type))]
│       ├── clipboard.rs        # ClipboardItem + derive
│       ├── translate.rs        # TranslateResult + derive
│       ├── ip.rs               # IpInfo + derive
│       └── screenshot.rs       # ScreenshotData + derive

src/
├── bindings.ts                 # 自动生成，提交到 git
├── utils/tauri.ts              # 删除 TauriSearchResult，toSearchResults 改用 bindings 类型
└── modules/
    ├── search-files/index.ts   # 迁移
    ├── search-apps/index.ts    # 迁移
    ├── clipboard/index.ts      # 迁移
    ├── translate/index.ts      # 迁移（删除本地 TranslateResult）
    └── ip/index.ts             # 迁移
```

---

## 组件详解

### 1. Cargo.toml 变更

```toml
[features]
# 仅用于开发时生成 TypeScript bindings，生产构建不启用
specta = ["dep:specta", "dep:specta-typescript", "dep:tauri-specta"]
webkit_tuning_mock = ["mockall"]

[dependencies]
# specta 可选依赖（仅 specta feature 启用时编译）
specta            = { version = "0.1", optional = true }
specta-typescript = { version = "0.1", optional = true }
tauri-specta      = { version = "2", features = ["derive", "typescript"], optional = true }
```

> **注意**：`tauri-specta` v2 支持 Tauri 2，需确认 crates.io 上的最新兼容版本。

### 2. 结构体 derive 标注

对每个需要导出的结构体，在现有 `#[derive(...)]` 行上追加条件 derive：

```rust
// 示例：commands/search.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub path: String,
    pub kind: String,
    pub icon: Option<String>,
    pub last_used: Option<String>,
    pub score: Option<i32>,
}
```

五个结构体均采用相同模式：`SearchResult`、`ClipboardItem`、`TranslateResult`、`IpInfo`、`ScreenshotData`。

### 3. type_gen.rs（代码生成入口）

```rust
// src-tauri/src/type_gen.rs
// 仅在 specta feature 启用时编译
#![cfg(feature = "specta")]

use specta_typescript::Typescript;
use tauri_specta::{collect_commands, Builder};

/// 生成 TypeScript bindings 并写入 src/bindings.ts
/// 运行方式：cargo test --features specta export_bindings -- --nocapture
#[test]
pub fn export_bindings() {
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            crate::commands::search::search_files,
            crate::commands::search::search_apps,
            crate::commands::search::score_items,
            crate::commands::clipboard::get_clipboard_history,
            crate::commands::translate::translate_youdao,
            crate::commands::translate::translate_ai,
            crate::commands::translate::get_selected_text,
            crate::commands::ip::fetch_ip_info,
            crate::commands::shortcut::is_app_active,
            crate::commands::shortcut::get_selected_text_cached,
            crate::commands::screenshot::ocr_image,
        ]);

    // 输出路径：相对于 src-tauri 目录，向上一级到 workspace 根，再进入 src/
    let out_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../src/bindings.ts");

    builder
        .export(Typescript::default(), &out_path)
        .expect("生成 TypeScript bindings 失败");

    println!("✅ bindings 已生成：{}", out_path.canonicalize().unwrap().display());
}
```

### 4. lib.rs 集成 type_gen 模块

```rust
// src-tauri/src/lib.rs 顶部追加
#[cfg(feature = "specta")]
mod type_gen;
```

### 5. 生成的 bindings.ts 结构（预期）

```typescript
// 此文件由 tauri-specta 自动生成，请勿手动修改。
// 重新生成：bun run gen:bindings

import { invoke } from "@tauri-apps/api/core";

// ---- 类型定义 ----

export type SearchResult = {
  id: string;
  title: string;
  path: string;
  kind: string;
  icon: string | null;
  last_used: string | null;
  score: number | null;
};

export type ClipboardItem = {
  id: string;
  content: string;
  content_type: string;
  source_app: string;
  created_at: string;
  is_favorite: boolean;
  score: number;
};

export type TranslateResult = {
  source: string;
  translation: string;
  engine: string;
};

export type IpInfo = {
  ip: string | null;
  success: boolean | null;
  message: string | null;
  country: string | null;
  region: string | null;
  city: string | null;
  isp: string | null;
  org: string | null;
  asn: string | null;
};

export type ScreenshotData = {
  data_url: string;
  width: number;
  height: number;
  scale: number;
};

// ---- 命令调用函数 ----

export const commands = {
  async searchFiles(query: string): Promise<SearchResult[]> {
    return await invoke("search_files", { query });
  },
  async searchApps(query: string): Promise<SearchResult[]> {
    return await invoke("search_apps", { query });
  },
  async getClipboardHistory(
    query: string | null,
    filterFavorite: boolean | null,
    limit: number | null
  ): Promise<ClipboardItem[]> {
    return await invoke("get_clipboard_history", { query, filterFavorite, limit });
  },
  async translateYoudao(
    text: string,
    appKey: string,
    appSecret: string,
    targetLang: string | null
  ): Promise<TranslateResult> {
    return await invoke("translate_youdao", { text, appKey, appSecret, targetLang });
  },
  async translateAi(
    text: string,
    endpoint: string,
    apiKey: string,
    model: string,
    targetLang: string | null,
    prompt: string | null
  ): Promise<TranslateResult> {
    return await invoke("translate_ai", { text, endpoint, apiKey, model, targetLang, prompt });
  },
  async fetchIpInfo(ip: string | null): Promise<IpInfo> {
    return await invoke("fetch_ip_info", { ip });
  },
  async scoreItems(query: string, items: string[]): Promise<number[]> {
    return await invoke("score_items", { query, items });
  },
  async isAppActive(): Promise<boolean> {
    return await invoke("is_app_active");
  },
  async getSelectedTextCached(): Promise<string> {
    return await invoke("get_selected_text_cached");
  },
  async getSelectedText(): Promise<string> {
    return await invoke("get_selected_text");
  },
  async ocrImage(imageData: string): Promise<string> {
    return await invoke("ocr_image", { imageData });
  },
};
```

> **实际输出**由 tauri-specta 生成，上述为预期结构示意。实际字段名和函数签名以生成结果为准。

### 6. 前端迁移模式

**迁移前**（以 search-files 为例）：
```typescript
import { invoke } from '@tauri-apps/api/core'
import { isTauri, toSearchResults, type TauriSearchResult } from '@/utils/tauri'

const files = await invoke<TauriSearchResult[]>('search_files', { query })
```

**迁移后**：
```typescript
import { commands, type SearchResult } from '@/bindings'
import { isTauri, toSearchResults } from '@/utils/tauri'

const files = await commands.searchFiles(query)
// files 的类型自动推断为 SearchResult[]
```

**utils/tauri.ts 变更**：
- 删除 `TauriSearchResult` 接口
- `toSearchResults` 函数参数类型改为从 `@/bindings` 导入的 `SearchResult`

**translate/index.ts 变更**：
- 删除本地 `TranslateResult` 接口
- 从 `@/bindings` 导入 `TranslateResult` 类型
- 将 `invoke<TranslateResult>('translate_youdao', ...)` 替换为 `commands.translateYoudao(...)`
- 将 `invoke<TranslateResult>('translate_ai', ...)` 替换为 `commands.translateAi(...)`

### 7. package.json 脚本

```json
{
  "scripts": {
    "gen:bindings": "cd src-tauri && cargo test --features specta export_bindings -- --nocapture"
  }
}
```

---

## 数据流

```
开发时：
  bun run gen:bindings
    → cargo test --features specta
    → type_gen.rs::export_bindings()
    → tauri_specta::Builder::export()
    → src/bindings.ts（写入）

运行时（前端）：
  import { commands } from '@/bindings'
  commands.searchFiles(query)
    → invoke('search_files', { query })
    → Rust: commands::search::search_files()
    → Result<Vec<SearchResult>, String>
    → Promise<SearchResult[]>（前端）
```

---

## 错误处理

- Rust 命令返回 `Result<T, String>`，tauri-specta 生成的函数在 TypeScript 侧返回 `Promise<T>`，错误通过 Promise rejection 传递，与现有 `invoke` 行为一致。
- 生成失败（如路径不存在）时，`export_bindings` 测试 panic，cargo test 报错，开发者可立即感知。
- 前端调用失败时，错误处理逻辑与迁移前相同（`try/catch` 或 `.catch()`）。

---

## 约束与注意事项

1. **生产构建零开销**：`specta` feature 默认不启用，`type_gen.rs` 整个文件在生产构建中不编译。
2. **字段命名**：tauri-specta 默认保持 snake_case，与现有前端代码一致，无需 `#[serde(rename_all = "camelCase")]`。
3. **`score` 字段类型差异**：`ClipboardItem.score` 在 Rust 侧为 `i32`，生成的 TS 类型为 `number`，与现有前端接口一致。
4. **流式命令排除**：`translate_ai_stream`、`chat_stream` 等流式命令不纳入 bindings（返回类型为 `()`，无类型安全价值），继续使用原始 `invoke`。
5. **`enter_screenshot_mode` 排除**：该命令接受 `ScreenshotData` 参数，但前端通过事件接收截图数据，不通过 bindings 调用，暂不纳入。

---

## 正确性属性

*属性是在系统所有有效执行中都应成立的特征或行为——本质上是关于系统应做什么的形式化陈述。属性是人类可读规范与机器可验证正确性保证之间的桥梁。*

### 属性 1：所有指定命令在 bindings 中均有对应导出函数

对于需求 3.2 中列出的每个命令名称，生成的 `src/bindings.ts` 文件中都应存在对应的导出函数（通过函数名或 `commands` 对象属性可访问）。

**Validates: Requirements 3.2, 4.2**

### 属性 2：所有指定结构体在 bindings 中均有对应类型导出

对于 `SearchResult`、`ClipboardItem`、`TranslateResult`、`IpInfo`、`ScreenshotData` 中的每一个，生成的 `src/bindings.ts` 文件中都应存在对应的 TypeScript 类型定义（`type` 或 `interface`）。

**Validates: Requirements 4.1**

### 属性 3：迁移后所有模块文件不包含手写泛型 invoke 断言

对于需求 5.1–5.5 中列出的每个前端模块文件，迁移完成后文件内容中不应包含 `invoke<TauriSearchResult` 或 `invoke<TranslateResult` 或 `invoke<ClipboardItem` 或 `invoke<{ success` 等手写泛型断言模式。

**Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5**
