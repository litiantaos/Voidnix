mod core;
mod infra;
mod macos;
#[cfg(feature = "specta")]
mod type_gen;
mod extensions;

use tauri::Manager;
use infra::db::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            crate::macos::webkit_tuning::show_main(app);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init());

    let builder = extensions::configure_app!(builder);

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "macos")]
            crate::macos::text_selection::init_ax_timeout();

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
                macos::panel::convert_to_panel(raw.cast());
                macos::webkit_tuning::install(&window)?;
                macos::webkit_tuning::intercept_cmd_backspace(app.handle());
            }

            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("screenshot") {
                use objc2_app_kit::{NSScreen, NSWindow as ScreenshotNSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior};
                use objc2_foundation::MainThreadMarker;
                crate::extensions::screenshot::install_background_layer(&window);
                let raw = window.ns_window().unwrap().cast::<ScreenshotNSWindow>();
                unsafe {
                    let ns_window = raw.as_ref().unwrap();
                    ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
                    ns_window.setLevel(objc2_app_kit::NSStatusWindowLevel);
                    let _: () = objc2::msg_send![ns_window, setAcceptsMouseMovedEvents: true];
                    let behavior = NSWindowCollectionBehavior::FullScreenAuxiliary
                        | NSWindowCollectionBehavior::Transient
                        | NSWindowCollectionBehavior::IgnoresCycle;
                    ns_window.setCollectionBehavior(behavior);
                    let mtm = MainThreadMarker::new().unwrap();
                    let screen = NSScreen::mainScreen(mtm).unwrap();
                    ns_window.setFrame_display(screen.frame(), true);
                    ns_window.setAlphaValue(0.0);
                    let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
                    ns_window.orderFrontRegardless();
                }
                macos::webkit_tuning::install_screenshot(&window)?;
            }

            #[cfg(target_os = "macos")]
            {
                use objc2_foundation::{NSNotificationCenter, NSNotificationName, NSString};
                use objc2::rc::Retained;
                use std::sync::OnceLock;

                static SCREENSHOT_REACTIVATE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
                let event_app = app.handle().clone();
                let _ = SCREENSHOT_REACTIVATE.set(Box::new(move || {
                    crate::extensions::screenshot::reactivate_screenshot_window(&event_app);
                }));

                let center = NSNotificationCenter::defaultCenter();
                let name: Retained<NSString> = NSString::from_str("NSApplicationDidBecomeActiveNotification");
                let name_ref: &NSNotificationName = unsafe { std::mem::transmute::<&NSString, &NSNotificationName>(&name) };

                unsafe {
                    center.addObserverForName_object_queue_usingBlock(
                        Some(name_ref),
                        None,
                        None,
                        &block2::RcBlock::new(|_notification| {
                            if let Some(cb) = SCREENSHOT_REACTIVATE.get() {
                                cb();
                            }
                        }),
                    );
                }

                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = app_handle.run_on_main_thread(|| {
                        crate::extensions::screenshot::prewarm_jpeg_encoder();
                    });
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}