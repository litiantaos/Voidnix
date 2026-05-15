mod commands;
mod db;
mod clipboard_monitor;
mod http;
mod mac_utils;
mod sse;
#[cfg(target_os = "macos")]
mod text_selection;
mod webkit_tuning;
#[cfg(feature = "specta")]
mod type_gen;

use tauri::Manager;
use db::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            commands::clipboard::get_clipboard_history,
            commands::clipboard::clear_clipboard_history,
            commands::clipboard::toggle_clipboard_favorite,
            commands::clipboard::paste_clipboard_item,
            commands::search::search_files,
            commands::search::search_apps,
            commands::search::launch_app,
            commands::search::reveal_in_finder,
            commands::search::get_recent_apps,
            commands::search::score_items,
            commands::shortcut::register_global_shortcut,
            commands::shortcut::hide_window,
            commands::shortcut::is_app_active,
            commands::shortcut::get_selected_text_cached,
            commands::ip::fetch_ip_info,
            commands::awake::toggle_awake,
            commands::awake::is_awake_enabled,
            commands::translate::get_selected_text,
            commands::translate::translate_youdao,
            commands::translate::translate_ai,
            commands::translate::translate_ai_stream,
            commands::chat::chat_stream,
            commands::finder_ext::open_extensions_prefs,
            commands::finder_ext::set_finder_ext_enabled,
            commands::finder_ext::quit_app,
            commands::screenshot::capture_screen,
            commands::screenshot::ocr_image,
            commands::screenshot::save_screenshot,
            commands::screenshot::copy_screenshot_to_clipboard,
            commands::screenshot::enter_screenshot_mode,
            commands::screenshot::exit_screenshot_mode,
            commands::window::set_main_window_size,
        ])
        .setup(|app| {
            commands::awake::init(app)?;
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Cap AX messaging timeout process-wide (default is 6s, a single
            // wedged target app would freeze Voidnix's main thread during
            // translate extraction). Must run once before any AX call.
            #[cfg(target_os = "macos")]
            text_selection::init_ax_timeout();

            let app_data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let db_path = app_data_dir.join("launcher.db");
            app.manage(Database::new(db_path));

            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                use objc2_app_kit::NSWindow;
                let raw = window.ns_window().unwrap().cast::<NSWindow>();
                let ns_window = unsafe { raw.as_ref().unwrap() };
                if let Some(content_view) = ns_window.contentView() {
                    let _: () = unsafe { objc2::msg_send![&content_view, setWantsLayer: true] };
                    let layer: *mut objc2::runtime::AnyObject = unsafe { objc2::msg_send![&content_view, layer] };
                    if !layer.is_null() {
                        let _: () = unsafe { objc2::msg_send![layer, setCornerRadius: 16.0_f64] };
                        let _: () = unsafe { objc2::msg_send![layer, setMasksToBounds: true] };
                    }
                }
                // webkit_tuning 驯化组件安装（在 contentView 圆角设置之后）
                webkit_tuning::install(&window)?;
            }

            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("screenshot") {
                use objc2_app_kit::{NSScreen, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior};
                use objc2_foundation::MainThreadMarker;
                let raw = window.ns_window().unwrap().cast::<NSWindow>();
                unsafe {
                    let ns_window = raw.as_ref().unwrap();
                    ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
                    ns_window.setLevel(objc2_app_kit::NSScreenSaverWindowLevel as isize);
                    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                        | NSWindowCollectionBehavior::FullScreenAuxiliary
                        | NSWindowCollectionBehavior::Stationary;
                    ns_window.setCollectionBehavior(behavior);
                    let mtm = MainThreadMarker::new().unwrap();
                    let screen = NSScreen::mainScreen(mtm).unwrap();
                    // display=true：让 contentView/WKWebView 立即随窗口 resize，
                    // 避免 viewport 与 NSWindow 尺寸不同步导致首次截屏遮罩留缝隙
                    ns_window.setFrame_display(screen.frame(), true);
                    // alpha=0 + ignoresMouseEvents：窗口不可见、不拦截点击，但 JS 持续运行
                    ns_window.setAlphaValue(0.0);
                    let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
                    ns_window.orderFrontRegardless();
                }
            }

            clipboard_monitor::start_monitor(app.handle().clone());
            commands::search::set_app_handle(app.handle().clone());
            commands::search::init_app_watcher();
            #[cfg(target_os = "macos")]
            commands::finder_ext::init_finder_ext(app.handle().clone());

            tauri::async_runtime::spawn(commands::search::prewarm_cache());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}
