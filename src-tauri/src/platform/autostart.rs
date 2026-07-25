//! 开机自启：基于 SMAppService（macOS 13+）注册主 app 为系统 Login Item。
//!
//! 注册后会出现在「系统设置 → 通用 → 登录项」列表，用户可在系统层统一开关，
//! 是 macOS 推荐方式（LaunchAgent 写 plist 不进系统设置 UI，已弃用）。
//!
//! framework 经 build.rs 显式链接。
//! 注：mainAppService 依赖 .app bundle identifier，dev 模式跑裸二进制时
//! status 为 NotFound——release .app 正常，此为 macOS 限制。
#![cfg(target_os = "macos")]

use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSString;

/// SMAppServiceStatus.Enabled（<ServiceManagement/SMAppService.h> 枚举：0/1/2/3）。
const STATUS_ENABLED: isize = 1;

/// 获取 SMAppService 单例。
/// 注意：属性在 ObjC 头文件是 `mainAppService`（Swift 别名才是 mainApp），
/// objc runtime selector 必须用 mainAppService，用 mainApp 会找不到方法。
fn main_app_service() -> *mut AnyObject {
    unsafe {
        let cls = AnyClass::get(c"SMAppService").expect("SMAppService 类不可用");
        objc2::msg_send![cls, mainAppService]
    }
}

/// 提取 NSError 的 localizedDescription 为 Rust String。
unsafe fn ns_error_description(err: *mut AnyObject) -> String {
    if err.is_null() {
        return String::from("未知错误");
    }
    let desc: *mut NSString = objc2::msg_send![err, localizedDescription];
    if desc.is_null() {
        return String::from("未知错误");
    }
    (*desc).to_string()
}

/// 是否已注册并启用（status == Enabled）。反映系统真实状态。
pub fn is_enabled() -> bool {
    let service = main_app_service();
    unsafe {
        let status: isize = objc2::msg_send![service, status];
        status == STATUS_ENABLED
    }
}

/// 注册开机自启。成功返回 Ok(())；失败返回 macOS NSError 描述。
pub fn enable() -> Result<(), String> {
    autoreleasepool(|_| {
        let service = main_app_service();
        unsafe {
            let mut err: *mut AnyObject = std::ptr::null_mut();
            let ok: bool = objc2::msg_send![service, registerAndReturnError: &mut err];
            if ok {
                Ok(())
            } else {
                Err(ns_error_description(err))
            }
        }
    })
}

/// 取消注册。成功返回 Ok(())；失败返回 macOS NSError 描述。
pub fn disable() -> Result<(), String> {
    autoreleasepool(|_| {
        let service = main_app_service();
        unsafe {
            let mut err: *mut AnyObject = std::ptr::null_mut();
            let ok: bool = objc2::msg_send![service, unregisterAndReturnError: &mut err];
            if ok {
                Ok(())
            } else {
                Err(ns_error_description(err))
            }
        }
    })
}
