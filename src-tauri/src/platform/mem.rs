//! macOS 进程内存查询原语（proc_pid_rusage）。
//! 查询 WebContent XPC 进程的 physical footprint，供 webview 内存阈值重载决策。

use libc::{c_int, c_void};

/// `RUSAGE_INFO_V1` flavor 值
const RUSAGE_INFO_V1: c_int = 1;

extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    fn mach_timebase_info(info: *mut TimebaseInfo) -> c_int;
}

#[repr(C)]
#[derive(Default)]
struct TimebaseInfo {
    numer: u32,
    denom: u32,
}

/// `rusage_info_v1`：含 `ri_phys_footprint` + `ri_proc_start_abstime`（96 字节）。
#[repr(C)]
#[derive(Default)]
struct RUsageInfoV1 {
    ri_uuid: [u8; 16],
    ri_user_time: u64,
    ri_system_time: u64,
    ri_pkg_idle_wkups: u64,
    ri_interrupt_wkups: u64,
    ri_pageins: u64,
    ri_wired_size: u64,
    ri_resident_size: u64,
    ri_phys_footprint: u64,
    ri_proc_start_abstime: u64,
    ri_proc_exit_abstime: u64,
}

fn get_rusage_v1(pid: c_int) -> Option<RUsageInfoV1> {
    let mut info = RUsageInfoV1::default();
    // SAFETY: proc_pid_rusage 为 libproc C API，buffer 为正确大小的栈结构
    let ret = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V1, &mut info as *mut _ as *mut c_void) };
    if ret == 0 {
        Some(info)
    } else {
        None
    }
}

/// 单进程 physical footprint（字节）。
pub fn footprint_by_pid(pid: c_int) -> Option<u64> {
    get_rusage_v1(pid).map(|i| i.ri_phys_footprint)
}

/// 从窗口的 WKWebView 实例直接取其 WebContent 进程 pid。
///
/// `_webProcessIdentifier` SPI（respondsToSelector 门控；页面未加载 / WebContent
/// 未 spawn 时返回 0 过滤）。**主线程调用**。这是 WebContent 归属的唯一精确途径：
/// 外部进程级归属（rpid/audit_token/环境变量/文件映射）均被 SIP 封锁，启动时间
/// 启发式会把其它 app 的 WebContent 吃进来。
///
/// 两个实现要点（均经 dev 真机验证）：
/// - `ns_view()` 返回 contentView 容器而非 WKWebView 本体（tao 层语义）；wry 将
///   WKWebView 自定义子类（WryWebView）作为其直接子视图挂载，遍历子视图以
///   selector 响应为判据定位——普通 NSView 不响应该 selector，零类引用依赖。
/// - 调用走 methodForSelector 取 IMP 后按 C ABI 直调：objc2 `msg_send!` 泛型
///   包装在该 SPI + WryWebView 实例上抛 ObjC 异常（foreign exception 直接
///   abort 进程），IMP 直调稳定（与 swift 探针同姿势）。
pub fn webview_webcontent_pid(window: &tauri::WebviewWindow) -> Option<i32> {
    use objc2_app_kit::NSView;

    let ptr = window.ns_view().ok()?;
    let raw = ptr.cast::<NSView>();
    let content = (unsafe { raw.as_ref() })?;
    let sel = objc2::sel!(_webProcessIdentifier);
    unsafe {
        for sv in content.subviews().iter() {
            let responds: bool = objc2::msg_send![&**sv, respondsToSelector: sel];
            if !responds {
                continue;
            }
            let imp: usize = objc2::msg_send![&**sv, methodForSelector: sel];
            if imp == 0 {
                continue;
            }
            type ImpFn =
                unsafe extern "C" fn(*mut objc2::runtime::AnyObject, objc2::runtime::Sel) -> i32;
            let f: ImpFn = std::mem::transmute(imp);
            let pid = f(
                std::ptr::from_ref(&**sv) as *mut objc2::runtime::AnyObject,
                sel,
            );
            if pid > 0 {
                return Some(pid);
            }
        }
    }
    None
}

#[allow(dead_code)]
fn seconds_to_ticks(secs: u64) -> u64 {
    let mut tb = TimebaseInfo::default();
    // SAFETY: mach_timebase_info 为 Mach C API，填充 TimebaseInfo 结构
    unsafe { mach_timebase_info(&mut tb as *mut _) };
    // ns = ticks * numer / denom → ticks = ns * denom / numer
    secs * 1_000_000_000 * tb.denom as u64 / tb.numer as u64
}

/// 扫描口径 fallback：启动时间下限（`>=` 主进程启动）的全系统 WebContent 取最重者。
///
/// WebKit XPC 进程由 launchd 托管（PPID=1），不能用 PPID 关联。**不精确**——其它
/// app 晚于本 app 启动的 WebContent 同样命中（误差方向单一：只会把别人的大占用当
/// 自己的超标、误触发重载，不会漏检自己）。仅作 SPI 不可用时的保守兜底，保留
/// 「不设上限」的动机：navigate 重载或 crash 恢复后创建的新 WebContent 进程启动
/// 时间远晚于主进程，上限窗口会漏检（曾导致 2.2G 进程不触发重载）。
pub fn webcontent_footprint_scan() -> Option<u64> {
    let our_pid = std::process::id() as c_int;
    let our_start = get_rusage_v1(our_pid)?.ri_proc_start_abstime;

    let mut pid_buf = [0i32; 2048];
    let count = unsafe {
        proc_listallpids(
            pid_buf.as_mut_ptr() as *mut c_void,
            (pid_buf.len() * std::mem::size_of::<i32>()) as c_int,
        )
    };
    if count <= 0 {
        return None;
    }

    let mut max_fp: u64 = 0;
    for &pid in pid_buf.iter().take(count as usize) {
        if pid == 0 || pid == our_pid {
            continue;
        }
        let mut path_buf = [0u8; 4096];
        // SAFETY: proc_pidpath 为 libproc C API，buffer 为栈分配
        let path_len = unsafe {
            proc_pidpath(
                pid,
                path_buf.as_mut_ptr() as *mut c_void,
                path_buf.len() as u32,
            )
        };
        if path_len <= 0 {
            continue;
        }
        let path = std::str::from_utf8(&path_buf[..path_len as usize]).unwrap_or("");
        if !path.contains("WebKit.WebContent") {
            continue;
        }
        let Some(info) = get_rusage_v1(pid) else {
            continue;
        };
        if info.ri_proc_start_abstime >= our_start {
            max_fp = max_fp.max(info.ri_phys_footprint);
        }
    }

    (max_fp > 0).then_some(max_fp)
}
