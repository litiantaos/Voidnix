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

#[allow(dead_code)]
fn seconds_to_ticks(secs: u64) -> u64 {
    let mut tb = TimebaseInfo::default();
    // SAFETY: mach_timebase_info 为 Mach C API，填充 TimebaseInfo 结构
    unsafe { mach_timebase_info(&mut tb as *mut _) };
    // ns = ticks * numer / denom → ticks = ns * denom / numer
    secs * 1_000_000_000 * tb.denom as u64 / tb.numer as u64
}

/// 查找本 app 的 WebContent XPC 进程，返回其 physical footprint（字节）。
///
/// WebKit XPC 进程由 launchd 托管（PPID=1），不能用 PPID 关联。
/// 用启动时间下限关联：所有在主进程之后创建的 WebContent 进程视为本 app 的子进程。
/// **不设上限**——navigate 重载或 crash 恢复后创建的新 WebContent 进程启动时间远晚于
/// 主进程，上限窗口会漏检（曾导致 2.2G 进程不触发重载）。
/// 取符合条件的 WebContent 中 footprint 最大的（多实例时取最重者）。
pub fn webcontent_footprint() -> Option<u64> {
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
