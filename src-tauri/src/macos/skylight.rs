//! SkyLight (CoreGraphics private) bridge：跨 Space 窗口迁移。
//!
//! macOS 没有公开 API 把已存在的 NSWindow 强制迁移到另一个 Space。
//! `collectionBehavior` 仅描述未来行为，对已 ordered 的窗口无效；`orderOut +
//! orderFrontRegardless` 也不解除内部 _spaceID 绑定——窗口会回到原 Space 显示。
//!
//! 唯一可靠的迁移途径是 SkyLight (SLS, 旧名 CGS) 私有 API，所有专业 macOS
//! 工具（Yabai/AltTab/Hammerspoon/Rectangle）都在用，跨 macOS 13/14/15 稳定。
//!
//! 本模块仅暴露一个高层入口：[`move_window_to_active_space`]——把指定 NSWindow
//! 迁移到当前 active Space。其余 API 仅作内部辅助使用。
//!
//! 项目 `tauri.conf.json` 已声明 `macOSPrivateApi: true`，说明项目本就走私有 API
//! 路径，无 App Store 审核约束。

#![cfg(target_os = "macos")]

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use std::ffi::c_void;

// SLS / CGS 私有符号（来自 yabai src/misc/extern.h + NUIKit/CGSInternal）。
// 链接指令由 build.rs 处理（`-F /System/Library/PrivateFrameworks -framework SkyLight`）。
#[allow(dead_code)]
extern "C" {
    fn SLSMainConnectionID() -> i32;
    fn SLSCopyManagedDisplaySpaces(cid: i32) -> CFArrayRef;
    fn SLSCopySpacesForWindows(cid: i32, selector: i32, window_list: CFArrayRef) -> CFArrayRef;
    // 把窗口附加到目标 Space 列表（保留原绑定）。
    // 与 SLSMoveWindowsToManagedSpace 不同：Move 是替换 + 仅适用 user space；
    // Add 是叠加 + 适用任何 Space 类型（包括全屏）。
    fn SLSAddWindowsToSpaces(cid: i32, windows: CFArrayRef, spaces: CFArrayRef);
    fn SLSRemoveWindowsFromSpaces(cid: i32, windows: CFArrayRef, spaces: CFArrayRef);
}

/// 把指定 windowNumber 的窗口附加到当前 active Space。
///
/// 用 SLSAddWindowsToSpaces 直接修改窗口的 Space 归属，绕过 collectionBehavior
/// 的异步生效问题。普通桌面和全屏 Space 都支持。
///
/// `previous_spaces`：调用前窗口所在的 Space ID 列表，用于 exit_impl 时
/// SLSRemoveWindowsFromSpaces 恢复（避免窗口残留在多个 Space 上）。
///
/// 调用方必须在主线程执行。返回原 Space ID 列表。
pub fn move_window_to_active_space(
    window_number: i64,
    _ns_window_ptr: *mut c_void,
) -> Vec<u64> {
    if window_number <= 0 { return Vec::new(); }

    let cid = unsafe { SLSMainConnectionID() };
    let Some(active_sid) = current_active_space_id(cid) else {
        return Vec::new();
    };

    let wid_num = CFNumber::from(window_number as i32);
    let wid_array = CFArray::from_CFTypes(&[wid_num]);

    // 记录窗口当前 Space 列表（selector 7 = AllSpaces）
    let prev_spaces = {
        let raw = unsafe { SLSCopySpacesForWindows(cid, 7, wid_array.as_concrete_TypeRef()) };
        if raw.is_null() {
            Vec::new()
        } else {
            let arr: CFArray<CFNumber> = unsafe { CFArray::wrap_under_create_rule(raw) };
            let mut v = Vec::with_capacity(arr.len() as usize);
            for i in 0..arr.len() {
                if let Some(n) = arr.get(i) {
                    if let Some(x) = n.to_i64() { v.push(x as u64); }
                }
            }
            v
        }
    };

    // 把窗口附加到目标 Space
    let sid_num = CFNumber::from(active_sid as i64);
    let sid_array = CFArray::from_CFTypes(&[sid_num]);
    unsafe {
        SLSAddWindowsToSpaces(cid, wid_array.as_concrete_TypeRef(), sid_array.as_concrete_TypeRef());
    }

    // 从原 Space 移除（避免残留）。如果原 Space 与目标相同则跳过。
    let to_remove: Vec<u64> = prev_spaces.iter().copied().filter(|s| *s != active_sid).collect();
    if !to_remove.is_empty() {
        let nums: Vec<CFNumber> = to_remove.iter().map(|&s| CFNumber::from(s as i64)).collect();
        let arr = CFArray::from_CFTypes(&nums);
        unsafe {
            SLSRemoveWindowsFromSpaces(cid, wid_array.as_concrete_TypeRef(), arr.as_concrete_TypeRef());
        }
    }

    prev_spaces
}

/// 便利入口：从 Tauri WebviewWindow 提取 NSWindow 信息并迁移到当前 active Space。
///
/// 调用方必须在主线程执行。返回原 Space ID 列表。
pub fn move_webview_window_to_active_space(window: &tauri::WebviewWindow) -> Vec<u64> {
    use objc2_app_kit::NSWindow;

    let Ok(raw) = window.ns_window() else { return Vec::new() };
    let ns_window_ptr = raw.cast::<NSWindow>() as *mut c_void;

    let window_number: objc2_foundation::NSInteger = unsafe {
        objc2::msg_send![raw.cast::<NSWindow>().as_ref().unwrap(), windowNumber]
    };

    move_window_to_active_space(window_number as i64, ns_window_ptr)
}

/// 枚举 SLSCopyManagedDisplaySpaces，找到 `Current Space` 字段对应的 sid。
///
/// 数据结构（参考 puffnfresh/4053980 + yabai 实现）：
/// ```text
/// [
///   { "Display Identifier": "Main", "Spaces": [...], "Current Space": { "id64": 12345, ... } },
///   { "Display Identifier": "<UUID>", ... }
/// ]
/// ```
///
/// 多显示器场景下取首个 display 的 Current Space。Voidnix 截屏覆盖层只针对主屏，
/// 多显示器适配是另一议题。
fn current_active_space_id(cid: i32) -> Option<u64> {
    let raw = unsafe { SLSCopyManagedDisplaySpaces(cid) };
    if raw.is_null() {
        return None;
    }
    let displays: CFArray<CFDictionary<*const c_void, *const c_void>> =
        unsafe { CFArray::wrap_under_create_rule(raw) };

    if displays.is_empty() {
        return None;
    }

    let display = displays.get(0)?;
    let key_current = CFString::from_static_string("Current Space");
    let key_id64 = CFString::from_static_string("id64");

    let current_space = lookup_dict(&display, &key_current)?;
    let sid_value = lookup(&current_space, &key_id64)?;
    let sid_num = sid_value.downcast::<CFNumber>()?;
    sid_num.to_i64().map(|v| v as u64)
}

/// 从 CFDictionary 按 CFString key 查 CFType 值。
fn lookup(
    dict: &CFDictionary<*const c_void, *const c_void>,
    key: &CFString,
) -> Option<CFType> {
    let key_ptr = key.as_concrete_TypeRef() as *const c_void;
    let value = dict.find(key_ptr)?;
    if value.is_null() {
        return None;
    }
    Some(unsafe { CFType::wrap_under_get_rule(*value as _) })
}

/// 从 CFDictionary 按 CFString key 查嵌套 CFDictionary。
fn lookup_dict(
    dict: &CFDictionary<*const c_void, *const c_void>,
    key: &CFString,
) -> Option<CFDictionary<*const c_void, *const c_void>> {
    lookup(dict, key)?.downcast::<CFDictionary<*const c_void, *const c_void>>()
}
