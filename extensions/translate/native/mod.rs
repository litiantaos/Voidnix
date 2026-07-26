pub mod ai_translate;
mod lang_utils;
pub mod youdao;

use crate::runtime::registry::Extension;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

/// 划词翻译快捷键触发时捕获的选中文本缓存。
static SELECTED_TEXT: Mutex<String> = Mutex::new(String::new());

#[tauri::command]
pub fn get_selected_text_cached() -> String {
    SELECTED_TEXT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslateResult {
    pub source: String,
    pub translation: String,
    pub engine: String,
}

#[tauri::command]
pub async fn get_selected_text() -> Result<String, String> {
    use std::time::Instant;

    // 优先读当前剪贴板（划词翻译快捷键触发前，cmd+c 已写入）
    if let Some(text) = crate::platform::pasteboard::read_text() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    // 轮询等待 cmd+c 写入（最多 300ms）
    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(20)).await;

        if let Some(text) = crate::platform::pasteboard::read_text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        if start.elapsed() > Duration::from_millis(300) {
            break;
        }
    }

    Ok(String::new())
}

/// Translate 扩展。
pub struct TranslateExtension;

#[async_trait::async_trait]
impl Extension for TranslateExtension {
    fn id(&self) -> &'static str {
        "translate"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        #[cfg(target_os = "macos")]
        {
            use tauri::Emitter;
            crate::runtime::shortcut::register_shortcut_hook(
                "translate",
                Box::new(|app, ctx| {
                    if ctx.window_hidden {
                        if let Ok(mut selected) = crate::extensions::translate::SELECTED_TEXT.lock()
                        {
                            *selected = String::new();
                        }

                        let self_pid = std::process::id() as i32;
                        let target_pid = ctx.front_pid.filter(|&p| p != self_pid);

                        let ax_text = crate::platform::selection::try_ax();
                        let snap = crate::platform::pasteboard::snapshot();
                        if ax_text.is_none() {
                            if let Some(pid) = target_pid {
                                crate::platform::input::post_combo("cmd+c", Some(pid));
                            }
                        }

                        // show 由前端 makeToggleHandler 控制（与其他扩展同路径）：
                        // setActiveExtension → rAF → showWindow，确保窗口渲染第一帧已是 translate 视图，
                        // 避免 Rust 立即 show 时 webview 仍为旧视图的闪现。
                        // 读文本（AX / cmd+c / poll）不依赖窗口可见状态。

                        let app_clone = app.clone();
                        std::thread::spawn(move || {
                            let text = if let Some(t) = ax_text {
                                t
                            } else {
                                crate::platform::selection::poll_clipboard(snap)
                            };
                            if let Ok(mut selected) =
                                crate::extensions::translate::SELECTED_TEXT.lock()
                            {
                                *selected = text.clone();
                            }
                            let _ = app_clone.emit("translate-text-ready", text);
                        });
                        return true;
                    }
                    let _ = app.emit("translate-text-ready", "");
                    false
                }),
            );
        }
        Ok(())
    }
}
