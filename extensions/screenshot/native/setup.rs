//! Tier 1 启动钩子：screenshot 全屏覆盖窗口的 NSWindow 初始化、
//! 应用激活通知的截图模式重激活监听、JPEG 编码器预热。
//!
//! 这些原本散落在 `lib.rs::run()::setup` 中，PR #3 全部收回扩展自管。

use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub fn configure_overlay_window(app: &AppHandle) {
    use objc2_app_kit::{
        NSScreen, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    };
    use objc2_foundation::MainThreadMarker;

    let Some(window) = app.get_webview_window("screenshot") else {
        return;
    };
    super::install_background_layer(&window);

    let Ok(raw) = window.ns_window() else {
        return;
    };
    let raw = raw.cast::<NSWindow>();
    // SAFETY: ns_window 经 as_ref().unwrap()（raw 来自 ns_window() Ok 分支，非空）；
    // 所有调用均为 NSWindow 标准方法（setAnimationBehavior/setLevel:setCollectionBehavior/
    // setFrame_display:setAlphaValue:setIgnoresMouseEvents:setHasShadow:/orderFrontRegardless）；
    // MainThreadMarker::new().unwrap() 校验主线程
    unsafe {
        let ns_window = raw.as_ref().unwrap();
        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        ns_window.setLevel(objc2_app_kit::NSStatusWindowLevel);
        let _: () = objc2::msg_send![ns_window, setAcceptsMouseMovedEvents: true];
        let behavior = NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle;
        ns_window.setCollectionBehavior(behavior);
        let mtm = MainThreadMarker::new().unwrap();
        let screen = NSScreen::mainScreen(mtm).unwrap();
        ns_window.setFrame_display(screen.frame(), true);
        ns_window.setAlphaValue(0.0);
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
        // 禁用系统阴影（原 lib.rs 的 shadow 循环下沉）
        let _: () = objc2::msg_send![ns_window, setHasShadow: false];
        ns_window.orderFrontRegardless();
    }
}

pub fn install_reactivate_observer(app: &AppHandle) {
    use objc2::rc::Retained;
    use objc2_foundation::{NSNotificationCenter, NSNotificationName, NSString};

    static REACTIVATE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
    let event_app = app.clone();
    let _ = REACTIVATE.set(Box::new(move || {
        super::reactivate_screenshot_window(&event_app);
    }));

    let center = NSNotificationCenter::defaultCenter();
    let name: Retained<NSString> = NSString::from_str("NSApplicationDidBecomeActiveNotification");
    let name_ref: &NSNotificationName =
        // SAFETY: NSString 与 NSNotificationName 内存布局一致（NSNotificationName 是
        // NSString 的 type alias），transmute 仅重解释引用类型，不涉及所有权转移
        unsafe { std::mem::transmute::<&NSString, &NSNotificationName>(&name) };

    // SAFETY: addObserverForName:object:queue:usingBlock: 是 NSNotificationCenter 标准选择子；
    // name_ref 来自上方 transmute（合法 &NSNotificationName），object/queue 传 None；
    // block 经 RcBlock 持有（observer 由 defaultCenter retain，进程生命周期常驻）
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(name_ref),
            None,
            None,
            &block2::RcBlock::new(|_notification| {
                if let Some(cb) = REACTIVATE.get() {
                    cb();
                }
            }),
        );
    }
}

pub fn schedule_jpeg_prewarm(app: &AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let _ = app_handle.run_on_main_thread(|| {
            super::ffi::prewarm_jpeg_encoder();
        });
    });
}

pub fn register_shortcut_hook() {
    use std::sync::atomic::Ordering;
    crate::runtime::shortcut::register_shortcut_hook(
        "screenshot",
        Box::new(|app, _ctx| {
            if super::session::IS_IN_SCREENSHOT_SESSION.swap(true, Ordering::SeqCst) {
                return true;
            }
            let app_clone = app.clone();
            std::thread::spawn(move || {
                let result = super::capture_screen();
                match result {
                    Ok(data) => {
                        let app_for_enter = app_clone.clone();
                        let _ = app_clone.run_on_main_thread(move || {
                            super::session::enter_screenshot_mode_sync(&app_for_enter, data);
                        });
                    }
                    Err(e) => {
                        super::session::IS_IN_SCREENSHOT_SESSION.store(false, Ordering::SeqCst);
                        eprintln!("截图失败: {e}");
                    }
                }
            });
            true
        }),
    );
}
