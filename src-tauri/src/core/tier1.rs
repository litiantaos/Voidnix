use tauri::{AppHandle, Wry};

/// Tier 1 内置扩展 trait。
///
/// 与 `configure_app!` 宏分工：
/// - 宏负责**编译期** API 表面注册：`#[tauri::command]` 函数与 `init() -> TauriPlugin` 插件
/// - 本 trait 负责**运行时**生命周期钩子：窗口初始化、后台监听器、资源预热等
pub trait Tier1Extension: Send + Sync + 'static {
    /// 扩展 ID，应与 `extensions/<id>/` 目录名一致
    fn id(&self) -> &'static str;

    /// 应用启动完成后调用，按注册顺序串行执行。任一扩展返回 Err 则中断启动。
    fn on_setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
}

/// Tier 1 扩展注册中心
pub struct Tier1Registry {
    extensions: Vec<Box<dyn Tier1Extension>>,
}

impl Tier1Registry {
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    pub fn register<E: Tier1Extension>(mut self, ext: E) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    pub fn run_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        for ext in &self.extensions {
            ext.on_setup(app).inspect_err(|e| {
                eprintln!("[tier1] '{}' on_setup failed: {e}", ext.id());
            })?;
        }
        Ok(())
    }
}

impl Default for Tier1Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// 在 `tauri::Builder::setup` 闭包内调用：执行所有 Tier 1 扩展的 `on_setup`，
/// 然后以 `app.manage()` 持有 registry。
pub fn bootstrap(app: &mut tauri::App<Wry>, registry: Tier1Registry) -> tauri::Result<()> {
    use tauri::Manager;
    let handle = app.handle().clone();
    registry.run_setup(&handle)?;
    app.manage(registry);
    Ok(())
}
