// 此模块仅在 specta feature 启用时编译，生产构建不包含任何此处代码。
// 运行方式：cargo test --features specta export_bindings -- --nocapture
#![cfg(feature = "specta")]

use specta_typescript::Typescript;
use tauri_specta::{collect_commands, Builder, ErrorHandlingMode};

/// 生成 TypeScript bindings 并写入 src/bindings.ts。
///
/// 执行命令：
///   cd src-tauri && cargo test --features specta export_bindings -- --nocapture
#[test]
pub fn export_bindings() {
    let builder = Builder::<tauri::Wry>::new()
        // 使用 Throw 模式：Result<T, E> 命令在 TS 侧直接 throw，与原有 invoke 行为一致
        .error_handling(ErrorHandlingMode::Throw)
        .commands(collect_commands![
            crate::extensions::search::search_files,
            crate::extensions::search::search_apps,
            crate::extensions::search::score_items,
            crate::extensions::clipboard::get_clipboard_history,
            crate::extensions::translate::translate_youdao,
            crate::extensions::translate::translate_ai,
            crate::extensions::translate::get_selected_text,
            crate::extensions::ip::fetch_ip_info,
            crate::extensions::shortcut::is_app_active,
            crate::extensions::shortcut::get_selected_text_cached,
            crate::extensions::screenshot::ocr_image,
        ]);

    // 输出路径：相对于 src-tauri 目录，向上一级到 workspace 根，再进入 src/
    let out_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");

    builder
        .export(Typescript::default(), &out_path)
        .expect("生成 TypeScript bindings 失败");

    println!(
        "✅ bindings 已生成：{}",
        out_path.canonicalize().unwrap().display()
    );
}
