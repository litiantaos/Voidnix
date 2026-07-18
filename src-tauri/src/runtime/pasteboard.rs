//! 框架命令薄壳：剪贴板写 / 写并粘贴。原语在 platform::pasteboard。

use tauri::AppHandle;

/// 前端 invoke('pasteboard_write_text')。替代 tauri-plugin-clipboard-manager。
/// 带 source marker：clipboard monitor 据此标 source_app = "Voidnix"（仍入库，
/// 区别于防回环 marker com.litiantao.voidnix.clipboard）。
#[tauri::command]
pub fn pasteboard_write_text(text: String) {
    write_text_marked(&text);
}

/// 写入剪贴板 → 隐藏主窗 → 注入 Cmd+V（与 clipboard 扩展粘贴路径同范式）。
/// 需辅助功能权限；用于 AI 提供商等「分字段粘贴到前台 App」。
#[tauri::command]
pub fn pasteboard_paste_text(text: String, app: AppHandle) -> Result<(), String> {
    if !ax_trusted() {
        return Err("需授予辅助功能权限".into());
    }
    write_text_marked(&text);
    crate::runtime::window::hide_main(&app);
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(200));
        crate::platform::input::post_combo("cmd+v", None);
    });
    Ok(())
}

fn write_text_marked(text: &str) {
    use crate::platform::pasteboard;
    pasteboard::clear();
    pasteboard::set_string(text);
    pasteboard::set_custom("", "com.litiantao.voidnix.source");
}

fn ax_trusted() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    // SAFETY: 仅查询当前进程 Accessibility 可信状态
    unsafe { AXIsProcessTrusted() }
}
