mod core;
mod infra;
mod macos;
#[cfg(feature = "specta")]
mod type_gen;
mod extensions;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            crate::core::window::show_main(app);
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
            let registry = crate::core::tier1::Tier1Registry::new()
                .register(crate::extensions::clipboard::Plugin)
                .register(crate::extensions::screenshot::Plugin)
                .register(crate::extensions::awake::Plugin)
                .register(crate::extensions::zsh_autosuggestions::Plugin)
                .register(crate::extensions::window_manager::Plugin)
                .register(crate::extensions::finder_ext::Plugin)
                .register(crate::extensions::translate::Plugin)
                .register(crate::extensions::agent::Plugin);
            crate::core::tier1::bootstrap(app, registry)?;

            // Agent 框架层全局 state
            app.manage(crate::core::agent::cancellation::SessionRegistry::default());
            app.manage(crate::core::agent::approval::ApprovalManager::default());

            // icon 缓存淘汰：启动时清理过期/超量文件
            crate::infra::path::cleanup_icon_cache(400, 90);

            // Tier 2 扩展运行时
            let ext_state = crate::core::ext_commands::ExtensionLoaderState::new();
            ext_state.loader().rescan(app.handle()).ok();
            app.manage(ext_state);

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "macos")]
            crate::macos::text_selection::init_ax_timeout();

            // Agent session 由用户主动 agent_abort 控制；
            // 不在主窗失焦时 cancel_all（panel 失焦频繁，会误中断审批弹窗）

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
            }

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
                    macos::panel::convert_to_panel(raw.cast());
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

            // 透明覆盖窗口禁用系统阴影：大面积透明时，系统阴影会勾勒局部可见元素
            // （工具栏/浮窗）的轮廓，形成来历不明的框线。主窗口有完整可见内容，
            // 保留系统阴影提供层次感。
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