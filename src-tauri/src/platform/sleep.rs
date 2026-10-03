use std::process::{Command, Stdio};
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
use std::sync::OnceLock;

mod cg {
    //! CoreGraphics 显示列表共享 extern（has_external_display 与 builtin_display_id 共用，
    //! 消除重复声明）。
    pub type CGDirectDisplayID = u32;
    pub const K_CG_ERROR_SUCCESS: i32 = 0;

    extern "C" {
        pub fn CGGetOnlineDisplayList(
            max_displays: u32,
            online_displays: *mut CGDirectDisplayID,
            display_count: *mut u32,
        ) -> i32;
        pub fn CGGetActiveDisplayList(
            max_displays: u32,
            active_displays: *mut CGDirectDisplayID,
            display_count: *mut u32,
        ) -> i32;
        pub fn CGDisplayIsBuiltin(display: CGDirectDisplayID) -> u32;
    }
}

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
    // SAFETY: CoreGraphics C API，缓冲区按容量传入并由调用方栈持有；
    // 超过 8 块屏的机器截断前 8 块判定（外接判定不受影响）
    unsafe {
        let mut ids = [0u32; 8];
        let mut count: u32 = 0;
        if cg::CGGetActiveDisplayList(8, ids.as_mut_ptr(), &mut count) != cg::K_CG_ERROR_SUCCESS {
            return false;
        }
        (0..count as usize).any(|i| cg::CGDisplayIsBuiltin(ids[i]) == 0)
    }
}

/// 内置面板的 display id：优先 online 列表（合盖后系统可能仍保持 online），
/// 退 active 列表，再退进程内缓存——合盖后两个 CG 列表都会摘掉内置屏
/// （Keepresso 实测），无缓存则合盖期 get/set 无目标。
fn builtin_display_id() -> Option<u32> {
    static LAST_BUILTIN: AtomicU32 = AtomicU32::new(0);
    // SAFETY: CG C API，栈缓冲；两个列表 API 签名一致由 fn 指针复用
    unsafe fn first_builtin(
        list: unsafe extern "C" fn(u32, *mut u32, *mut u32) -> i32,
    ) -> Option<u32> {
        let mut ids = [0u32; 8];
        let mut count: u32 = 0;
        if list(8, ids.as_mut_ptr(), &mut count) != cg::K_CG_ERROR_SUCCESS {
            return None;
        }
        (0..count as usize)
            .map(|i| ids[i])
            .find(|id| cg::CGDisplayIsBuiltin(*id) != 0)
    }
    let live = unsafe {
        first_builtin(cg::CGGetOnlineDisplayList)
            .or_else(|| first_builtin(cg::CGGetActiveDisplayList))
    };
    match live {
        Some(id) => {
            LAST_BUILTIN.store(id, AtomicOrdering::Relaxed);
            Some(id)
        }
        None => match LAST_BUILTIN.load(AtomicOrdering::Relaxed) {
            0 => None,
            cached => Some(cached),
        },
    }
}

/// DisplayServices 私有 framework 符号（dlopen 一次按需 dlsym）。
/// 该通道为 Keepresso 生产验证的同款实现（`KPBrightness.m`）。
/// 返回裸符号地址，判空调用方负责。
fn display_services_symbol(name: &std::ffi::CStr) -> *mut c_void {
    static HANDLE: OnceLock<usize> = OnceLock::new();
    extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    let handle = *HANDLE.get_or_init(|| {
        // SAFETY: dlopen 对已加载 framework 返回同句柄（引用计数），
        // RTLD_LAZY=1 延迟绑定；句柄以 usize 存入 OnceLock（裸指针非 Sync）
        unsafe {
            dlopen(
                c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices"
                    .as_ptr(),
                1,
            ) as usize
        }
    }) as *mut c_void;
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: handle 来自 dlopen 成功返回
    unsafe { dlsym(handle, name.as_ptr()) }
}

/// 内置面板用户亮度（0..1），经 DisplayServices 私有 framework 读写。
/// 通道选择依据（均实测于本机新背光架构）：IORegistry 直写
/// `AppleARMBacklight.brightness`（无论 32/64 位 CFNumber）与
/// `CoreDisplay_Display_SetUserBrightness` 均只改报告值、不驱动实际背光；
/// DisplayServices 为 Keepresso 生产验证通道。读回值在新架构下同样是
/// 服务层报告，写入有效性以物理观察为准（tick 诊断日志带写入返回值）。
/// 符号缺失或无内置屏返回 None，消费方据此退化到显示睡眠策略。
pub fn builtin_brightness() -> Option<f32> {
    type GetFn = unsafe extern "C" fn(u32, *mut f32) -> i32;
    let sym = display_services_symbol(c"DisplayServicesGetBrightness");
    if sym.is_null() {
        return None;
    }
    // SAFETY: 符号非空，按 DisplayServicesGetBrightness 的真实签名转译
    let get: GetFn = unsafe { std::mem::transmute(sym) };
    let id = builtin_display_id()?;
    let mut level = 0f32;
    // SAFETY: get 为已解析符号，id 来自 CG 列表/缓存
    (unsafe { get(id, &mut level) } == 0).then_some(level)
}

/// 设置内置面板用户亮度（0..1，clamp）。返回写入是否被服务接受。
pub fn set_builtin_brightness(value: f32) -> bool {
    type SetFn = unsafe extern "C" fn(u32, f32) -> i32;
    let sym = display_services_symbol(c"DisplayServicesSetBrightness");
    if sym.is_null() {
        return false;
    }
    // SAFETY: 符号非空，按 DisplayServicesSetBrightness 的真实签名转译
    let set: SetFn = unsafe { std::mem::transmute(sym) };
    let Some(id) = builtin_display_id() else {
        return false;
    };
    // SAFETY: set 为已解析符号，float 按值传参
    unsafe { set(id, value.clamp(0.0, 1.0)) == 0 }
}

/// 立即熄灭所有显示器（`pmset displaysleepnow`，无需 root）。对已灭屏为
/// 幂等无操作，不关心退出结果，但必须同步等退出回收——std Child 被 drop
/// 不回收，父进程存活期间退出的子进程恒为僵尸（补熄循环曾每 5s 漏一个，
/// 整夜累积数千个耗尽用户进程配额）。异步上下文消费走 spawn_blocking。
pub fn sleep_displays_now() {
    let _ = Command::new("/usr/bin/pmset")
        .arg("displaysleepnow")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// 面板视为「亮」的归一化阈值（macOS 合盖首拍即把背光归零，活读常为 0，
/// 开盖期样本是恢复目标的可靠来源）。
pub const BRIGHTNESS_LIT_THRESHOLD: f32 = 0.02;

/// 零亮度策略的动作决策（纯函数供单测）。
#[derive(Debug, PartialEq)]
pub enum ScreenDimAction {
    /// 合盖边沿：以 target 为恢复目标把背光归零（framebuffer 保持活跃，远程捕获流不断）
    Zero { target: f32 },
    /// 停留期被外部调亮：压回零（节流）
    Rezero,
    /// 开盖（或策略退出）：恢复保存的目标（调用方应在面板仍暗时才恢复）
    Restore,
    /// 本 tick 无动作
    None,
}

/// 零亮度策略决策：合盖 + 无外接时归零背光（恢复目标取活读亮值，开盖期
/// 样本兜底——合盖首拍活读常已归零）；开盖恢复；停留期被调亮则节流压回。
/// lid 读 None 按 `was_closed` 闭锁延续。外接在场恒不动（macOS 自管内屏）。
#[allow(clippy::too_many_arguments)]
pub fn screen_dim_action(
    lid_closed: Option<bool>,
    was_closed: bool,
    external: bool,
    brightness: Option<f32>,
    open_sample: Option<f32>,
    has_saved: bool,
    since_last_dim: Duration,
) -> ScreenDimAction {
    let closed_now = match lid_closed {
        Some(closed) => closed,
        None => was_closed,
    };
    if !closed_now {
        return if has_saved {
            ScreenDimAction::Restore
        } else {
            ScreenDimAction::None
        };
    }
    if external {
        return ScreenDimAction::None;
    }
    if !has_saved {
        // 边沿：恢复目标优先活读亮值，macOS 合盖首拍即归零则退开盖样本；
        // 两者皆无（从未采到亮值）不动，等开盖样本就绪后的下个边沿
        let target = brightness
            .filter(|b| *b > BRIGHTNESS_LIT_THRESHOLD)
            .or(open_sample);
        return match target {
            Some(t) => ScreenDimAction::Zero { target: t },
            None => ScreenDimAction::None,
        };
    }
    // 停留期：外部调亮（或压零失败）时节流重试
    if brightness
        .map(|b| b > BRIGHTNESS_LIT_THRESHOLD)
        .unwrap_or(false)
        && since_last_dim >= RESLEEP_INTERVAL
    {
        return ScreenDimAction::Rezero;
    }
    ScreenDimAction::None
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

/// daemon 循环周期（秒）：与原 watchdog 同频，持有维持与 beat 过期检测的粒度。
pub const DAEMON_CYCLE_SECS: u64 = 2;

/// beat 过期阈值（秒）：app 心跳间隔 2s 的 5 周期余量（容忍 worker 短暂阻塞）。
/// app 死亡（退出/崩溃）后最迟 GRACE + 一个周期恢复默认睡眠；重启间隙短于
/// GRACE 则无缝续持。
pub const BEAT_GRACE_SECS: i64 = 10;

/// daemon 循环逻辑版本令牌：行为变更时递增。安装件以包含此令牌判定新鲜，
/// 而非整文件字节比对——plist 模板排版等外观调整不触发提权重装，令牌缺席
/// 或不同才重装。
pub const DAEMON_LOOP_VERSION: &str = "awake-loop-1";

/// 睡眠守护 daemon 的常驻 sh 循环体（launchd LaunchDaemon 托管，plist 内联、
/// 永不退出、KeepAlive 自愈）。每周期三分支，恢复路径全部**现实态判定**
///（`pmset -g` 实查）而非进程内存——`disablesleep` 是持久系统设置，daemon
/// 进程被替换（KeepAlive 重生 / 重启 RunAtLoad / 重装 bootout+bootstrap）后
/// 内存态归零，若按内存守卫恢复会漏写 0，留下无人持有的全局睡眠禁用：
///
/// - flag 在场 + beat 新鲜（mtime 距今 ≤ GRACE）→ 持有：每周期校验
///   `pmset -g` 的 SleepDisabled（实际输出为 `SleepDisabled\t\t1`，中间两个
///   tab，匹配须用 `.*` 桥接），失配即重写 1——覆盖置位边沿与外部清零自愈
///   （UU 远程会话建立、手动 `sudo pmset`；边沿式写入有实测事故：合盖防睡
///   无声丢失、系统按 Clamshell Sleep 入睡）。覆盖外部手动改动的代价被
///   接受：app 开关是用户意图的明确表达。置 HELD=1。
/// - flag 在场 + beat 过期 → app 已死（退出/崩溃）：rm flag，现实态为 1 才
///   写 0（崩溃安全，无落盘恢复债务），HELD=0。
/// - flag 缺席 → HELD 或首周期（FIRST）时现实态为 1 才写 0，随后空闲
///   （pmset 零 fork，仅 touch/sleep 周期开销）。HELD 只做「恢复一次」的
///   停止条件，防与另一实例（dev/prod 双 daemon）的持有互相拉扯；FIRST
///   兜底「上一代进程持有中死亡 + flag 已被删」的孤儿 disablesleep——代价
///   是空闲首周期可能覆盖用户手工 `sudo pmset disablesleep 1`（接受，见上）。
///
/// 每周期 touch daemon 存活标记（app 侧安装验证与活性判定的唯一信号）。
/// beat age 以 `date +%s` − `stat -f %m` 计算：beat 缺失 → age 巨大 → 过期；
/// 时钟后跳 age 为负 → 判新鲜（安全方向）；时钟前跳 → 单周期伪释放，下周期
/// 自愈。launchd 环境无 PATH，全部外部命令绝对路径。
pub fn daemon_loop_script(flag: &str, beat: &str, daemon_beat: &str) -> String {
    let q = crate::platform::elevate::shell_quote;
    let fresh = "! /usr/bin/pmset -g | /usr/bin/grep -q 'SleepDisabled.*1'";
    let held = "/usr/bin/pmset -g | /usr/bin/grep -q 'SleepDisabled.*1'";
    format!(
        "HELD=0; FIRST=1; while :; do \
         if [ -f {flag} ]; then \
         AGE=$(( $(/bin/date +%s) - $(/usr/bin/stat -f %m {beat} 2>/dev/null || echo 0) )); \
         if [ ${{AGE:-999}} -le {grace} ]; then \
         if {fresh}; then /usr/bin/pmset -a disablesleep 1; fi; HELD=1; \
         else /bin/rm -f {flag}; \
         if {held}; then /usr/bin/pmset -a disablesleep 0; fi; HELD=0; fi; \
         elif [ \"$HELD\" -eq 1 ] || [ \"$FIRST\" -eq 1 ]; then \
         if {held}; then /usr/bin/pmset -a disablesleep 0; fi; HELD=0; fi; \
         FIRST=0; /usr/bin/touch {daemon_beat}; /bin/sleep {cycle}; done",
        flag = q(flag),
        beat = q(beat),
        daemon_beat = q(daemon_beat),
        fresh = fresh,
        held = held,
        grace = BEAT_GRACE_SECS,
        cycle = DAEMON_CYCLE_SECS,
    )
}

/// XML 五字符实体转义（plist `<string>` 内联脚本含双引号/`&` 等）。
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 睡眠守护 daemon 的 LaunchDaemon plist。脚本经 `cat` 复制进安装件（不走
/// `do shell script`），只需 XML 转义。RunAtLoad + KeepAlive：开机常驻、
/// 崩溃自愈（ThrottleInterval 10 保持重生敏捷——循环设计为永不退出，仅异常
/// 死亡时生效）。LoopVersion 令牌供安装侧新鲜度判定（launchd 忽略未知键）。
pub fn awake_daemon_plist(label: &str, script: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>-c</string>
        <string>{script}</string>
    </array>
    <key>LoopVersion</key>
    <string>{version}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>10</integer>
</dict>
</plist>"#,
        label = label,
        script = xml_escape(script),
        version = DAEMON_LOOP_VERSION
    )
}

/// 进程活动持有（`NSProcessInfo beginActivity`）：持有期间豁免本进程的
/// App Nap / timer 合并——app 心跳依赖 2s 定时器，合盖无交互的隐藏 LSUIElement
/// 正是 App Nap 的完整触发画像（定时器被拖延 >GRACE 即丢持有）。选
/// `UserInitiatedAllowingIdleSleep`：不加冗余的闲置睡眠断言（睡眠杠杆由
/// daemon 的 disablesleep 独占），仅保本进程调度不被节流。
pub struct ActivityHold {
    handle:
        objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>>,
}

// SAFETY: 句柄是不透明 token，仅回传给 endActivity（Apple 文档允许跨线程），
// 不触发任何 Objective-C 消息传递
unsafe impl Send for ActivityHold {}

/// 开始进程活动持有。符号/运行时异常返回 None，消费方按无豁免降级
/// （心跳照常，仅失去 App Nap 保护）。
pub fn begin_activity_hold() -> Option<ActivityHold> {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let info = NSProcessInfo::processInfo();
    let reason = NSString::from_str("voidnix-awake-heartbeat");
    let handle = info.beginActivityWithOptions_reason(
        NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
        &reason,
    );
    Some(ActivityHold { handle })
}

impl ActivityHold {
    /// 结束持有（drop 不自动结束，必须显式 endActivity）
    pub fn end(self) {
        use objc2_foundation::NSProcessInfo;
        // SAFETY: handle 来自 beginActivityWithOptions:reason: 的原样返回
        unsafe { NSProcessInfo::processInfo().endActivity(&self.handle) };
    }
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
    fn daemon_loop_script_shape() {
        let cmd = daemon_loop_script(
            "/Users/x/Library/Application Support/a/awake.flag",
            "/Users/x/Library/Application Support/a/awake.beat",
            "/Users/x/Library/Application Support/a/awake.daemon-beat",
        );
        // 永真循环（launchd 托管常驻，永不退出）+ 现实态恢复的状态位
        assert!(cmd.starts_with("HELD=0; FIRST=1; while :; do "));
        // 路径含空格必须以单引号传入
        assert!(cmd.contains("-f '/Users/x/Library/Application Support/a/awake.flag'"));
        assert!(cmd.contains("stat -f %m '/Users/x/Library/Application Support/a/awake.beat'"));
        // beat age 计算 + GRACE 常量内嵌（缺失 → age 巨大 → 过期方向安全）
        assert!(cmd.contains("AGE=$(( $(/bin/date +%s) - $(/usr/bin/stat -f %m"));
        assert!(cmd.contains("|| echo 0) ));"));
        assert!(cmd.contains(&format!("[ ${{AGE:-999}} -le {BEAT_GRACE_SECS} ]")));
        // 持有：pmset -g 现实态校验（tab 桥接）失配即写 1（覆盖置位边沿与清零自愈）
        assert!(cmd.contains(
            "if ! /usr/bin/pmset -g | /usr/bin/grep -q 'SleepDisabled.*1'; then /usr/bin/pmset -a disablesleep 1; fi; HELD=1;"
        ));
        // beat 过期 → app 已死：清 flag + 现实态为 1 才恢复（进程被替换后也能收尾）
        assert!(cmd.contains("else /bin/rm -f"));
        assert!(cmd.contains(
            "if /usr/bin/pmset -g | /usr/bin/grep -q 'SleepDisabled.*1'; then /usr/bin/pmset -a disablesleep 0; fi; HELD=0; fi;"
        ));
        // flag 缺席：HELD 或首周期（FIRST 兜底进程替换的孤儿 disablesleep）恢复一次
        assert!(cmd.contains("elif [ \"$HELD\" -eq 1 ] || [ \"$FIRST\" -eq 1 ]; then"));
        assert!(cmd.contains("FIRST=0; /usr/bin/touch"));
        // daemon 存活标记 + 周期常量
        assert!(cmd.contains(
            "/usr/bin/touch '/Users/x/Library/Application Support/a/awake.daemon-beat';"
        ));
        assert!(cmd.contains(&format!("/bin/sleep {DAEMON_CYCLE_SECS}; done")));
        // launchd 无 PATH：全部外部命令绝对路径（裸 sleep/touch/rm/stat 不容出现）
        assert!(!cmd.contains(" sleep 2;") && !cmd.contains(" rm ") && !cmd.contains(" touch "));
    }

    #[test]
    fn xml_escape_covers_entity_chars() {
        assert_eq!(
            xml_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn awake_daemon_plist_inlines_escaped_script() {
        let script = daemon_loop_script("/a/awake.flag", "/a/awake.beat", "/a/awake.daemon-beat");
        let plist = awake_daemon_plist("com.x.awake", &script);
        // launchd 形状：Label + 内联 sh -c + LoopVersion + RunAtLoad/KeepAlive/ThrottleInterval
        assert!(plist.contains("<string>com.x.awake</string>"));
        assert!(plist.contains("<string>/bin/sh</string>\n        <string>-c</string>"));
        assert!(plist.contains(&format!(
            "<key>LoopVersion</key>\n    <string>{DAEMON_LOOP_VERSION}</string>"
        )));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>KeepAlive</key>\n    <true/>"));
        assert!(plist.contains("<key>ThrottleInterval</key>\n    <integer>10</integer>"));
        // 脚本中的双引号（HELD/FIRST 判等）必须转义为实体
        assert!(plist.contains("[ &quot;$HELD&quot; -eq 1 ]"));
        // 直接验证 & 转义：注入含 & 的探针脚本
        let probe = awake_daemon_plist("l", "a&b");
        assert!(probe.contains("<string>a&amp;b</string>"));
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
    fn screen_dim_decision_lifecycle() {
        use ScreenDimAction as A;
        let long = RESLEEP_INTERVAL + Duration::from_secs(1);
        let short = RESLEEP_INTERVAL - Duration::from_secs(1);
        let lit = Some(0.5f32);
        let dark = Some(0.0f32);
        let no_sample: Option<f32> = None;

        // 开盖：无保存不动，有保存恢复
        assert_eq!(
            screen_dim_action(Some(false), false, false, lit, no_sample, false, long),
            A::None
        );
        assert_eq!(
            screen_dim_action(Some(false), true, false, dark, no_sample, true, short),
            A::Restore
        );

        // 合盖边沿：活读亮值即恢复目标；活读已归零则退开盖样本；两者皆无不动
        assert_eq!(
            screen_dim_action(Some(true), false, false, lit, no_sample, false, long),
            A::Zero { target: 0.5 }
        );
        assert_eq!(
            screen_dim_action(Some(true), false, false, dark, Some(0.7), false, long),
            A::Zero { target: 0.7 }
        );
        assert_eq!(
            screen_dim_action(Some(true), false, false, dark, no_sample, false, long),
            A::None
        );

        // 停留期：被调亮且过节流才压回；暗或窗口内不动
        assert_eq!(
            screen_dim_action(Some(true), true, false, lit, no_sample, true, long),
            A::Rezero
        );
        assert_eq!(
            screen_dim_action(Some(true), true, false, lit, no_sample, true, short),
            A::None
        );
        assert_eq!(
            screen_dim_action(Some(true), true, false, dark, no_sample, true, long),
            A::None
        );

        // 外接在场恒不动（macOS 自管内屏）；lid None 按闭锁延续
        assert_eq!(
            screen_dim_action(Some(true), false, true, lit, Some(0.5), false, long),
            A::None
        );
        assert_eq!(
            screen_dim_action(None, true, false, lit, no_sample, true, long),
            A::Rezero
        );
        assert_eq!(
            screen_dim_action(None, false, false, lit, no_sample, false, long),
            A::None
        );
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
}
