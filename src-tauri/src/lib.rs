mod extensions;
mod http;
mod platform;
mod runtime;

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
        .setup(|app| {
            let boot_start = std::time::Instant::now();

            // 启动期统一清理 /tmp 残留（覆盖 voidnix_*、voidnix-icon-*、voidnix/picker.jpg）
            crate::runtime::storage::cleanup_all_voidnix_temps();

            // pre-bootstrap：框架级共享资源（串行，bootstrap 之前）。AX timeout 多扩展共享，
            // 不可下沉扩展 setup（concurrent bootstrap 无法保证时序）。
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
                .register(crate::extensions::clean_mode::CleanModeExtension)
                .register(crate::extensions::zsh_autosuggestions::ZshAutosuggestionsExtension)
                .register(crate::extensions::window_manager::WindowManagerExtension)
                .register(crate::extensions::finder_ext::FinderExtExtension)
                .register(crate::extensions::translate::TranslateExtension)
                .register(crate::extensions::agent::AgentExtension)
                .register(crate::extensions::search::SearchExtension)
                .register(crate::extensions::proxy::ProxyExtension)
                .register(crate::extensions::system_status::SystemStatusExtension);
            let t_build = boot_start.elapsed();

            // block_on 探针：确认 setup 同步闭包内可安全 block_on（非 tokio worker 嵌套）。
            tauri::async_runtime::block_on(async {});

            crate::runtime::registry::bootstrap(app, registry)?;
            let t_boot = boot_start.elapsed();

            // 框架级主窗口配置（panel 转换 + 圆角）；扩展窗口配置由各扩展 setup 自管
            #[cfg(target_os = "macos")]
            crate::runtime::window::configure_main_window(app.handle());

            // 启动埋点：量化 pre-bootstrap(串行) + bootstrap(join_all concurrent) 耗时，
            // 验证 Rust bootstrap <100ms 目标。debug 构建打印，release 静默。
            // 注：block_on(join_all) 为单线程上的并发交错（cooperative），非多核并行；
            // 多数 setup 内部 spawn_blocking 委托，实际差异不大。
            if cfg!(debug_assertions) {
                let pre = t_pre.as_secs_f64() * 1000.0;
                let build = (t_build - t_pre).as_secs_f64() * 1000.0;
                let boot = (t_boot - t_build).as_secs_f64() * 1000.0;
                let total = t_boot.as_secs_f64() * 1000.0;
                eprintln!(
                    "[boot] pre-bootstrap={pre:.1}ms build-registry={build:.1}ms bootstrap(concurrent)={boot:.1}ms rust-total={total:.1}ms (target <100ms bootstrap: {})",
                    if boot < 100.0 { "PASS" } else { "OVER" }
                );
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running launcher");
}
