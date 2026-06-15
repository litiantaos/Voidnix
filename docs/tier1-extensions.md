# 开发 Tier 1 扩展

内置扩展与主程序同编译同签名。每个扩展自包含：前端 `index.ts` + Vue 组件，Rust 后端 `native/`。

```
extensions/<name>/
├── index.ts              # AppModule 定义 + registerModule()
├── *.vue                 # Vue 组件（defineAsyncComponent 懒加载）
└── native/
    ├── mod.rs            # Tier1Extension 实现 + 模块声明
    └── *.rs              # commands / db / monitor / setup（按需）
```

**注册机制**：

- 前端：`main.ts` glob 自动注册 `@ext/*/index.ts`，无需手动导入
- 后端：`bun run sync:extensions` 扫描 `extensions/*/native/` 和 `src-tauri/src/core/`，生成 `extensions.rs`（`#[path]` 引用源文件不复制，`configure_app!` 宏统一注册命令和插件）
- 连字符自动映射：`finder-ext` → `finder_ext`

**双注册模型**：

- `configure_app!` 宏 → 编译期 API 注册（`#[tauri::command]` + `init() -> TauriPlugin`）
- `Tier1Extension` trait → 运行时生命周期钩子（`on_setup`：窗口初始化、监听器、资源预热、binary 自动部署）

```rust
pub trait Tier1Extension: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn on_setup(&self, _app: &AppHandle) -> tauri::Result<()> { Ok(()) }
}
```

**新增步骤**：`mkdir extensions/my-ext` → 写 `index.ts` + `native/mod.rs` → `bun run sync:extensions` → `bun run tauri:dev`

现有扩展清单见 [AGENTS.md](../AGENTS.md)「开发扩展」。
