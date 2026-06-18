mod http;
mod platform;
mod runtime;
mod extensions;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            crate::runtime::window::show_main(app);
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
            let registry = crate::runtime::registry::ExtensionRegistry::new()
                .register(crate::extensions::clipboard::Plugin)
                .register(crate::extensions::screenshot::Plugin)
                .register(crate::extensions::awake::Plugin)
                .register(crate::extensions::zsh_autosuggestions::Plugin)
                .register(crate::extensions::window_manager::Plugin)
                .register(crate::extensions::finder_ext::Plugin)
                .register(crate::extensions::translate::Plugin)
                .register(crate::extensions::agent::Plugin);
            crate::runtime::registry::bootstrap(app, registry)?;

            // Agent 框架层全局 state
            app.manage(crate::extensions::agent::engine::cancellation::SessionRegistry::default());
            app.manage(crate::extensions::agent::engine::approval::ApprovalManager::default());

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "macos")]
            crate::platform::selection::init_ax_timeout();

            // 主窗口：圆角 + panel 转换
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                use objc2_app_kit::NSWindow;
                let raw = window.ns_window().unwrap().cast::<NSWindow>();
                let ns_window = unsafe { raw.as_ref().unwrap() };
                if let Some(content_view) = ns_window.contentView() {
                    let _: () = unsafe { objc2::msg_send![&content_view, setWantsLayer: true] };
                    let layer: *mut objc2::runtime::AnyObject =
                        unsafe { objc2::msg_send![&content_view, layer] };
                    if !layer.is_null() {
                        let _: () = unsafe { objc2::msg_send![layer, setCornerRadius: 16.0_f64] };
                        let _: () = unsafe { objc2::msg_send![layer, setMasksToBounds: true] };
                    }
                }
                crate::platform::panel::convert_to_panel(raw.cast());
            }

            // snap-panel 窗口：透明覆盖层 + 跨 Space
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("snap-panel") {
                use objc2_app_kit::{NSScreen, NSWindow as SnapNSWindow, NSWindowCollectionBehavior};
                use objc2_foundation::MainThreadMarker;
                let raw = window.ns_window().unwrap().cast::<SnapNSWindow>();
                unsafe {
                    let ns_window = raw.as_ref().unwrap();
                    if let Some(cv) = ns_window.contentView() {
                        let _: () = objc2::msg_send![&cv, setWantsLayer: true];
                    }
                    crate::platform::panel::convert_to_panel(raw.cast());
                    let mtm = MainThreadMarker::new().unwrap();
                    let screen = NSScreen::mainScreen(mtm).unwrap();
                    ns_window.setFrame_display(screen.frame(), true);
                    ns_window.setLevel(objc2_app_kit::NSStatusWindowLevel + 1);
                    let behavior = NSWindowCollectionBehavior::FullScreenAuxiliary
                        | NSWindowCollectionBehavior::Transient
                        | NSWindowCollectionBehavior::CanJoinAllSpaces;
                    ns_window.setCollectionBehavior(behavior);
                    ns_window.setIgnoresMouseEvents(true);
                    let _: () = objc2::msg_send![ns_window, setAcceptsMouseMovedEvents: true];
                    ns_window.setAlphaValue(0.0);
                    ns_window.orderFrontRegardless();
                }
            }

            // 透明覆盖窗口禁用系统阴影
            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::NSWindow;
                for label in ["screenshot", "snap-panel"] {
                    let Some(window) = app.get_webview_window(label) else {
                        continue;
                    };
                    if let Ok(raw) = window.ns_window() {
                        let raw = raw.cast::<NSWindow>();
                        unsafe {
                            if let Some(ns_window) = raw.as_ref() {
                                let _: () = objc2::msg_send![ns_window, setHasShadow: false];
                            }
                        }
                    }
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}
