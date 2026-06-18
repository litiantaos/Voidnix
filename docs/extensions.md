# 扩展开发

所有扩展同构，目录结构统一。是否含 `native/` 子目录区分实现方式，不构成分类。

## 目录结构

```
extensions/<id>/
├── index.ts               # 前端注册（registerModule）
├── View.vue               # 主视图（若需要）
├── Settings.vue           # 设置面板（若需要）
├── *.test.ts              # 测试（co-location）
└── native/                # Rust 后端（仅需要系统级能力时）
    ├── mod.rs             # Extension trait + 命令 + pub fn init()
    └── ...                # 子模块
```

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
- `runtime::storage::SETTINGS_STORE_PATH`：全局配置路径
- `platform::focus`：焦点管理（capture_frontmost / restore_captured / captured_pid）
- `platform::input`：键盘注入（post_key / inject_copy / paste_global）
- `platform::pasteboard`：剪贴板操作（read_text / snapshot / restore / change_count）
- `platform::selection`：AX 选中文本提取（try_ax / poll_clipboard）
- `platform::path_guard`：路径安全校验
- `http::client()`：全局 HTTP 客户端
- `runtime::llm`：LLM 基础设施（stream_openai_request / validate_ai_request / LlmMessage）

## 纯 TS 扩展（无 native/）

前端注册即可，无需 Rust 代码。通过 `@tauri-apps/api` 直接调命令。

## 搜索集成

扩展通过 `AppModule` 接口参与搜索：

- `onSearch(query)`：全局搜索聚合（并行调用所有模块，3s 超时）
- `onModuleSearch(query)`：模块激活时的本地搜索
- `searchItems()`：半静态声明，框架自动跑 `scoreFields` 模糊匹配

排序权重（`src/utils/fuzzy.ts` + `src/core/module-registry.ts`）：模块(+500) > 应用(+300) > 文件夹(+80) > 文件
