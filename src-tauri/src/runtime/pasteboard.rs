//! 框架命令薄壳：pasteboard_write_text。原语在 platform::pasteboard。

/// 前端 invoke('pasteboard_write_text')。替代 tauri-plugin-clipboard-manager。
#[tauri::command]
pub fn pasteboard_write_text(text: String) {
    crate::platform::pasteboard::write_text(&text);
}
