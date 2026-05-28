use std::ffi::c_void;
use std::sync::OnceLock;

use objc2::runtime::AnyObject;

const NONACTIVATING_PANEL_MASK: u64 = 1 << 7;

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
        let name = b"VoidnixMainPanel\0".as_ptr() as *const c_void;
        let existing = objc_getClass(name);
        if !existing.is_null() {
            return PanelClass(existing);
        }

        let superclass = objc_getClass(b"NSPanel\0".as_ptr() as *const c_void);
        let cls = objc_allocateClassPair(superclass, name, 0);
        assert!(!cls.is_null(), "objc_allocateClassPair failed");

        let sel = sel_registerName(b"canBecomeKeyWindow\0".as_ptr() as *const c_void);
        let types = b"c@:\0".as_ptr() as *const c_void;
        let imp = can_become_key as *const c_void;
        class_addMethod(cls, sel, imp, types);

        objc_registerClassPair(cls);
        PanelClass(cls)
    }
    });
    ptr
}

pub fn convert_to_panel(ns_window: *mut AnyObject) {
    if ns_window.is_null() {
        return;
    }

    let panel_class = ensure_panel_class();

    unsafe {
        object_setClass(ns_window as *mut c_void, panel_class);

        let style: u64 = objc2::msg_send![ns_window, styleMask];
        let _: () = objc2::msg_send![ns_window, setStyleMask: style | NONACTIVATING_PANEL_MASK];

        let _: () = objc2::msg_send![ns_window, setBecomesKeyOnlyIfNeeded: false];
        let _: () = objc2::msg_send![ns_window, setHidesOnDeactivate: false];
    }
}
