use tauri::AppHandle;

/// 扩展 trait：所有扩展（native / pure）统一的运行时生命周期契约。
///
/// - 命令注册在各扩展 `init()` 局部 `invoke_handler`
/// - 本 trait 负责运行时生命周期钩子：setup 并行执行
///
/// **并行 bootstrap**：setup 无跨扩展依赖，全仓零跨扩展 import，
/// 故 join_all 并行。setup 内禁止依赖其它扩展 setup 的产物、禁止初始化
/// 框架级共享资源（此类放 lib.rs pre-bootstrap 串行）。
#[async_trait::async_trait]
pub trait Extension: Send + Sync + 'static {
    /// 扩展 ID，应与 `extensions/<id>/` 目录名一致。
    fn id(&self) -> &'static str;

    /// 启动钩子（并行执行）。任一失败则中断 bootstrap。
    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
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
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 在 `tauri::Builder::setup` 同步闭包内调用：并行（join_all）执行所有扩展 setup，
/// 随后 `app.manage()` 持有 registry。
///
/// **block_on 安全性**：Tauri setup 闭包在主线程同步执行，不在 tokio
/// worker 上下文内，故 `tauri::async_runtime::block_on` 非嵌套调用、不会 panic。
/// lib.rs pre-bootstrap 处的 block_on 探针为运行期 canary。
pub fn bootstrap(
    app: &mut tauri::App<tauri::Wry>,
    registry: ExtensionRegistry,
) -> tauri::Result<()> {
    use tauri::Manager;
    let handle = app.handle().clone();
    let count = registry.extensions.len();
    let start = std::time::Instant::now();

    // 扩展自治：单扩展 setup 失败 log + 跳过，不拖垮整体启动（对齐前端 main.ts try/catch）。
    let failed = tauri::async_runtime::block_on(async {
        let results =
            futures_util::future::join_all(registry.extensions.iter().map(|e| e.setup(&handle)))
                .await;
        let mut n = 0usize;
        for (idx, r) in results.into_iter().enumerate() {
            if let Err(e) = r {
                n += 1;
                eprintln!(
                    "[ext] '{}' setup failed (skipped): {e}",
                    registry.extensions[idx].id()
                );
            }
        }
        n
    });

    if failed == 0 {
        eprintln!(
            "[bootstrap] {} extensions setup in {:?}",
            count,
            start.elapsed()
        );
    } else {
        eprintln!(
            "[bootstrap] {} extensions setup in {:?} ({} failed, isolated)",
            count,
            start.elapsed(),
            failed
        );
    }
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

    #[tokio::test]
    async fn register_holds_extensions_with_async_setup() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let counter = Arc::new(AtomicUsize::new(0));

        struct CountingExt {
            c: Arc<AtomicUsize>,
        }
        #[async_trait::async_trait]
        impl Extension for CountingExt {
            fn id(&self) -> &'static str {
                "counting"
            }
            async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
                self.c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        // registry 能持有带 async setup 的扩展。join_all 真实并行驱动需 AppHandle，
        // 此处无法注入；并行 join_all 的运行期正确性由 lib.rs 启动埋点 + tauri:dev
        // 冒烟保证。
        let reg = ExtensionRegistry::new()
            .register(CountingExt { c: counter.clone() })
            .register(CountingExt { c: counter.clone() })
            .register(CountingExt { c: counter.clone() });
        assert_eq!(reg.extensions.len(), 3);
    }
}
