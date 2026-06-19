# 扩展开发

所有扩展同构，目录结构统一。是否含 `native/` 子目录区分实现方式，不构成分类。

## 目录结构

```
extensions/<id>/
├── index.ts               # 前端注册（registerModule）
├── config.ts              # defineConfig 自管配置（可选）
├── View.vue               # 主视图（若需要）
├── Settings.vue           # 设置面板（若需要）
├── logic.ts               # 纯逻辑提取（可选，便于测试）
├── *.test.ts              # 测试（co-location）
└── native/                # Rust 后端（仅需要系统级能力时）
    ├── mod.rs             # Extension trait + 命令 + pub fn init()
    └── ...                # 子模块
```

## 扩展配置（defineConfig）

每个扩展通过 `config.ts` 自管配置，自动持久化至 `extensions/<id>/config.json`：

```typescript
// extensions/clipboard/config.ts
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('clipboard', {
  maxDays: 30,
  enabled: true,
})

// 使用：响应式读写，变更自动持久化（300ms 防抖）
config.maxDays // → 30
config.maxDays = 60 // 自动写盘
```

含 Rust 命令同步的配置（如开关类）：

```typescript
// extensions/finder-ext/config.ts
import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineConfig } from '@/runtime/storage'

export const config = defineConfig('finder-ext', { enabled: false })

watch(
  () => config.enabled,
  (enabled) => {
    invoke('set_finder_ext_enabled', { enabled }).catch(() => {})
  },
)
```

框架级配置（全局快捷键、AI Provider）在 `stores/settings.ts`，不在此系统。

## Rust 扩展（含 native/）

### 双注册

1. **编译期**：`#[tauri::command]` 函数 + `pub fn init() -> TauriPlugin`（sync-extensions 自动扫描，生成 `extensions.rs` 的 `configure_app!` 宏）
2. **运行时**：`Extension` trait（`runtime/registry.rs`），在 `lib.rs` 的 `ExtensionRegistry` 注册，提供 `on_setup`/`on_teardown` 生命周期钩子

### Extension trait

```rust
impl Extension for Plugin {
    fn id(&self) -> &'static str { "clipboard" }

    fn on_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // 启动监听器、初始化数据库、预热缓存等
        Ok(())
    }
}
```

### 新增命令

1. 在 `native/` 下声明 `#[tauri::command]`
2. 运行 `bun run sync:extensions` 自动注册到 `configure_app!`

### 框架能力

- `runtime::window::show_main` / `hide_main`：主窗口控制
- `runtime::shortcut::register_shortcut_hook`：快捷键钩子
- `runtime::storage`：TempHandle 临时文件管理（register_temp / cleanup_all_temps / cleanup_temps_by_prefix）
- `platform::focus`：焦点管理（capture_frontmost / restore_captured / captured_pid）
- `platform::input`：键盘注入（post_key / inject_copy / paste_global / post_key_global）
- `platform::pasteboard`：NSPasteboard 统一（read_text / string_for_type / data_for_type / has_type / change_count / snapshot / restore）
- `platform::selection`：AX 选中文本提取（try_ax / poll_clipboard / init_ax_timeout）
- `platform::path_guard`：路径安全校验
- `http::client()`：全局 HTTP 客户端
- `runtime::llm`：LLM 基础设施（stream_openai_request / validate_ai_request / LlmMessage / trim_conversation）

## 纯 TS 扩展（无 native/）

前端注册即可，无需 Rust 代码。通过 `fetch()` 或 `@tauri-apps/api` 直接调命令。

现有纯 TS 扩展：calculator、settings、ip、base64、time、uuid、currency

## 搜索集成

扩展通过 `AppModule` 接口参与搜索（5 组合接口按需实现）：

- `onSearch(query)`：全局搜索聚合（并行调用所有模块，3s 超时）
- `onModuleSearch(query)`：模块激活时的本地搜索
- `searchItems()`：半静态声明，框架自动跑 `scoreFields` 模糊匹配

排序权重（`src/utils/fuzzy.ts` + `src/core/module-registry.ts`）：模块(+500) > 应用(+300) > 文件夹(+80) > 文件

## 测试

纯逻辑提取至 `logic.ts`，测试写在同目录 `logic.test.ts`。vitest 自动扫描 `extensions/**/*.test.ts`。
