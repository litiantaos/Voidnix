//! macOS 选中文本提取（AX 系统级 API）。
//!
//! 与 platform::input（键盘注入）+ platform::pasteboard（剪贴板操作）配合使用。
//! 划词取词流程：try_ax → 失败则 input::post_combo("cmd+c") → poll_clipboard。

use std::ffi::{c_void, CStr, CString};
use std::time::{Duration, Instant};

use crate::platform::pasteboard;

// ── AX system-wide 选中文本提取 ─────────────────────────

#[cfg(target_os = "macos")]
mod ax {
    use super::*;

    type AXUIElementRef = *mut c_void;
    type AXError = i32;
    const AX_ERROR_SUCCESS: AXError = 0;
    const CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    /// M-rs3：缓存 system-wide AXUIElement。
    /// AXUIElementSetMessagingTimeout 是 per-element 设置（非进程级全局），
    /// 故 init_timeout 创建-设-释放后 timeout 即失效。改为进程生命期缓存 element，
    /// 让所有 get_selected_text 复用同一已设 timeout 的句柄。
    struct SystemWideAx(AXUIElementRef);
    // SAFETY: AXUIElementRef 是 Apple AX API 的不可变句柄，跨线程读取安全
    unsafe impl Send for SystemWideAx {}
    unsafe impl Sync for SystemWideAx {}

    static SYSTEM_WIDE: std::sync::OnceLock<SystemWideAx> = std::sync::OnceLock::new();

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementSetMessagingTimeout(
            element: AXUIElementRef,
            timeout_in_seconds: f32,
        ) -> AXError;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: *mut c_void,
            value: *mut *mut c_void,
        ) -> AXError;
        fn AXIsProcessTrustedWithOptions(options: *mut c_void) -> bool;
        fn CFGetTypeID(cf: *mut c_void) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(boolean: *mut c_void) -> u8;
        fn CFRelease(cf: *mut c_void);
        fn CFStringGetLength(theString: *mut c_void) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(
            theString: *mut c_void,
            buffer: *mut u8,
            bufferSize: isize,
            encoding: u32,
        ) -> bool;
        fn CFStringCreateWithCString(
            alloc: *mut c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> *mut c_void;
    }

    /// 取（首次创建并设 timeout）缓存的 system-wide element。
    /// 返回的 AXUIElementRef 进程生命期常驻，无需调用方释放。
    fn system_wide() -> AXUIElementRef {
        SYSTEM_WIDE
            .get_or_init(|| {
                // SAFETY: AXUIElementCreateSystemWide 无副作用依赖，可跨线程调用
                let sys = unsafe { AXUIElementCreateSystemWide() };
                if !sys.is_null() {
                    unsafe { AXUIElementSetMessagingTimeout(sys, 0.05) };
                }
                SystemWideAx(sys)
            })
            .0
    }

    pub fn init_timeout() {
        // M-rs3：触发 system_wide 初始化（首次调用设 timeout 并缓存 element）
        system_wide();
    }

    fn cf_str(s: &str) -> *mut c_void {
        let Ok(c) = CString::new(s) else {
            return std::ptr::null_mut();
        };
        unsafe {
            CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), CF_STRING_ENCODING_UTF8)
        }
    }

    fn cf_to_string(cf: *mut c_void) -> Option<String> {
        if cf.is_null() {
            return None;
        }
        unsafe {
            let len = CFStringGetLength(cf);
            let max = CFStringGetMaximumSizeForEncoding(len, CF_STRING_ENCODING_UTF8) + 1;
            let mut buf = vec![0u8; max as usize];
            if CFStringGetCString(cf, buf.as_mut_ptr(), max, CF_STRING_ENCODING_UTF8) {
                CStr::from_ptr(buf.as_ptr() as *const i8)
                    .to_str()
                    .ok()
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
    }

    unsafe fn copy_attr(element: AXUIElementRef, attr: &str) -> Option<*mut c_void> {
        let key = cf_str(attr);
        if key.is_null() {
            return None;
        }
        let mut val: *mut c_void = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(element, key, &mut val);
        CFRelease(key);
        if err == AX_ERROR_SUCCESS && !val.is_null() {
            Some(val)
        } else {
            None
        }
    }

    pub fn get_selected_text() -> Option<String> {
        if !unsafe { AXIsProcessTrustedWithOptions(std::ptr::null_mut()) } {
            return None;
        }
        let sys = system_wide();
        if sys.is_null() {
            return None;
        }
        unsafe {
            // M-rs3：复用缓存 system-wide element（已设 timeout），不再每次创建+释放
            let focused = copy_attr(sys, "AXFocusedUIElement")?;
            let selected = copy_attr(focused, "AXSelectedText");
            CFRelease(focused);
            let selected = selected?;
            if CFGetTypeID(selected) != CFStringGetTypeID() {
                CFRelease(selected);
                return None;
            }
            let text = cf_to_string(selected);
            CFRelease(selected);
            text.filter(|s| !s.trim().is_empty())
        }
    }

    /// 文本输入区角色集合：标准 AppKit 可编辑控件角色。终端 / IDE 编辑器
    /// （Terminal、VS Code 等）的正文聚焦元素同为 AXTextArea。
    const TEXT_INPUT_ROLES: &[&str] = &[
        "AXTextField",
        "AXSecureTextField",
        "AXSearchField",
        "AXComboBox",
        "AXTextArea",
    ];

    /// 明确非输入区的角色集合（聚焦在这些元素上时 Cmd+V 落空）：Finder/边栏列表
    /// （AXOutline/AXList）、表格（AXTable/AXRow/AXCell）、按钮、静态文本、图片、
    /// 网页正文（AXWebArea）。
    const NON_TEXT_ROLES: &[&str] = &[
        "AXOutline",
        "AXList",
        "AXTable",
        "AXRow",
        "AXCell",
        "AXButton",
        "AXStaticText",
        "AXImage",
        "AXWebArea",
    ];

    /// 角色三态分类（纯函数，pub(super) 供本文件单测）：Some(true) 输入区 /
    /// Some(false) 明确非输入区 / None 未知。未知是安全缺省——自绘引擎（Zed 等
    /// GPUI/Qt 应用）AX 树只到 AXWindow/AXGroup 容器层（实测 Zed 窗口内无任何
    /// 文本元素，AXManualAccessibility 也唤不起），误判为非输入区会让编辑器内的
    /// 粘贴回退成复制，宁可维持粘贴。
    pub(super) fn classify_role(role: &str) -> Option<bool> {
        if TEXT_INPUT_ROLES.contains(&role) {
            return Some(true);
        }
        if NON_TEXT_ROLES.contains(&role) {
            return Some(false);
        }
        None
    }

    /// 查询指定 app 聚焦元素是否为文本输入区（可接收键盘文本输入）。
    /// Some(true/false) = 判定成功（输入区 / 明确非输入区）；None = 无法判定
    /// （pid 无效 / 无 AX 权限 / app 根元素创建失败 / 目标无聚焦元素 / 角色未知），
    /// 调用方按自身默认行为处理。
    pub fn focused_element_editable(pid: i32) -> Option<bool> {
        if pid <= 0 {
            return None;
        }
        if !unsafe { AXIsProcessTrustedWithOptions(std::ptr::null_mut()) } {
            return None;
        }
        // SAFETY: AXUIElementCreateApplication 按 pid 建立目标 app 根元素
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return None;
        }
        // 目标 app 无响应时快速失败（同步命令路径调用，勿长时间阻塞主线程）
        unsafe { AXUIElementSetMessagingTimeout(app, 0.05) };
        // SAFETY: classify_focused_editable 仅要求 app 为有效 app 级元素
        let editable = unsafe { classify_focused_editable(app) };
        // SAFETY: app 由本函数创建，释放所有权
        unsafe { CFRelease(app) };
        editable
    }

    /// # Safety
    /// `app` 须为有效 AXUIElementRef（目标 app 根元素）。
    unsafe fn classify_focused_editable(app: AXUIElementRef) -> Option<bool> {
        let focused = copy_attr(app, "AXFocusedUIElement")?;
        // SAFETY: focused 为有效元素句柄
        let result = unsafe { classify_element(focused) };
        CFRelease(focused);
        result
    }

    /// # Safety
    /// `element` 须为有效 AXUIElementRef。
    unsafe fn classify_element(element: AXUIElementRef) -> Option<bool> {
        // SAFETY: element_role 内释放自建 CF 值
        let role = unsafe { element_role(element) };
        // 输入区角色快路径：跳过后续属性查询
        if role.as_deref().and_then(classify_role) == Some(true) {
            return Some(true);
        }
        // AXEditable 自报可编辑：自绘编辑器角色不规范时的兜底通道
        // SAFETY: focused_attr_editable 内释放自建 CF 值
        if unsafe { focused_attr_editable(element) } {
            return Some(true);
        }
        // 非输入区角色 → Some(false)；未知/缺失角色（AXWindow/AXGroup 等容器层）→ None
        role.as_deref().and_then(classify_role)
    }

    /// # Safety
    /// `element` 须为有效 AXUIElementRef。
    unsafe fn element_role(element: AXUIElementRef) -> Option<String> {
        copy_attr(element, "AXRole").and_then(|role| {
            // SAFETY: role 为 copy_attr 返回的拥有所有权 CF 值
            let s = cf_to_string(role);
            CFRelease(role);
            s
        })
    }

    /// # Safety
    /// `element` 须为有效 AXUIElementRef。
    unsafe fn focused_attr_editable(element: AXUIElementRef) -> bool {
        copy_attr(element, "AXEditable")
            .map(|v| {
                // SAFETY: v 为 copy_attr 返回的拥有所有权 CF 值
                let val = CFGetTypeID(v) == CFBooleanGetTypeID() && CFBooleanGetValue(v) != 0;
                CFRelease(v);
                val
            })
            .unwrap_or(false)
    }
}

// ── 公开 API ──────────────────────────────────────────

/// 初始化 AX 超时设置（setup 阶段调用一次）。
#[cfg(target_os = "macos")]
pub fn init_ax_timeout() {
    ax::init_timeout();
}

/// Layer 1：通过 AX 提取系统级选中文本（后台线程调用）。
#[cfg(target_os = "macos")]
pub fn try_ax() -> Option<String> {
    ax::get_selected_text()
}

/// 判定指定 app 当前聚焦元素是否为文本输入区（可接收键盘文本输入）。
/// Some(false) 仅对明确非输入区角色（列表/表格/按钮/静态文本/网页正文等）返回；
/// None = 无法判定（权限缺失 / 目标无响应 / 无聚焦元素 / 未知角色——自绘引擎
/// 只暴露 AXWindow/AXGroup 容器层），调用方取自身默认行为。
/// clipboard 粘贴回退复制等消费方使用。
#[cfg(target_os = "macos")]
pub fn focused_element_editable(pid: i32) -> Option<bool> {
    ax::focused_element_editable(pid)
}

/// Layer 2：在后台线程轮询剪贴板变化（配合 input::post_combo("cmd+c") 使用）。
///
/// 流程：post_combo("cmd+c") 注入复制后调用此函数 → 等待 changeCount 变化 → 读取文本 → 恢复原剪贴板。
#[cfg(target_os = "macos")]
pub fn poll_clipboard(snap: pasteboard::PasteboardSnapshot) -> String {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        std::thread::sleep(Duration::from_millis(10));
        if pasteboard::change_count() != snap.change_count {
            let mut last = pasteboard::change_count();
            let mut stable = Instant::now();
            while stable.elapsed() < Duration::from_millis(30) {
                std::thread::sleep(Duration::from_millis(5));
                let cur = pasteboard::change_count();
                if cur != last {
                    last = cur;
                    stable = Instant::now();
                }
            }
            // 判断必须在 restore 之前：restore 会把剪贴板恢复到快照状态，
            // 此后再检测文件类型必然为 false。
            // 剪贴板含文件 URL 时，public.utf8-plain-text 携带的是文件名副作用，
            // 不属于选中文本 → 视为空。
            let is_file = pasteboard::has_file_url();
            let text = pasteboard::read_text().unwrap_or_default();
            let text = text.trim().to_string();
            pasteboard::restore(&snap);
            return if is_file { String::new() } else { text };
        }
    }
    pasteboard::restore(&snap);
    String::new()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::ax::classify_role;

    #[test]
    fn text_input_roles_classified_editable() {
        // 标准可编辑控件 + 终端 / IDE 正文（AXTextArea）
        for role in [
            "AXTextField",
            "AXSecureTextField",
            "AXSearchField",
            "AXComboBox",
            "AXTextArea",
        ] {
            assert_eq!(classify_role(role), Some(true), "{role} 应为输入区");
        }
    }

    #[test]
    fn definite_non_text_roles_classified_not_input() {
        // Finder/边栏列表、表格、按钮、静态文本、图片、网页正文
        for role in [
            "AXOutline",
            "AXList",
            "AXTable",
            "AXRow",
            "AXCell",
            "AXButton",
            "AXStaticText",
            "AXImage",
            "AXWebArea",
        ] {
            assert_eq!(classify_role(role), Some(false), "{role} 应为非输入区");
        }
    }

    #[test]
    fn container_and_unknown_roles_stay_undetermined() {
        // 自绘引擎（Zed 等 GPUI/Qt 应用）AX 树只到容器层：误判非输入区会让
        // 编辑器内粘贴回退成复制，未知角色必须维持 None（调用方默认粘贴）
        for role in [
            "AXWindow",
            "AXGroup",
            "AXScrollArea",
            "AXSplitGroup",
            "AXUnknownRole",
            "",
        ] {
            assert_eq!(classify_role(role), None, "{role} 应为未知");
        }
    }
}
