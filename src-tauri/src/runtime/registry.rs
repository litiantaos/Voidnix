use tauri::{AppHandle, Wry};

/// 扩展 trait：所有扩展（native / pure）统一的运行时生命周期契约。
///
/// 与 `configure_app!` 宏分工：
/// - 宏负责**编译期** API 表面注册：`#[tauri::command]` 函数与 `init() -> TauriPlugin` 插件
/// - 本 trait 负责**运行时**生命周期钩子：启动初始化、后台监听器、资源预热、清理等
pub trait Extension: Send + Sync + 'static {
    /// 扩展 ID，应与 `extensions/<id>/` 目录名一致
    fn id(&self) -> &'static str;

    /// 依赖的其他扩展 id（用于并行 bootstrap 拓扑排序，未实现并行时忽略）
    #[allow(dead_code)]
    fn deps(&self) -> &'static [&'static str] {
        &[]
    }

    /// 应用启动完成后调用。任一扩展返回 Err 则中断启动。
    fn on_setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }

    /// 应用退出前调用，用于清理资源。
    #[allow(dead_code)]
    fn on_teardown(&self, _app: &AppHandle) {}
}

/// 扩展注册中心
pub struct ExtensionRegistry {
    extensions: Vec<Box<dyn Extension>>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    pub fn register<E: Extension>(mut self, ext: E) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    pub fn run_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        for ext in &self.extensions {
            ext.on_setup(app).inspect_err(|e| {
                eprintln!("[ext] '{}' on_setup failed: {e}", ext.id());
            })?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn run_teardown(&self, app: &AppHandle) {
        for ext in self.extensions.iter().rev() {
            ext.on_teardown(app);
        }
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 在 `tauri::Builder::setup` 闭包内调用：执行所有扩展的 `on_setup`，
/// 然后以 `app.manage()` 持有 registry。
pub fn bootstrap(app: &mut tauri::App<Wry>, registry: ExtensionRegistry) -> tauri::Result<()> {
    use tauri::Manager;
    let handle = app.handle().clone();
    registry.run_setup(&handle)?;
    app.manage(registry);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExt {
        ext_id: &'static str,
    }

    impl Extension for MockExt {
        fn id(&self) -> &'static str {
            self.ext_id
        }
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = ExtensionRegistry::new();
        assert!(reg.extensions.is_empty());
    }

    #[test]
    fn register_single_extension() {
        let reg = ExtensionRegistry::new().register(MockExt { ext_id: "test" });
        assert_eq!(reg.extensions.len(), 1);
        assert_eq!(reg.extensions[0].id(), "test");
    }

    #[test]
    fn register_preserves_order() {
        let reg = ExtensionRegistry::new()
            .register(MockExt { ext_id: "first" })
            .register(MockExt { ext_id: "second" })
            .register(MockExt { ext_id: "third" });
        assert_eq!(reg.extensions.len(), 3);
        assert_eq!(reg.extensions[0].id(), "first");
        assert_eq!(reg.extensions[1].id(), "second");
        assert_eq!(reg.extensions[2].id(), "third");
    }

    #[test]
    fn default_impl() {
        let reg = ExtensionRegistry::default();
        assert!(reg.extensions.is_empty());
    }
}
