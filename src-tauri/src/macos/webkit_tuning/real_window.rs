#![allow(dead_code)]

#[cfg(target_os = "macos")]
use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
use super::obj_exception;
#[cfg(target_os = "macos")]
use super::entry::{Frame, WindowOps, PresentationBridge};

#[cfg(target_os = "macos")]
pub(crate) struct RealWindow {
    pub(crate) ns_window: *mut objc2::runtime::AnyObject,
}

#[cfg(target_os = "macos")]
unsafe fn get_wkwebview(ns_window: *mut objc2::runtime::AnyObject) -> *mut objc2::runtime::AnyObject {
    if ns_window.is_null() { return std::ptr::null_mut(); }
    let mut result = std::ptr::null_mut();
    obj_exception::try_block(|| {
        let content_view: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, contentView];
        if content_view.is_null() { return; }
        let subviews: *mut objc2::runtime::AnyObject = objc2::msg_send![content_view, subviews];
        if subviews.is_null() { return; }
        let count: usize = objc2::msg_send![subviews, count];
        if count == 0 { return; }
        result = objc2::msg_send![subviews, objectAtIndex: 0usize];
    });
    result
}

#[cfg(target_os = "macos")]
pub(crate) struct RealPresentationBridge<'a> {
    pub(crate) window: &'a WebviewWindow,
}

#[cfg(target_os = "macos")]
impl PresentationBridge for RealPresentationBridge<'_> {
    fn schedule(&self, timeout_ms: u64, cb: Box<dyn FnOnce(bool) + Send>) -> bool {
        use objc2_app_kit::NSWindow;

        let ns_window_ptr = match self.window.ns_window() {
            Ok(ptr) => ptr.cast::<NSWindow>() as *mut objc2::runtime::AnyObject,
            Err(_) => return false,
        };

        let wk_view_ptr = unsafe { get_wkwebview(ns_window_ptr) };
        if wk_view_ptr.is_null() { return false; }

        extern "C" {
            fn voidnix_do_after_next_presentation_update_fn(
                web: *mut objc2::runtime::AnyObject,
                window: *mut objc2::runtime::AnyObject,
                timeout_ms: u64,
                cb_fn: extern "C-unwind" fn(*mut std::ffi::c_void, bool),
                ctx: *mut std::ffi::c_void,
            ) -> bool;
        }

        extern "C-unwind" fn trampoline(ctx: *mut std::ffi::c_void, ok: bool) {
            let cb = unsafe { Box::from_raw(ctx as *mut Box<dyn FnOnce(bool) + Send>) };
            cb(ok);
        }

        let ctx = Box::into_raw(Box::new(cb)) as *mut std::ffi::c_void;
        let result = unsafe {
            voidnix_do_after_next_presentation_update_fn(
                wk_view_ptr,
                ns_window_ptr,
                timeout_ms,
                trampoline,
                ctx,
            )
        };
        if !result {
            unsafe { drop(Box::from_raw(ctx as *mut Box<dyn FnOnce(bool) + Send>)); }
        }
        result
    }
}

#[cfg(target_os = "macos")]
unsafe impl Send for RealWindow {}
#[cfg(target_os = "macos")]
unsafe impl Sync for RealWindow {}

#[cfg(target_os = "macos")]
impl RealWindow {
    pub fn from_webview_window(window: &WebviewWindow) -> Option<Self> {
        use objc2_app_kit::NSWindow;
        let raw = window.ns_window().ok()?.cast::<NSWindow>();
        Some(Self {
            ns_window: raw as *mut objc2::runtime::AnyObject,
        })
    }
}

#[cfg(target_os = "macos")]
impl WindowOps for RealWindow {
    fn alpha(&self) -> f64 {
        let mut result = 1.0f64;
        obj_exception::try_block(|| unsafe {
            result = objc2::msg_send![self.ns_window, alphaValue];
        });
        result
    }

    fn set_alpha(&self, v: f64) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, setAlphaValue: v];
        });
    }

    fn window_frame(&self) -> Frame {
        let mut result = Frame::default();
        obj_exception::try_block(|| unsafe {
            let frame: objc2_foundation::NSRect = objc2::msg_send![self.ns_window, frame];
            result = Frame::new(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);
        });
        result
    }

    fn set_window_frame(&self, f: Frame, animated: bool) {
        obj_exception::try_block(|| unsafe {
            use objc2_foundation::{NSPoint, NSRect, NSSize};
            let rect = NSRect::new(
                NSPoint::new(f.x, f.y),
                NSSize::new(f.width, f.height),
            );
            if animated {
                let _: () = objc2::msg_send![objc2::class!(NSAnimationContext), beginGrouping];
                let ctx: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![objc2::class!(NSAnimationContext), currentContext];
                let _: () = objc2::msg_send![ctx, setAllowsImplicitAnimation: true];
                let _: () = objc2::msg_send![ctx, setDuration: 0.18f64];
                let _: () = objc2::msg_send![self.ns_window, setFrame: rect, display: false, animate: true];
                let _: () = objc2::msg_send![objc2::class!(NSAnimationContext), endGrouping];
            } else {
                let _: () = objc2::msg_send![self.ns_window, setFrame: rect, display: false];
            }
        });
    }

    fn ignores_mouse(&self) -> bool {
        let mut result = false;
        obj_exception::try_block(|| unsafe {
            result = objc2::msg_send![self.ns_window, ignoresMouseEvents];
        });
        result
    }

    fn set_ignores_mouse(&self, v: bool) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, setIgnoresMouseEvents: v];
        });
    }

    fn order_out_count(&self) -> u32 {
        0
    }

    fn order_front(&self) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, orderFrontRegardless];
        });
    }

    fn occlusion_detection(&self) -> bool {
        unsafe {
            extern "C" {
                fn voidnix_get_occlusion_detection(view: *mut objc2::runtime::AnyObject) -> bool;
            }
            let wk_view = get_wkwebview(self.ns_window);
            voidnix_get_occlusion_detection(wk_view)
        }
    }

    fn set_occlusion_detection(&self, v: bool) {
        unsafe {
            extern "C" {
                fn voidnix_set_occlusion_detection(view: *mut objc2::runtime::AnyObject, enabled: bool);
            }
            let wk_view = get_wkwebview(self.ns_window);
            voidnix_set_occlusion_detection(wk_view, v);
        }
    }

    fn collection_behavior(&self) -> u64 {
        let mut result = 0u64;
        obj_exception::try_block(|| unsafe {
            result = objc2::msg_send![self.ns_window, collectionBehavior];
        });
        result
    }

    fn set_collection_behavior(&self, v: u64) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, setCollectionBehavior: v];
        });
    }

    fn content_view_corner_radius(&self) -> f64 {
        let mut result = 0.0f64;
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    result = objc2::msg_send![layer, cornerRadius];
                }
            }
        });
        result
    }

    fn set_content_view_corner_radius(&self, r: f64) {
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    let _: () = objc2::msg_send![layer, setCornerRadius: r];
                }
            }
        });
    }

    fn content_view_masks_to_bounds(&self) -> bool {
        let mut result = false;
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    result = objc2::msg_send![layer, masksToBounds];
                }
            }
        });
        result
    }

    fn set_content_view_masks_to_bounds(&self, v: bool) {
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    let _: () = objc2::msg_send![layer, setMasksToBounds: v];
                }
            }
        });
    }

    fn wkwebview_frame(&self) -> Frame {
        let mut result = Frame::default();
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let frame: objc2_foundation::NSRect =
                    objc2::msg_send![content_view, frame];
                result = Frame::new(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);
            }
        });
        result
    }

    fn set_wkwebview_frame(&self, f: Frame) {
        obj_exception::try_block(|| unsafe {
            use objc2_foundation::{NSPoint, NSRect, NSSize};
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let rect = NSRect::new(
                    NSPoint::new(f.x, f.y),
                    NSSize::new(f.width, f.height),
                );
                let _: () = objc2::msg_send![content_view, setFrame: rect];
            }
        });
    }

    fn observer_count(&self) -> u32 {
        0
    }

    fn make_key(&self) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, makeKeyWindow];
        });
    }

    fn resign_key(&self) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, resignKeyWindow];
        });
    }
}
