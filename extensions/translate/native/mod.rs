pub mod ai_translate;
pub mod youdao;
mod lang_utils;

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TranslateResult {
    pub source: String,
    pub translation: String,
    pub engine: String,
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn get_selected_text() -> Result<String, String> {
    use std::time::Instant;
    use tokio::process::Command;

    let text = Command::new("pbpaste")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if !text.trim().is_empty() {
        return Ok(text.trim().to_string());
    }

    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(20)).await;

        let text = Command::new("pbpaste")
            .output()
            .await
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        if !text.trim().is_empty() {
            return Ok(text.trim().to_string());
        }

        if start.elapsed() > Duration::from_millis(300) {
            break;
        }
    }

    Ok(String::new())
}

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("translate")
        .setup(|_app, _api| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                crate::core::shortcut::register_shortcut_hook("translate", Box::new(|app, ctx| {
                    if ctx.window_hidden {
                        if let Ok(mut selected) = crate::core::shortcut::SELECTED_TEXT.lock() {
                            *selected = String::new();
                        }

                        let self_pid = std::process::id() as i32;
                        let target_pid = ctx.front_pid.filter(|&p| p != self_pid);

                        let ax_text = crate::macos::text_selection::try_ax();
                        let snap = crate::macos::text_selection::snapshot_clipboard();
                        if ax_text.is_none() {
                            if let Some(pid) = target_pid {
                                crate::macos::text_selection::inject_copy(pid);
                            }
                        }

                        crate::macos::webkit_tuning::show_main(app);

                        let app_clone = app.clone();
                        std::thread::spawn(move || {
                            let text = if let Some(t) = ax_text {
                                t
                            } else {
                                crate::macos::text_selection::poll_clipboard(snap)
                            };
                            if let Ok(mut selected) = crate::core::shortcut::SELECTED_TEXT.lock() {
                                *selected = text.clone();
                            }
                            let _ = app_clone.emit("translate-text-ready", text);
                        });
                        return true;
                    }
                    let _ = app.emit("translate-text-ready", "");
                    false
                }));
            }
            Ok(())
        })
        .build()
}
