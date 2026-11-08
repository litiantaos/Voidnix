//! 框架命令薄壳：剪贴板写 / 写并粘贴。原语在 platform::pasteboard。

use serde::Serialize;
use tauri::AppHandle;

/// 粘贴执行的实际结果：目标无文本输入区时文本回退复制（跳过 Cmd+V 模拟），
/// 前端按此 toast 分流（已粘贴 / 已复制）。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PasteOutcome {
    Pasted,
    Copied,
}

/// 文本粘贴目标输入区判定：hide_main 的 restore_captured 会把焦点交还
/// captured_pid 的 app，Cmd+V 落在其聚焦元素上。
///
/// 三态取向：输入区 → 自动粘贴；**明确非输入区**（列表/表格/网页正文，以及
/// 未知容器中的空容器或子元素全为图标/列表/网页内容类——桌面、网页内容宿主）→
/// 回退复制，避免在非输入目标注入 Cmd+V 制造副作用（实测桌面凭空生成文件）；
/// **无法判定**（自绘引擎容器只有标题栏按钮组等控件、无聚焦元素、AX 无响应）→
/// 维持粘贴，自绘编辑器（Zed 等）优先自动粘贴，代价是非输入目标可能空粘一次
/// （内容已写板，toast 提示略偏但不丢数据）。
pub(crate) fn paste_target_is_input_area() -> bool {
    crate::platform::selection::focused_element_editable(crate::platform::focus::captured_pid())
        .unwrap_or(true)
}

/// 前端 invoke('pasteboard_write_text')。替代 tauri-plugin-clipboard-manager。
/// 带 source marker：clipboard monitor 据此标 source_app = "Voidnix"（仍入库，
/// 区别于防回环 marker com.litiantao.voidnix.clipboard）。
#[tauri::command]
pub fn pasteboard_write_text(text: String) {
    write_text_marked(&text);
}

/// 写入剪贴板 → 隐藏主窗 → 目标为文本输入区时注入 Cmd+V，否则回退纯复制
/// （与 clipboard 扩展粘贴路径同范式，共用 `paste_target_is_input_area` 判定）。
/// 需设备控制权限；用于 AI 提供商等「分字段粘贴到前台 App」。
#[tauri::command]
pub fn pasteboard_paste_text(text: String, app: AppHandle) -> Result<PasteOutcome, String> {
    if !ax_trusted() {
        return Err("需授予设备控制权限".into());
    }
    write_text_marked(&text);
    // 判定必须在 hide_main 之前：restore_captured 会 swap(0) 消费掉 captured_pid，
    // 之后查询必失败（unwrap_or(true) 退化成恒粘贴）。clipboard 扩展路径同理先判后藏。
    let paste = paste_target_is_input_area();
    // 与 clipboard 扩展粘贴路径同范式：直接 hide_main（不经 hide_window 命令），
    // 内存兜底须在此对齐
    crate::runtime::window::hide_main(&app);
    crate::runtime::window::maybe_reload_webview(&app);
    if paste {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            crate::platform::input::post_combo("cmd+v", None);
        });
        Ok(PasteOutcome::Pasted)
    } else {
        Ok(PasteOutcome::Copied)
    }
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
