use objc2_app_kit::NSApp;
use objc2_foundation::MainThreadMarker;

pub fn activate_app() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApp(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

pub fn is_app_active() -> bool {
    if let Some(mtm) = MainThreadMarker::new() {
        return NSApp(mtm).isActive();
    }
    false
}
