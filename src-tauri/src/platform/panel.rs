use std::ffi::c_void;
use std::sync::OnceLock;

use objc2::runtime::AnyObject;

struct PanelClass(*mut c_void);
unsafe impl Send for PanelClass {}
unsafe impl Sync for PanelClass {}

static PANEL_CLASS: OnceLock<PanelClass> = OnceLock::new();

extern "C" fn can_become_key(_this: *mut c_void, _cmd: *const c_void) -> i8 {
    1
}

extern "C" {
    fn objc_getClass(name: *const c_void) -> *mut c_void;
    fn objc_allocateClassPair(
        superclass: *const c_void,
        name: *const c_void,
        extra_bytes: usize,
    ) -> *mut c_void;
    fn objc_registerClassPair(cls: *mut c_void);
    fn class_addMethod(
        cls: *const c_void,
        name: *const c_void,
        imp: *const c_void,
        types: *const c_void,
    ) -> i32;
    fn sel_registerName(name: *const c_void) -> *const c_void;
    fn object_setClass(obj: *mut c_void, cls: *const c_void) -> *mut c_void;
}

fn ensure_panel_class() -> *mut c_void {
    let PanelClass(ptr) = *PANEL_CLASS.get_or_init(|| {
        unsafe {
        let name = c"VoidnixMainPanel".as_ptr() as *const c_void;
        let existing = objc_getClass(name);
        if !existing.is_null() {
            return PanelClass(existing);
        }

        let superclass = objc_getClass(c"NSPanel".as_ptr() as *const c_void);
        let cls = objc_allocateClassPair(superclass, name, 0);
        assert!(!cls.is_null(), "objc_allocateClassPair failed");

        let sel = sel_registerName(c"canBecomeKeyWindow".as_ptr() as *const c_void);
        let types = c"c@:".as_ptr() as *const c_void;
        let imp = can_become_key as *const c_void;
        class_addMethod(cls, sel, imp, types);

        objc_registerClassPair(cls);
        PanelClass(cls)
    }
    });
    ptr
}

/// 把 NSWindow 替换为自定义 NSPanel 子类并打开 NonactivatingPanel 属性。
///
/// 这是「轻浮层」体感的基底:panel makeKey 时不抢 NSApp active,
/// 原应用菜单栏 / Dock 不动。但 panel 仍会偷走系统级 key window /
/// first responder —— 关闭时由调用方（runtime::window::hide_main）走
/// `restore_captured`（deactivate_app + activate_app_by_pid）把焦点还给原应用
/// 窗口。两步缺一不可。
pub fn convert_to_panel(ns_window: *mut AnyObject) {
    if ns_window.is_null() {
        return;
    }

    let panel_class = ensure_panel_class();

    unsafe {
        object_setClass(ns_window as *mut c_void, panel_class);

        // NSWindowStyleMaskNonactivatingPanel = 1 << 7
        let cur_mask: usize = objc2::msg_send![ns_window, styleMask];
        let nonactivating: usize = 1 << 7;
        let _: () = objc2::msg_send![ns_window, setStyleMask: cur_mask | nonactivating];

        // `false` 让 panel 显示后立即 makeKey 接收输入(主窗口)/被点击时立即响应
        // (SnapPanel 首击触发 layout) —— `true` 会消耗首次点击只 makeKey 不响应。
        let _: () = objc2::msg_send![ns_window, setBecomesKeyOnlyIfNeeded: false];
        let _: () = objc2::msg_send![ns_window, setHidesOnDeactivate: false];
    }
}
