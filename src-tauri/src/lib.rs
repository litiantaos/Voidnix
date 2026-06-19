mod http;
mod platform;
mod runtime;
mod extensions;

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
        .invoke_handler(tauri::generate_handler![
            // 框架命令（permission / shortcut / window），手写、不参与 sync-extensions 扫描（§2.8）
            crate::runtime::permission::check_screen_recording_permission,
            crate::runtime::permission::check_accessibility_permission,
            crate::runtime::permission::request_accessibility_permission,
            crate::runtime::permission::check_full_disk_access_permission,
            crate::runtime::permission::open_privacy_settings,
            crate::runtime::shortcut::start_shortcut_recording,
            crate::runtime::shortcut::stop_shortcut_recording,
            crate::runtime::shortcut::is_app_active,
            crate::runtime::shortcut::hide_window,
            crate::runtime::shortcut::register_global_shortcut,
            crate::runtime::window::set_main_window_size,
            crate::runtime::window::get_home_dir,
            crate::runtime::window::pick_directory,
        ])
        .setup(|app| {
            // pre-bootstrap：框架级共享资源（串行，bootstrap 之前）。AX timeout 多扩展共享，
            // 不可下沉扩展 setup（并行 bootstrap 无法保证时序，§2.1）。
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                crate::platform::selection::init_ax_timeout();
            }

            let registry = crate::runtime::registry::ExtensionRegistry::new()
                .register(crate::extensions::clipboard::ClipboardExtension)
                .register(crate::extensions::screenshot::ScreenshotExtension)
                .register(crate::extensions::awake::AwakeExtension)
                .register(crate::extensions::zsh_autosuggestions::ZshAutosuggestionsExtension)
                .register(crate::extensions::window_manager::WindowManagerExtension)
                .register(crate::extensions::finder_ext::FinderExtExtension)
                .register(crate::extensions::translate::TranslateExtension)
                .register(crate::extensions::agent::AgentExtension)
                .register(crate::extensions::search::SearchExtension);

            // block_on 探针（§7 N7）：确认 setup 同步闭包内可安全 block_on（非 tokio worker 嵌套）。
            tauri::async_runtime::block_on(async {});

            crate::runtime::registry::bootstrap(app, registry)?;

            // 框架级主窗口配置（panel 转换 + 圆角）；扩展窗口配置由各扩展 setup 自管（§2.8）
            #[cfg(target_os = "macos")]
            crate::runtime::window::configure_main_window(app.handle());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}
