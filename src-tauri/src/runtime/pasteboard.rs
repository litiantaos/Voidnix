//! 框架命令薄壳：pasteboard_write_text。原语在 platform::pasteboard。

/// 前端 invoke('pasteboard_write_text')。替代 tauri-plugin-clipboard-manager。
/// 带 source marker：clipboard monitor 据此标 source_app = "Voidnix"（仍入库，
/// 区别于防回环 marker com.litiantao.voidnix.clipboard）。
#[tauri::command]
pub fn pasteboard_write_text(text: String) {
    use crate::platform::pasteboard;
    pasteboard::clear();
    pasteboard::set_string(&text);
    pasteboard::set_custom("", "com.litiantao.voidnix.source");
}
