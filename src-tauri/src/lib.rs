mod commands;
mod db;
mod clipboard_monitor;
mod http;
mod mac_utils;
#[cfg(target_os = "macos")]
mod skylight;
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
            commands::screenshot::show_screenshot_window,
            commands::window::set_main_window_size,
            commands::window::get_home_dir,
            commands::window::pick_directory,
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
                    // Status 级别覆盖 Dock（~20）和菜单栏（24），但不像 PopUpMenu（101）
                    // 那样被系统当作全局菜单跟随用户切换 Space。
                    ns_window.setLevel(objc2_app_kit::NSStatusWindowLevel as isize);
                    let _: () = objc2::msg_send![ns_window, setAcceptsMouseMovedEvents: true];
                    // FullScreenAuxiliary：允许覆盖全屏应用。
                    // Transient：窗口不出现在 Mission Control / Exposé 中。
                    // IgnoresCycle：不参与 Cmd+~ 窗口切换。
                    // 不设 CanJoinAllSpaces——那会让窗口同时显示在所有桌面，
                    // 违背"截屏窗口只属于触发桌面"的语义。
                    let behavior = NSWindowCollectionBehavior::FullScreenAuxiliary
                        | NSWindowCollectionBehavior::Transient
                        | NSWindowCollectionBehavior::IgnoresCycle;
                    ns_window.setCollectionBehavior(behavior);
                    let mtm = MainThreadMarker::new().unwrap();
                    let screen = NSScreen::mainScreen(mtm).unwrap();
                    ns_window.setFrame_display(screen.frame(), true);
                    // 关键：立刻 orderFrontRegardless 让窗口处于 ordered 状态。
                    // 配合 throttling::install 的 windowOcclusionDetectionEnabled=NO，
                    // WKWebView 始终保持在渲染管线中，避免空桌面"冷启动"延迟。
                    // alpha=0 + ignoresMouseEvents 让窗口在视觉/交互上完全隐身。
                    // 平时窗口 ordered 在启动 Space，触发截屏时由 SkyLight 强制迁移
                    // 到当前 active Space（瞬时操作，不依赖系统动画）。
                    ns_window.setAlphaValue(0.0);
                    let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
                    ns_window.orderFrontRegardless();
                }
                // 安装 Throttling_Suppressor：关 occlusion detection + Transient flag，
                // 让 WKWebView 在 orderOut 期间仍保持渲染管线活跃，
                // 避免空桌面截屏时因 Space 迁移引起的卡顿。
                webkit_tuning::install_screenshot(&window)?;
                // 安装 CALayer 背景层：工业级零编码直贴 CGImage 的基础设施。
                commands::screenshot::install_background_layer(&window);
            }

            clipboard_monitor::start_monitor(app.handle().clone());
            commands::search::set_app_handle(app.handle().clone());
            commands::search::init_app_watcher();
            #[cfg(target_os = "macos")]
            commands::finder_ext::init_finder_ext(app.handle().clone());

            // 监听应用激活通知：从 Mission Control 返回时重新激活截屏窗口的鼠标追踪
            #[cfg(target_os = "macos")]
            {
                use objc2_foundation::{NSNotificationCenter, NSNotificationName, NSString};
                use objc2::rc::Retained;
                use std::sync::OnceLock;

                // 用 OnceLock 持有 app handle，供 block 内使用
                static SCREENSHOT_APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
                let _ = SCREENSHOT_APP_HANDLE.set(app.handle().clone());

                let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
                let center = NSNotificationCenter::defaultCenter();
                let name: Retained<NSString> = NSString::from_str("NSApplicationDidBecomeActiveNotification");
                let name_ref: &NSNotificationName = unsafe { std::mem::transmute::<&NSString, &NSNotificationName>(&name) };

                unsafe {
                    center.addObserverForName_object_queue_usingBlock(
                        Some(name_ref),
                        None,
                        None,
                        &block2::RcBlock::new(|_notification| {
                            if let Some(handle) = SCREENSHOT_APP_HANDLE.get() {
                                commands::screenshot::reactivate_screenshot_window(handle);
                            }
                        }),
                    );
                }
                let _ = mtm; // suppress unused warning
            }

            tauri::async_runtime::spawn(commands::search::prewarm_cache());

            // 截屏 JPEG 编码器预热：异步在主线程上做一次 1×1 编码，
            // 让 NSBitmapImageRep / Image I/O 完成 lazy 加载，真正第一次截屏不付出
            // 首次代价（实测 ~120ms 降到稳定的 30-50ms）。
            #[cfg(target_os = "macos")]
            {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    // 延迟 500ms 等启动主路径稳定，再做预热
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = app_handle.run_on_main_thread(|| {
                        commands::screenshot::prewarm_jpeg_encoder();
                    });
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}
