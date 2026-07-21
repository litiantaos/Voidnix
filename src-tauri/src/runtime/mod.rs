pub mod binary_fetch;
pub mod llm;
pub mod menubar;
pub mod pasteboard;
pub mod permission;
pub mod registry;
pub mod shell_rc;
pub mod shortcut;
pub mod storage;
pub mod window;

/// Mutex 毒锁恢复辅助（统一性审查 G2）：debug 构建下持锁 panic 会毒锁，
/// 后续访问连锁 panic。release（panic=abort）下不会毒，但保持一致风格 + 防 debug 卡死。
/// 全仓所有 `Mutex::lock()` 应通过此函数而非裸 `.unwrap()`。
pub(crate) fn lock_or_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
