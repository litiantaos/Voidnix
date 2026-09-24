use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::{c_char, c_void};

/// 熄屏补熄节流间隔：合盖停留期间面板可能被通知等重新点亮，按此间隔
/// 复核补熄（`pmset displaysleepnow` 对已灭屏为幂等无操作）。
pub const RESLEEP_INTERVAL: Duration = Duration::from_secs(5);

/// 合盖状态（IORegistry `IOPMrootDomain` 的 `AppleClamshellState`，无公开
/// API 的事实标准读法）。`None` = 读取失败或无盖机型（Mac mini/Studio）。
/// 该属性偶发返回失败，消费方须以闭锁防抖（None 不翻转既有判定）。
pub fn lid_is_closed() -> Option<bool> {
    type MachPort = u32;
    type IoObject = u32;
    type KernReturn = i32;
    const K_IO_MAIN_PORT_DEFAULT: MachPort = 0;
    extern "C" {
        fn IOServiceGetMatchingService(main_port: MachPort, matching: *mut c_void) -> IoObject;
        fn IOServiceMatching(name: *const c_char) -> *mut c_void;
        fn IORegistryEntryCreateCFProperty(
            entry: IoObject,
            key: CFStringRef,
            allocator: *const c_void,
            options: u32,
        ) -> *const c_void;
        fn IOObjectRelease(object: IoObject) -> KernReturn;
    }
    // SAFETY: IOKit C API 惯例调用——IOServiceMatching 返回的 matching dict
    // 由 IOServiceGetMatchingService 消耗无需释放；属性返回 Create 规则
    // CFBoolean，wrap 接管所有权（drop 自动 CFRelease）；entry 由
    // IOObjectRelease 释放
    unsafe {
        let name = c"IOPMrootDomain".as_ptr();
        let service = IOServiceGetMatchingService(K_IO_MAIN_PORT_DEFAULT, IOServiceMatching(name));
        if service == 0 {
            return None;
        }
        let key = CFString::new("AppleClamshellState");
        let prop = IORegistryEntryCreateCFProperty(
            service,
            key.as_concrete_TypeRef(),
            std::ptr::null(),
            0,
        );
        IOObjectRelease(service);
        if prop.is_null() {
            return None;
        }
        let closed = bool::from(CFBoolean::wrap_under_create_rule(prop.cast()));
        Some(closed)
    }
}

/// 是否接有外接显示器（CoreGraphics 活动显示列表中存在非内置屏）。
/// 合盖 + 外接 = clamshell 正常用法，外接屏应保持点亮。
pub fn has_external_display() -> bool {
    type CGDirectDisplayID = u32;
    const K_CG_ERROR_SUCCESS: i32 = 0;
    extern "C" {
        fn CGGetActiveDisplayList(
            max_displays: u32,
            active_displays: *mut CGDirectDisplayID,
            display_count: *mut u32,
        ) -> i32;
        fn CGDisplayIsBuiltin(display: CGDirectDisplayID) -> u32;
    }
    // SAFETY: CoreGraphics C API，缓冲区按容量传入并由调用方栈持有；
    // 超过 8 块屏的机器截断前 8 块判定（外接判定不受影响）
    unsafe {
        let mut ids = [0u32; 8];
        let mut count: u32 = 0;
        if CGGetActiveDisplayList(8, ids.as_mut_ptr(), &mut count) != K_CG_ERROR_SUCCESS {
            return false;
        }
        (0..count as usize).any(|i| CGDisplayIsBuiltin(ids[i]) == 0)
    }
}

/// 立即熄灭所有显示器（`pmset displaysleepnow`，无需 root）。fire-and-forget：
/// 无值得等待的结果。对已灭屏为幂等无操作。
pub fn sleep_displays_now() {
    // SAFETY: 进程 spawn 后即脱离管理（无 stdio 句柄泄漏），僵尸由系统回收
    let _ = Command::new("/usr/bin/pmset")
        .arg("displaysleepnow")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// 熄屏决策（纯函数供单测）：处于「合盖 + 无外接」时需要熄屏——进入该
/// 状态的边沿立即发，停留期经 ``RESLEEP_INTERVAL`` 节流补熄。lid 读 None
/// （AppleClamshellState 偶发 flutter）按 `was_closed` 闭锁延续既有判定，
/// 防 flutter 触发重复边沿。开盖恒不熄。
pub fn screen_sleep_due(
    lid_closed: Option<bool>,
    external: bool,
    was_closed: bool,
    since_last_sleep: Duration,
) -> bool {
    let closed_now = match lid_closed {
        Some(closed) => closed,
        None => was_closed,
    };
    closed_now && !external && (!was_closed || since_last_sleep >= RESLEEP_INTERVAL)
}

/// 启动睡眠 watchdog 的失败分类。
pub enum SleepWatchdogError {
    /// 用户取消了管理员授权弹窗（osascript error -128）
    Cancelled,
    /// 授权通过但命令执行失败，携带 stderr
    Failed(String),
}

/// 启动 root 睡眠 watchdog：经 osascript 管理员授权运行一个后台 shell 循环，
/// 每 2 秒轮询 flag 文件与本进程 pid——flag 出现时上翻 `pmset -a disablesleep 1`、
/// 消失时回落 0（各仅写一次，边沿触发）；app 退出或崩溃（pid 消失）时自动恢复
/// 默认睡眠并清理 flag。每次 app 运行期只需授权一次，之后开关全靠 flag 文件、
/// 零弹窗。watchdog 生命周期绑定 app 进程是崩溃安全的最小机制：无需落盘恢复
/// 债务、无需守护进程，极端场景（断电同时杀死 watchdog）的残留 flag 由下次
/// 启动的 setup 清理。
///
/// 这是全局唯一能实现「合盖 + 无外接显示器 + 电池供电」不休眠的杠杆：
/// `IOPMAssertion` 压不住 clamshell 睡眠路径。flag 驱动而非每次直写，还避免
/// 了与用户手工 `sudo pmset` 的持续互相覆盖（单次覆盖用户可见）。
pub fn spawn_sleep_watchdog(flag: &Path, app_pid: u32) -> Result<(), SleepWatchdogError> {
    let shell = watchdog_shell(&flag.to_string_lossy(), app_pid);
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        shell.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| SleepWatchdogError::Failed(e.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("-128") || stderr.to_lowercase().contains("cancel") {
        return Err(SleepWatchdogError::Cancelled);
    }
    let message = stderr.trim();
    Err(SleepWatchdogError::Failed(if message.is_empty() {
        "osascript exited with error".into()
    } else {
        message.into()
    }))
}

/// watchdog 的 sh 循环体。子 shell 整体后台化（`&` + 全重定向），授权命令立即
/// 返回；循环存活至 app pid 消失，退出前回收 flag 并在持有期间恢复默认睡眠。
/// SET 状态使 pmset 写入只发生在 flag 边沿：外部把 disablesleep 改回去时不会
/// 被无脑重写（与用户手工设置互相尊重），flag 仍在即维持既有状态。
fn watchdog_shell(flag: &str, app_pid: u32) -> String {
    format!(
        "( SET=; while kill -0 {app_pid} 2>/dev/null; do \
         if [ -f {flag} ]; then \
         if [ -z \"$SET\" ]; then /usr/bin/pmset -a disablesleep 1; SET=1; fi; \
         elif [ -n \"$SET\" ]; then /usr/bin/pmset -a disablesleep 0; SET=; fi; \
         sleep 2; done; \
         rm -f {flag}; \
         if [ -n \"$SET\" ]; then /usr/bin/pmset -a disablesleep 0; fi ) \
         </dev/null >/dev/null 2>&1 &",
        app_pid = app_pid,
        flag = shell_single_quoted(flag)
    )
}

/// 单引号包裹一个字面量词传给 sh（路径含空格如 "Application Support"）。
/// 单引号内唯一无法出现的字符是单引号本身，以 `'\''` 断开重开转义。
fn shell_single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 电池供电状态摘要（`pmset -g batt`）。护栏的只读输入。
pub struct BatteryStatus {
    pub percent: u32,
    /// 正在用电池供电（放电中；插电充电/充满均为 false）
    pub on_battery: bool,
}

/// 读取当前电池状态。`None` = 读取失败（无电池的台式机、pmset 异常），
/// 消费方据此保持现状不动作（读不到电量不误杀持有中的任务）。
pub fn read_battery_status() -> Option<BatteryStatus> {
    let output = Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_battery(&String::from_utf8_lossy(&output.stdout))
}

/// 解析 `pmset -g batt` 输出：
/// `Now drawing from 'Battery Power'` 行 = 放电中（插电时为 'AC Power'）；
/// 百分比取首个 `%` 前的连续数字（多电池取第一块，放电判定不受影响）。
fn parse_battery(text: &str) -> Option<BatteryStatus> {
    let on_battery = text
        .lines()
        .any(|l| l.contains("Now drawing from 'Battery Power'"));
    let percent_line = text.lines().find(|l| l.contains('%'))?;
    let idx = percent_line.find('%')?;
    let digits: String = percent_line[..idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let percent = digits.parse().ok()?;
    Some(BatteryStatus {
        percent,
        on_battery,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_spaces_and_apostrophes() {
        assert_eq!(shell_single_quoted("/a b/c"), "'/a b/c'");
        assert_eq!(shell_single_quoted("/us'er/x"), "'/us'\\''er/x'");
    }

    #[test]
    fn screen_sleep_decision_edges_and_throttle() {
        let long = RESLEEP_INTERVAL + Duration::from_secs(1);
        let short = RESLEEP_INTERVAL - Duration::from_secs(1);
        // 开盖恒不熄
        assert!(!screen_sleep_due(Some(false), false, false, long));
        assert!(!screen_sleep_due(Some(false), false, true, long));
        // 合盖 + 无外接：边沿立即熄
        assert!(screen_sleep_due(Some(true), false, false, long));
        // 停留期节流：窗口内不重发，到期补熄（防通知唤醒）
        assert!(!screen_sleep_due(Some(true), false, true, short));
        assert!(screen_sleep_due(Some(true), false, true, long));
        // 合盖 + 外接 = clamshell 正常用法，不熄
        assert!(!screen_sleep_due(Some(true), true, false, long));
        assert!(!screen_sleep_due(Some(true), true, true, long));
        // lid 读 None（AppleClamshellState flutter）：按闭锁延续，不产生伪边沿
        assert!(!screen_sleep_due(None, false, false, long));
        assert!(!screen_sleep_due(None, false, true, short));
        assert!(screen_sleep_due(None, false, true, long));
    }

    #[test]
    fn battery_parse_covers_power_states() {
        let ac = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true\n";
        let b = parse_battery(ac).unwrap();
        assert_eq!((b.percent, b.on_battery), (100, false));

        let batt = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t42%; discharging; 2:11 remaining present: true\n";
        let b = parse_battery(batt).unwrap();
        assert_eq!((b.percent, b.on_battery), (42, true));

        // 插电充电中：AC 供电源，百分比低也不算放电
        let charging = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t5%; charging; 0:45 remaining present: true\n";
        let b = parse_battery(charging).unwrap();
        assert_eq!((b.percent, b.on_battery), (5, false));

        // 无百分比行（异常输出）→ None，消费方保持现状
        assert!(parse_battery("Now drawing from 'Battery Power'\n").is_none());
        assert!(parse_battery("").is_none());
    }

    #[test]
    fn watchdog_shell_shape() {
        let cmd = watchdog_shell("/Users/x/Library/Application Support/a/sleep.flag", 4242);
        // flag 路径含空格必须以单引号传入
        assert!(cmd.contains("-f '/Users/x/Library/Application Support/a/sleep.flag'"));
        // pid 监视 + 边沿写 + 退出恢复 + 后台化，四要素齐备
        assert!(cmd.contains("kill -0 4242"));
        assert!(cmd.contains("disablesleep 1; SET=1"));
        assert!(cmd.contains("disablesleep 0; SET=;"));
        assert!(cmd.ends_with("</dev/null >/dev/null 2>&1 &"));
        // 关闭恢复须以 SET 状态守卫：从未持有时不写 0（尊重用户手工设置）
        assert!(cmd.contains("if [ -n \"$SET\" ]; then /usr/bin/pmset -a disablesleep 0; fi )"));
    }
}
