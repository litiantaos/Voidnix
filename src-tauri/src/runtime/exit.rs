//! 退出钩子：扩展 setup 内 `register` 注册，app 退出（`RunEvent::Exit`）时统一执行。
//! 镜像 menubar.rs 注册范式（LazyLock<Mutex<Vec>> + free function）。
//! 用于「扩展托管的系统资源须随 app 退出回收」的场景（如 proxy dev 变体清理 LaunchDaemon）。

use std::sync::{Arc, LazyLock, Mutex};
use tauri::AppHandle;

use crate::runtime::lock_or_recover;

type ExitHook = Arc<dyn Fn(&AppHandle) + Send + Sync>;

static HOOKS: LazyLock<Mutex<Vec<ExitHook>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 注册退出钩子（扩展 setup 内调用）。
pub fn register(hook: ExitHook) {
    lock_or_recover(&HOOKS).push(hook);
}

/// 执行全部退出钩子（lib.rs 的 RunEvent::Exit 分发）。锁内仅克隆、锁外执行
/// （防钩子内部重入注册死锁）。钩子内阻塞操作（提权弹框等）会延迟 app 退出，
/// 注册方自行保证语义必要。
pub fn run_exit_hooks(app: &AppHandle) {
    let hooks: Vec<ExitHook> = lock_or_recover(&HOOKS).clone();
    for hook in hooks {
        hook(app);
    }
}
