//! CGWindowList 共享封装：枚举可见窗口（排除桌面）。
//! screenshot（智能吸附选区）与 window-manager（找最顶层窗口 pid）共用，
//! 消除两处 extern 声明 + 常量 + 调用样板。字段提取由调用方按需进行。

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use std::ffi::c_void;

type CGWindowListOption = u32;
type CGWindowID = u32;

const ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;
const EXCLUDE_DESKTOP: CGWindowListOption = 1 << 4;

extern "C" {
    fn CGWindowListCopyWindowInfo(
        option: CGWindowListOption,
        relative_to_window: CGWindowID,
    ) -> CFArrayRef;
}

/// 枚举所有可见窗口（排除桌面元素）。返回 Create 规则 CFArray（已 wrap 接管所有权）。
/// None = CoreGraphics 返回空（极少见）。调用方按需提取 layer/pid/bounds/alpha/name 等字段。
pub fn copy_on_screen_windows() -> Option<CFArray<CFDictionary<*const c_void, *const c_void>>> {
    // SAFETY: CGWindowListCopyWindowInfo 为 CoreGraphics C API，option 为合法位掩码，
    // relative_to_window=0（无相对窗口）；返回 Create 规则 CFArray，null 检查后由
    // wrap_under_create_rule 接管所有权
    let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: raw 由 CGWindowListCopyWindowInfo 返回（Create 规则，已 null 检查），
    // wrap_under_create_rule 接管所有权，array 释放时自动 CFRelease
    Some(unsafe { CFArray::wrap_under_create_rule(raw) })
}

/// 从 CFDictionary 按 key 取 CFType（Get 规则，不获取所有权）。字段提取公共 helper。
#[allow(dead_code)]
pub fn dict_lookup(
    dict: &CFDictionary<*const c_void, *const c_void>,
    key: &core_foundation::string::CFString,
) -> Option<CFType> {
    let ptr = key.as_concrete_TypeRef() as *const c_void;
    let v = dict.find(ptr)?;
    if v.is_null() {
        return None;
    }
    // SAFETY: *v 已非空校验；wrap_under_get_rule 遵循 CF Get 规则（不获取所有权）
    Some(unsafe { CFType::wrap_under_get_rule(*v as _) })
}
