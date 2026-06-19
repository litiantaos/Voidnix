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
            crate::runtime::window::get_home_dir,
            crate::runtime::window::pick_directory,
            crate::platform::pasteboard::pasteboard_write_text,
        ])
        .setup(|app| {
            let boot_start = std::time::Instant::now();

            // pre-bootstrap：框架级共享资源（串行，bootstrap 之前）。AX timeout 多扩展共享，
            // 不可下沉扩展 setup（并行 bootstrap 无法保证时序，§2.1）。
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                crate::platform::selection::init_ax_timeout();
            }
            let t_pre = boot_start.elapsed();

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
            let t_build = boot_start.elapsed();

            // block_on 探针（§7 N7）：确认 setup 同步闭包内可安全 block_on（非 tokio worker 嵌套）。
            tauri::async_runtime::block_on(async {});

            crate::runtime::registry::bootstrap(app, registry)?;
            let t_boot = boot_start.elapsed();

            // 框架级主窗口配置（panel 转换 + 圆角）；扩展窗口配置由各扩展 setup 自管（§2.8）
            #[cfg(target_os = "macos")]
            crate::runtime::window::configure_main_window(app.handle());

            // 启动埋点（§0.2/§7 N8）：量化 pre-bootstrap(串行) + bootstrap(join_all 并行) 耗时，
            // 验证 Rust bootstrap <100ms 目标。debug 构建打印，release 静默。
            if cfg!(debug_assertions) {
                let pre = t_pre.as_secs_f64() * 1000.0;
                let build = (t_build - t_pre).as_secs_f64() * 1000.0;
                let boot = (t_boot - t_build).as_secs_f64() * 1000.0;
                let total = t_boot.as_secs_f64() * 1000.0;
                eprintln!(
                    "[boot] pre-bootstrap={pre:.1}ms build-registry={build:.1}ms bootstrap(parallel)={boot:.1}ms rust-total={total:.1}ms (target <100ms bootstrap: {})",
                    if boot < 100.0 { "PASS" } else { "OVER" }
                );
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}
