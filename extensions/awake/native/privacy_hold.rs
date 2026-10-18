//! 隐私屏的全局左键长按退出监视（2s）。
//!
//! 复用 clean-mode 的退出手势：左键按住 2s 且基本不动即退出隐私屏。实现用
//! `NSEvent` global + local monitor（不锁输入、不装 event tap）：global 覆盖
//! 其它 app / 远控注入的点击，local 覆盖 Voidnix 自己的窗口；100ms 轮询线程
//! 判断超时与位移。仅隐私屏开启期间存在，关闭即移除。
//!
//! 为什么不是 CGEventTap：隐私屏不需要吞事件，NSEvent 监听足够；避免额外的
//! Accessibility tap 与维护成本（clean-mode 需要锁输入才用 tap）。

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::ClassType;
use objc2_app_kit::NSEvent;
use objc2_foundation::NSPoint;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const HOLD_MS: u64 = 2000;
const POLL_MS: u64 = 100;
const MOVE_TOLERANCE: f64 = 4.0;

// NSEventMask / NSEventType（AppKit 标准值）
const MASK: u64 = (1 << 1) | (1 << 2) | (1 << 6); // LeftMouseDown | LeftMouseUp | LeftMouseDragged
const EVT_LEFT_DOWN: i64 = 1;
const EVT_LEFT_UP: i64 = 2;
const EVT_LEFT_DRAGGED: i64 = 6;

#[derive(Clone, Copy)]
struct DownState {
    at: Instant,
    start: (f64, f64),
    last: (f64, f64),
}

struct GlobalEntry {
    monitor: *mut AnyObject,
    _block: RcBlock<dyn Fn(*mut AnyObject)>,
}
unsafe impl Send for GlobalEntry {}
unsafe impl Sync for GlobalEntry {}

struct LocalEntry {
    monitor: *mut AnyObject,
    _block: RcBlock<dyn Fn(*mut AnyObject) -> *mut AnyObject>,
}
unsafe impl Send for LocalEntry {}
unsafe impl Sync for LocalEntry {}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static DOWN: Mutex<Option<DownState>> = Mutex::new(None);
static GLOBAL: Mutex<Option<GlobalEntry>> = Mutex::new(None);
static LOCAL: Mutex<Option<LocalEntry>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// 长按判定（纯函数供单测）：按住 >=2s 且位移 <=4pt 才退出。
fn long_press_reached(elapsed: Duration, moved: f64) -> bool {
    elapsed >= Duration::from_millis(HOLD_MS) && moved <= MOVE_TOLERANCE
}

unsafe fn handle_event(event: *mut AnyObject) {
    let ty: i64 = objc2::msg_send![event, type];
    match ty {
        EVT_LEFT_DOWN => {
            let loc: NSPoint = objc2::msg_send![NSEvent::class(), mouseLocation];
            *lock(&DOWN) = Some(DownState {
                at: Instant::now(),
                start: (loc.x, loc.y),
                last: (loc.x, loc.y),
            });
        }
        EVT_LEFT_UP => {
            *lock(&DOWN) = None;
        }
        EVT_LEFT_DRAGGED => {
            let mut guard = lock(&DOWN);
            if let Some(d) = guard.as_mut() {
                let loc: NSPoint = objc2::msg_send![NSEvent::class(), mouseLocation];
                d.last = (loc.x, loc.y);
            }
        }
        _ => {}
    }
}

unsafe fn remove_monitors() {
    if let Some(entry) = lock(&GLOBAL).take() {
        let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: entry.monitor];
        let _: () = objc2::msg_send![entry.monitor, release];
    }
    if let Some(entry) = lock(&LOCAL).take() {
        let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: entry.monitor];
        let _: () = objc2::msg_send![entry.monitor, release];
    }
}

unsafe fn add_monitors() {
    remove_monitors();

    let block: RcBlock<dyn Fn(*mut AnyObject)> = RcBlock::new(|event| unsafe {
        handle_event(event);
    });
    let monitor: *mut AnyObject = objc2::msg_send![
        NSEvent::class(),
        addGlobalMonitorForEventsMatchingMask: MASK,
        handler: &*block
    ];
    if !monitor.is_null() {
        let _: () = objc2::msg_send![monitor, retain];
        *lock(&GLOBAL) = Some(GlobalEntry {
            monitor,
            _block: block,
        });
    }

    let block: RcBlock<dyn Fn(*mut AnyObject) -> *mut AnyObject> = RcBlock::new(|event| {
        unsafe {
            handle_event(event);
        }
        event
    });
    let monitor: *mut AnyObject = objc2::msg_send![
        NSEvent::class(),
        addLocalMonitorForEventsMatchingMask: MASK,
        handler: &*block
    ];
    if !monitor.is_null() {
        let _: () = objc2::msg_send![monitor, retain];
        *lock(&LOCAL) = Some(LocalEntry {
            monitor,
            _block: block,
        });
    }
}

fn spawn_poll(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("awake-privacy-hold".into())
        .spawn(move || {
            while ACTIVE.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(POLL_MS));
                if !ACTIVE.load(Ordering::Acquire) {
                    break;
                }
                let reached = {
                    let guard = lock(&DOWN);
                    guard.as_ref().is_some_and(|d| {
                        long_press_reached(d.at.elapsed(), distance(d.start, d.last))
                    })
                };
                if !reached {
                    continue;
                }
                ACTIVE.store(false, Ordering::Release);
                *lock(&DOWN) = None;
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = super::apply_privacy(&app, false).await;
                });
                break;
            }
        });
}

/// 启动长按监视（仅隐私屏开启时）。须由命令/菜单在隐私状态置位后调用。
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let poll_app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        // 主线程执行时隐私可能已被关闭（快速开关的过期 start）：以当前状态
        // 为准，避免在关闭后重新装监视器/留活轮询线程。
        if !app
            .state::<super::AwakeState>()
            .privacy
            .load(Ordering::Relaxed)
        {
            return;
        }
        let was_active = ACTIVE.swap(true, Ordering::AcqRel);
        *lock(&DOWN) = None;
        // 每次开启都重建监视器（幂等）：即使上一轮的 stop/start 在主线程队列里
        // 交错，也不会出现「ACTIVE 为真但监视器已被移除」的静默失效。
        unsafe { add_monitors() };
        if !was_active {
            spawn_poll(poll_app);
        }
    });
}

/// 停止长按监视（隐私屏关闭 / disarm / fail-open 时调用）。幂等。
pub fn stop(app: &AppHandle) {
    ACTIVE.store(false, Ordering::Release);
    *lock(&DOWN) = None;
    let app = app.clone();
    let _ = app.run_on_main_thread(move || unsafe { remove_monitors() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_press_requires_hold_and_stillness() {
        assert!(long_press_reached(Duration::from_millis(2000), 0.0));
        assert!(!long_press_reached(Duration::from_millis(1999), 0.0));
        assert!(!long_press_reached(Duration::from_millis(2500), 5.0));
        assert!(long_press_reached(Duration::from_millis(2500), 3.0));
    }
}
