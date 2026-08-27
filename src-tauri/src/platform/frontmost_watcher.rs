//! 系统授权弹窗关闭后自动恢复焦点。
//!
//! 当系统弹窗（/System/ 路径进程）接管 frontmost 时,is_app_active 返回 true
//! 抑制了 blur hide。弹窗关闭后 macOS 把 frontmost 还给原前台 app,但 panel
//! 已丢失 key window —— 此观察器在 frontmost 变更时检测到这一状态,自动
//! makeKeyWindow 恢复键盘焦点。若用户在此期间主动切换到其他 app,则触发 dismiss。

#[cfg(target_os = "macos")]
mod inner {
    use crate::runtime::lock_or_recover;
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
        let mut guard = lock_or_recover(&WATCHER);
        if guard.is_some() {
            return;
        }

        let app_handle = app.clone();
        let block: block2::RcBlock<dyn Fn(*mut AnyObject)> = block2::RcBlock::new(move |_| {
            // NSWorkspace 通知 queue=NULL 在发送线程回调；ns_window / is_app_active /
            // makeKeyWindow 均主线程限定，统一转主线程（镜像 shortcut::is_app_active 的 H4 修复）
            let app = app_handle.clone();
            let _ = app_handle.run_on_main_thread(move || handle_frontmost_change(&app));
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
        let mut guard = lock_or_recover(&WATCHER);
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
        // hide 不 orderOut：is_visible 在 alpha=0 时仍可能为 true，以 alpha 为准
        let visibly_shown = window
            .ns_window()
            .ok()
            .and_then(|p| {
                let raw = p.cast::<objc2_app_kit::NSWindow>();
                // SAFETY: as_ref 后读 alphaValue
                unsafe { raw.as_ref().map(|ns| ns.alphaValue() >= 0.01) }
            })
            .unwrap_or(false);
        if !visibly_shown {
            return;
        }

        // frontmost == Voidnix 自身（WKWebView 聚焦可编辑元素触发的自我激活）：
        // 交互流在自己身上,不构成用户切换。激活事务可能短暂夺走 panel key——
        // 若已丢失则恢复,否则窗口停留在「可见但无键」状态（打字无响应）。
        // 置于 is_app_active 守卫之前：自我激活时该守卫恒真,后续分支不可达
        if crate::platform::focus::current_frontmost_pid().is_none() {
            crate::platform::window::make_key_window(&window);
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
