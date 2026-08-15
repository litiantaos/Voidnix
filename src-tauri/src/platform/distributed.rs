//! 同用户会话跨进程事件（NSDistributedNotificationCenter 原生总线）。
//!
//! macOS 自带的进程间通知机制：零端口、零文件、零自建协议。发布者以自身标识作 object，
//! 观察者按 object 过滤（自发布自收天然排除）。观察为进程生命周期注册，无 remove。
//! 回调投递线程不作保证，handler 须自备线程安全。当前消费者：proxy TUN 让渡即时对账
//! （extensions/proxy/native/lifecycle.rs）。

#[cfg(target_os = "macos")]
mod inner {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;
    use std::sync::{Arc, Mutex};

    use crate::runtime::lock_or_recover;

    struct ObserverEntry {
        // observer token 仅存档（进程生命周期不 remove；若未来需要动态注销用它调 removeObserver）
        #[allow(dead_code)]
        observer: *mut AnyObject,
        // block 须存活于观察期全程（center 不持有拷贝），存 entry 保命
        #[allow(dead_code)]
        block: block2::RcBlock<dyn Fn(*mut AnyObject)>,
    }
    unsafe impl Send for ObserverEntry {}
    unsafe impl Sync for ObserverEntry {}

    static OBSERVER: Mutex<Option<ObserverEntry>> = Mutex::new(None);

    fn center() -> *mut AnyObject {
        unsafe {
            objc2::msg_send![
                objc2::class!(NSDistributedNotificationCenter),
                defaultCenter
            ]
        }
    }

    /// 发布分布式通知（name + 发布者标识）。deliverImmediately 立即送达，防接收方被
    /// App Nap 挂起时通知被合并延迟。注意 selector 必须是四段式（含 userInfo:）——
    /// NSDistributedNotificationCenter 无三段式变体，写错即 unrecognized selector
    /// 异常（release panic=abort 表现为闪退，有崩溃复现测试兜底）。
    pub fn post(name: &str, sender: &str) {
        let name = NSString::from_str(name);
        let sender = NSString::from_str(sender);
        unsafe {
            let _: () = objc2::msg_send![
                center(),
                postNotificationName: &*name,
                object: &*sender,
                userInfo: core::ptr::null_mut::<AnyObject>(),
                deliverImmediately: true
            ];
        }
    }

    /// 注册观察（进程生命周期，幂等：已注册则忽略——当前单一消费者模型，多消费者时改 Vec）。
    /// `sender` 为要观察的发布者标识（通常是对端变体的 bundle id），按 object 过滤。
    pub fn observe(name: &str, sender: &str, handler: Arc<dyn Fn() + Send + Sync>) {
        let mut guard = lock_or_recover(&OBSERVER);
        if guard.is_some() {
            return;
        }
        let block: block2::RcBlock<dyn Fn(*mut AnyObject)> = block2::RcBlock::new(move |_| {
            handler();
        });
        let name = NSString::from_str(name);
        let sender = NSString::from_str(sender);
        unsafe {
            let observer: *mut AnyObject = objc2::msg_send![
                center(),
                addObserverForName: &*name,
                object: &*sender,
                queue: core::ptr::null_mut::<AnyObject>(),
                usingBlock: &*block
            ];
            if !observer.is_null() {
                *guard = Some(ObserverEntry { observer, block });
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    use std::sync::Arc;

    pub fn post(_name: &str, _sender: &str) {}
    pub fn observe(_name: &str, _sender: &str, _handler: Arc<dyn Fn() + Send + Sync>) {}
}

pub fn post(name: &str, sender: &str) {
    inner::post(name, sender);
}

/// 注册须在主线程（通知投递挂主 runloop，与 frontmost_watcher 同范式）。
pub fn observe_on_main(
    app: &tauri::AppHandle,
    name: &str,
    sender: &str,
    handler: std::sync::Arc<dyn Fn() + Send + Sync>,
) {
    let name = name.to_string();
    let sender = sender.to_string();
    let fallback = (name.clone(), sender.clone());
    let scheduled = app.run_on_main_thread(move || {
        inner::observe(&name, &sender, handler);
    });
    if scheduled.is_err() {
        // app 退出等极端情况：直接注册（无主 runloop 则收不到，无泄漏无危害）
        inner::observe(&fallback.0, &fallback.1, std::sync::Arc::new(|| {}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 后台线程 post 的崩溃复现（tokio worker 调用路径）：崩溃则测试进程 abort。
    #[test]
    fn post_from_background_thread_does_not_crash() {
        let t = std::thread::spawn(|| {
            post("com.litiantao.voidnix.test-notify", "test-sender");
        });
        t.join().unwrap();
        post("com.litiantao.voidnix.test-notify", "test-sender");
    }
}
