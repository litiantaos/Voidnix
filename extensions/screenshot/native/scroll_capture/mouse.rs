//! 滚动截屏鼠标穿透：选区内忽略鼠标，工具栏保留点击。

use objc2::runtime::AnyObject;
use std::sync::Mutex;

use super::super::ffi::{
    voidnix_screenshot_get_mouse_location, voidnix_screenshot_set_ignores_mouse,
};
use super::state::SESSION;

struct SendObj(*mut AnyObject);
unsafe impl Send for SendObj {}
unsafe impl Sync for SendObj {}

static GLOBAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));
static LOCAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

unsafe fn cur_loc() -> (f64, f64) {
    let mut x: f64 = 0.0;
    let mut y: f64 = 0.0;
    voidnix_screenshot_get_mouse_location(&mut x, &mut y, 0.0);
    (x, y)
}

unsafe fn check_and_toggle() {
    let (mx, my) = cur_loc();
    let snapshot = {
        let g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        g.as_ref().map(|s| {
            (
                s.sel_x,
                s.sel_y,
                s.sel_w,
                s.sel_h,
                s.ns_window_addr,
                s.ignoring_mouse,
                s.toolbar_rect,
            )
        })
    };
    let Some((sx, sy, sw, sh, ns_addr, currently_ignoring, toolbar_rect)) = snapshot else {
        return;
    };
    let in_sel = mx >= sx && mx <= sx + sw && my >= sy && my <= sy + sh;
    // 工具栏区域不穿透(即使落在选区内,也保留接收点击)
    let in_toolbar = toolbar_rect
        .is_some_and(|(tx, ty, tw, th)| mx >= tx && mx <= tx + tw && my >= ty && my <= ty + th);
    let in_hole = in_sel && !in_toolbar;
    if in_hole != currently_ignoring {
        let ptr = ns_addr as *mut std::ffi::c_void;
        voidnix_screenshot_set_ignores_mouse(ptr, if in_hole { 1 } else { 0 });
        let mut g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = g.as_mut() {
            s.ignoring_mouse = in_hole;
        }
    }
}

pub fn start() {
    use objc2::ClassType;
    use objc2_app_kit::NSEvent;
    {
        let g = GLOBAL_MONITOR.lock().unwrap_or_else(|e| e.into_inner());
        if !g.0.is_null() {
            return;
        }
    }
    let mask: u64 = (1u64 << 5)
        | (1u64 << 6)
        | (1u64 << 7)
        | (1u64 << 27)
        | (1u64 << 22)
        | (1u64 << 1)
        | (1u64 << 2)
        | (1u64 << 3)
        | (1u64 << 4);
    {
        // SAFETY (closure body): check_and_toggle 为 unsafe fn，内部所有指针操作
        // 经 SESSION 快照读取 + ns_addr 非 0 保障；闭包在 NSEvent 回调线程执行
        let blk = block2::RcBlock::new(move |_event: *mut AnyObject| unsafe {
            check_and_toggle();
        });
        // SAFETY: mask 为标准 NSEvent 位掩码；block 经 RcBlock 持有，&*block 取引用；
        // addGlobalMonitorForEventsMatchingMask: 返回 monitor，null 检查后
        // retain + forget block（生命周期随 monitor，stop 时 removeMonitor+release）
        unsafe {
            let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*blk];
            if !m.is_null() {
                let _: () = objc2::msg_send![m, retain];
                *GLOBAL_MONITOR.lock().unwrap_or_else(|e| e.into_inner()) = SendObj(m);
                std::mem::forget(blk);
            }
        }
    }
    {
        let blk = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
            // SAFETY: check_and_toggle 为 unsafe fn，同 global 闭包契约
            unsafe {
                check_and_toggle();
            }
            event
        });
        // SAFETY: 同 global monitor 契约；local monitor 返回 event（透传）
        unsafe {
            let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addLocalMonitorForEventsMatchingMask: mask, handler: &*blk];
            if !m.is_null() {
                let _: () = objc2::msg_send![m, retain];
                *LOCAL_MONITOR.lock().unwrap_or_else(|e| e.into_inner()) = SendObj(m);
                std::mem::forget(blk);
            }
        }
    }
}

pub fn stop() {
    use objc2::ClassType;
    use objc2_app_kit::NSEvent;
    for slot in [&GLOBAL_MONITOR, &LOCAL_MONITOR] {
        let mut g = slot.lock().unwrap_or_else(|e| e.into_inner());
        if !g.0.is_null() {
            // SAFETY: g.0 非 null（已检查）；removeMonitor + release 与 start 的
            // retain + forget 配对（monitor 注销 + 引用计数归零）
            unsafe {
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: g.0];
                let _: () = objc2::msg_send![g.0, release];
            }
            *g = SendObj(std::ptr::null_mut());
        }
    }
}
