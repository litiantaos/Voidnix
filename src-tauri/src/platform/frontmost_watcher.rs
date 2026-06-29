//! 系统授权弹窗关闭后自动恢复焦点。
//!
//! 当系统弹窗（/System/ 路径进程）接管 frontmost 时,is_app_active 返回 true
//! 抑制了 blur hide。弹窗关闭后 macOS 把 frontmost 还给原前台 app,但 panel
//! 已丢失 key window —— 此观察器在 frontmost 变更时检测到这一状态,自动
//! makeKeyWindow 恢复键盘焦点。若用户在此期间主动切换到其他 app,则触发 dismiss。

#[cfg(target_os = "macos")]
mod inner {
    use objc2::runtime::AnyObject;
    use objc2_app_kit::NSWorkspace;
    use std::sync::Mutex;
    use tauri::{Emitter, Manager};

    struct WatcherEntry {
        observer: *mut AnyObject,
        #[allow(dead_code)]
        block: block2::RcBlock<dyn Fn(*mut AnyObject)>,
    }
    unsafe impl Send for WatcherEntry {}
    unsafe impl Sync for WatcherEntry {}

    static WATCHER: Mutex<Option<WatcherEntry>> = Mutex::new(None);

    extern "C" {
        static NSWorkspaceDidActivateApplicationNotification: *mut AnyObject;
    }

    pub fn add(app: &tauri::AppHandle) {
        let mut guard = WATCHER.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            return;
        }

        let app_handle = app.clone();
        let block: block2::RcBlock<dyn Fn(*mut AnyObject)> = block2::RcBlock::new(move |_| {
            handle_frontmost_change(&app_handle);
        });

        unsafe {
            let ws = NSWorkspace::sharedWorkspace();
            let center: *mut AnyObject = objc2::msg_send![&ws, notificationCenter];
            let observer: *mut AnyObject = objc2::msg_send![
                center,
                addObserverForName: NSWorkspaceDidActivateApplicationNotification,
                object: core::ptr::null_mut::<AnyObject>(),
                queue: core::ptr::null_mut::<AnyObject>(),
                usingBlock: &*block
            ];
            if !observer.is_null() {
                *guard = Some(WatcherEntry { observer, block });
            }
        }
    }

    pub fn remove() {
        let mut guard = WATCHER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = guard.take() {
            unsafe {
                let ws = NSWorkspace::sharedWorkspace();
                let center: *mut AnyObject = objc2::msg_send![&ws, notificationCenter];
                let _: () = objc2::msg_send![center, removeObserver: entry.observer];
            }
        }
    }

    fn handle_frontmost_change(app: &tauri::AppHandle) {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if !window.is_visible().unwrap_or(false) {
            return;
        }

        // panel 持有 key 或系统弹窗仍在 → 无需干预
        if crate::platform::focus::is_app_active() {
            return;
        }

        // panel 可见但丢 key,frontmost 非 system
        // frontmost == 原前台 app → 系统弹窗关闭,macOS 还原 frontmost → 重新取 key
        // frontmost != 原前台 app → 用户主动切换 → dismiss
        let prev = crate::platform::focus::captured_pid();
        match crate::platform::focus::current_frontmost_pid() {
            Some(front) if front == prev => {
                crate::platform::window::make_key_window(&window);
            }
            _ => {
                let _ = app.emit("frontmost-changed", ());
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    pub fn add(_app: &tauri::AppHandle) {}
    pub fn remove() {}
}

pub fn add(app: &tauri::AppHandle) {
    inner::add(app);
}

pub fn remove() {
    inner::remove();
}
