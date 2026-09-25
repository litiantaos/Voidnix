use crate::runtime::registry::Extension;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{
    Components, DiskKind, Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System,
};
use tauri::{AppHandle, Emitter, Manager, State};

/// 系统状态扩展：硬件信息 + 实时状态。快照为拉模式（前端进入模块轮询）；
/// 菜单栏下拉状态行（单个可点击项，点击打开扩展界面）由 setup 内采样任务供给
/// （CPU/内存 5s、温度 30s，图标隐藏时挂起，见 `spawn_menu_sampler`）。
/// sysinfo 全局实例在 setup 时创建并 prime CPU（首次 refresh_cpu_usage 后下次才有真实使用率）。
pub struct SystemState {
    sys: Mutex<System>,
    disks: Mutex<Disks>,
    networks: Mutex<Networks>,
    components: Mutex<Components>,
    net_stats: Mutex<NetStats>,
    /// GPU 信息缓存（system_profiler 调用慢 ~1s，静态信息缓存首次结果）
    gpu: Mutex<Option<(String, Option<u32>)>>,
    /// 菜单栏状态行缓存（采样任务写、MenuBarContribution build 读，build 零阻塞）
    menu_line: Mutex<Option<String>>,
    /// 菜单栏状态段显隐（前端 config watch 经 set_system_status_menubar_visible 同步）。
    /// 初始 false：生效值到位前不显示，防配置关闭时启动闪现（与 menubar ICON_VISIBLE 同思路）
    menubar_visible: AtomicBool,
}

struct NetStats {
    last_time: Option<Instant>,
    last_rx: u64,
    last_tx: u64,
}

// ── 静态信息（一次性）──

#[derive(Serialize)]
pub struct DiskStaticInfo {
    pub name: String,
    pub mount_point: String,
    pub fs_type: String,
    /// SSD / HDD / Unknown
    pub kind: String,
    pub removable: bool,
    pub total: u64,
}

#[derive(Serialize)]
pub struct SystemStaticInfo {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub model: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub gpu_model: String,
    pub gpu_cores: Option<u32>,
    pub total_memory: u64,
    pub disks: Vec<DiskStaticInfo>,
}

// ── 实时快照（每 2s 轮询）──

#[derive(Serialize)]
pub struct DiskUsageInfo {
    pub name: String,
    pub mount_point: String,
    /// SSD / HDD / Unknown
    pub kind: String,
    pub removable: bool,
    pub used: u64,
    pub total: u64,
}

#[derive(Serialize)]
pub struct ProcessInfo {
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

#[derive(Serialize)]
pub struct BatteryInfo {
    pub level: u8,
    pub state: String,
    pub cycles: Option<u32>,
    pub health: Option<u8>,
    pub time_to_empty: Option<i32>,
    /// 充满剩余分钟（仅充电中有意义）
    pub time_to_full: Option<i32>,
    /// 适配器额定功率（W）
    pub adapter_watts: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Serialize)]
pub struct SystemSnapshot {
    pub cpu_usage: f32,
    pub cpu_cores_usage: Vec<f32>,
    pub cpu_temp: Option<f32>,
    pub used_memory: u64,
    pub available_memory: u64,
    pub total_memory: u64,
    pub used_swap: u64,
    pub total_swap: u64,
    /// 1 / 5 / 15 分钟负载均值
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    /// nominal / fair / serious / critical
    pub thermal: String,
    pub low_power_mode: bool,
    pub disks_usage: Vec<DiskUsageInfo>,
    pub battery: Option<BatteryInfo>,
    pub net_up: f64,
    pub net_down: f64,
    pub local_ip: String,
    pub uptime: u64,
    pub processes: Vec<ProcessInfo>,
}

fn disk_kind_label(kind: DiskKind) -> String {
    match kind {
        DiskKind::SSD => "SSD".into(),
        DiskKind::HDD => "HDD".into(),
        _ => "Unknown".into(),
    }
}

#[tauri::command]
pub async fn system_static_info(state: State<'_, SystemState>) -> Result<SystemStaticInfo, String> {
    // sys/disks 锁作用域：读取后释放，避免跨 GPU await 持锁
    let (model, cpu_model, cpu_cores, total_memory, disks_info) = {
        let sys = crate::runtime::lock_or_recover(&state.sys);
        let disks = crate::runtime::lock_or_recover(&state.disks);

        let model = sysctl_hw_model().unwrap_or_default();
        let cpu = sys.cpus().first();
        let cpu_model = cpu.map(|c| c.brand().to_string()).unwrap_or_default();
        let cpu_cores = System::physical_core_count().unwrap_or_else(|| sys.cpus().len());

        // 磁盘去重：APFS 下同名卷 + /System/Volumes/ 系统数据卷会造成重复
        let mut seen: HashSet<String> = HashSet::new();
        let disks_info = disks
            .list()
            .iter()
            .filter_map(|d| {
                let name = d.name().to_string_lossy().into_owned();
                let mount_point = d.mount_point().to_string_lossy().into_owned();
                if mount_point.starts_with("/System/Volumes/") {
                    return None;
                }
                if !seen.insert(name.clone()) {
                    return None;
                }
                Some(DiskStaticInfo {
                    name,
                    mount_point,
                    fs_type: d.file_system().to_string_lossy().into_owned(),
                    kind: disk_kind_label(d.kind()),
                    removable: d.is_removable(),
                    total: d.total_space(),
                })
            })
            .collect();

        (model, cpu_model, cpu_cores, sys.total_memory(), disks_info)
    };

    // GPU：system_profiler 调用慢 ~1s，缓存首次结果（静态信息不变）
    let (gpu_model, gpu_cores) = {
        let cached = crate::runtime::lock_or_recover(&state.gpu).clone();
        if let Some(g) = cached {
            g
        } else {
            let g = tauri::async_runtime::spawn_blocking(read_gpu)
                .await
                .unwrap_or_default();
            *crate::runtime::lock_or_recover(&state.gpu) = Some(g.clone());
            g
        }
    };

    Ok(SystemStaticInfo {
        hostname: System::host_name().unwrap_or_default(),
        os_name: System::name().unwrap_or_default(),
        os_version: System::os_version().unwrap_or_default(),
        model,
        cpu_model,
        cpu_cores,
        gpu_model,
        gpu_cores,
        total_memory,
        disks: disks_info,
    })
}

/// 精简进程刷新策略：只拉 cpu + memory（UI 仅消费 Top 3 的 name/cpu/memory）。
/// 默认 refresh_processes 额外拉 disk_usage（I/O 开销）+ exe 路径，纯属浪费。
fn process_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing().with_cpu().with_memory()
}

/// 菜单栏状态段显隐（前端 config watch 同步，Config 字段型）。
#[tauri::command]
pub async fn set_system_status_menubar_visible(
    app: AppHandle,
    state: State<'_, SystemState>,
    visible: bool,
) -> Result<(), String> {
    state.menubar_visible.store(visible, Ordering::Relaxed);
    crate::runtime::menubar::refresh(&app);
    Ok(())
}

/// 实时快照（每 2s 轮询）。
///
/// 声明 async 而无显式 await：Tauri 把 async command body 调度到 runtime worker 线程，
/// 把 blocking ioreg 子进程调用（read_battery）+ 全进程遍历从主线程卸载，避免 UI 冻结。
#[tauri::command]
pub async fn system_snapshot(state: State<'_, SystemState>) -> Result<SystemSnapshot, String> {
    let mut sys = crate::runtime::lock_or_recover(&state.sys);
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());

    let cpu_usage = sys.global_cpu_usage();
    let cpu_cores_usage = sys.cpus().iter().map(|c| c.cpu_usage()).collect();

    // CPU 温度：跨平台传感器选择（Intel 含 cpu 标签 / Apple Silicon PMU tdie 族）
    let cpu_temp = cpu_temp_of(&state);

    // 磁盘（跳过 APFS 系统数据卷 + 按 name 去重）
    let disks_usage = {
        let mut disks = crate::runtime::lock_or_recover(&state.disks);
        disks.refresh(false);
        let mut seen: HashSet<String> = HashSet::new();
        disks
            .list()
            .iter()
            .filter_map(|d| {
                let name = d.name().to_string_lossy().into_owned();
                let mount_point = d.mount_point().to_string_lossy().into_owned();
                if mount_point.starts_with("/System/Volumes/") {
                    return None;
                }
                if !seen.insert(name.clone()) {
                    return None;
                }
                Some(DiskUsageInfo {
                    name,
                    mount_point,
                    kind: disk_kind_label(d.kind()),
                    removable: d.is_removable(),
                    used: d.total_space().saturating_sub(d.available_space()),
                    total: d.total_space(),
                })
            })
            .collect()
    };

    // 网络速率：基于上次采样的字节差值 / 时间
    // refresh(true)（全量对账）而非 refresh(false)：后者只更新已有条目，捕获不到接口增删（TUN/VPN 启停、Wi-Fi 切换）
    let (net_up, net_down) = {
        let mut nets = crate::runtime::lock_or_recover(&state.networks);
        nets.refresh(true);
        let cur_rx: u64 = nets.list().values().map(|n| n.total_received()).sum();
        let cur_tx: u64 = nets.list().values().map(|n| n.total_transmitted()).sum();
        let mut stats = crate::runtime::lock_or_recover(&state.net_stats);
        let (up, down) = if let Some(last_t) = stats.last_time {
            let elapsed = last_t.elapsed().as_secs_f64().max(0.1);
            (
                ((cur_tx as f64 - stats.last_tx as f64) / elapsed).max(0.0),
                ((cur_rx as f64 - stats.last_rx as f64) / elapsed).max(0.0),
            )
        } else {
            (0.0, 0.0)
        };
        stats.last_time = Some(Instant::now());
        stats.last_rx = cur_rx;
        stats.last_tx = cur_tx;
        (up, down)
    };

    // Top 3 进程（按 CPU 降序）
    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p| ProcessInfo {
            name: p.name().to_string_lossy().into_owned(),
            cpu: p.cpu_usage(),
            memory: p.memory(),
        })
        .collect();
    processes.sort_by(|a, b| {
        b.cpu
            .partial_cmp(&a.cpu)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    processes.truncate(3);

    let load = System::load_average();
    let (thermal, low_power_mode) = read_thermal();

    Ok(SystemSnapshot {
        cpu_usage,
        cpu_cores_usage,
        cpu_temp,
        used_memory: sys.used_memory(),
        available_memory: sys.available_memory(),
        total_memory: sys.total_memory(),
        used_swap: sys.used_swap(),
        total_swap: sys.total_swap(),
        load_one: load.one,
        load_five: load.five,
        load_fifteen: load.fifteen,
        thermal,
        low_power_mode,
        disks_usage,
        battery: read_battery(),
        net_up,
        net_down,
        local_ip: local_ip(),
        uptime: System::uptime(),
        processes,
    })
}

/// `sysctl -n hw.model` → 机型代码（如 "MacBookPro18,4"），前端映射为友好名。
fn sysctl_hw_model() -> Option<String> {
    std::process::Command::new("sysctl")
        .args(["-n", "hw.model"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `system_profiler SPDisplaysDataType -json` → GPU 型号 + 核数。取首个 GPU。
fn read_gpu() -> (String, Option<u32>) {
    let out = std::process::Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output();
    let Ok(out) = out else {
        return (String::new(), None);
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return (String::new(), None);
    };
    if let Some(arr) = json["SPDisplaysDataType"].as_array() {
        for gpu in arr {
            let model = gpu["sppci_model"]
                .as_str()
                .or_else(|| gpu["_name"].as_str())
                .unwrap_or("")
                .to_string();
            let cores = gpu["sppci_cores"]
                .as_str()
                .and_then(|s| s.parse::<u32>().ok())
                .or_else(|| gpu["sppci_cores"].as_u64().map(|n| n as u32));
            if !model.is_empty() || cores.is_some() {
                return (model, cores);
            }
        }
    }
    (String::new(), None)
}

/// `ioreg -rc AppleSmartBattery` 解析电池信息。无电池（台式机）返回 None。
fn read_battery() -> Option<BatteryInfo> {
    let output = std::process::Command::new("ioreg")
        .args(["-rc", "AppleSmartBattery"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return None;
    }

    let mut current = None;
    let mut max_cap = None;
    let mut design_cap = None;
    let mut cycles = None;
    let mut is_charging = None;
    let mut external = None;
    let mut fully_charged = None;
    let mut time_to_empty = None;
    let mut time_to_full = None;
    let mut adapter_watts = None;
    let mut temperature = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = parse_ioreg_int(line, "\"CurrentCapacity\"") {
            current = Some(v);
        }
        if let Some(v) = parse_ioreg_int(line, "\"AppleRawMaxCapacity\"") {
            max_cap = Some(v);
        }
        if let Some(v) = parse_ioreg_int(line, "\"DesignCapacity\"") {
            design_cap = Some(v);
        }
        if let Some(v) = parse_ioreg_int(line, "\"CycleCount\"") {
            cycles = Some(v);
        }
        if let Some(v) = parse_ioreg_bool(line, "\"IsCharging\"") {
            is_charging = Some(v);
        }
        if let Some(v) = parse_ioreg_bool(line, "\"ExternalConnected\"") {
            external = Some(v);
        }
        if let Some(v) = parse_ioreg_bool(line, "\"FullyCharged\"") {
            fully_charged = Some(v);
        }
        // 剩余使用时间（分钟），充电时为 65535（无意义）
        if let Some(v) = parse_ioreg_int(line, "\"InstantTimeToEmpty\"") {
            if v > 0 && v < 65535 {
                time_to_empty = Some(v as i32);
            }
        }
        // 充满剩余时间：优先 Instant，回退 Avg
        if let Some(v) = parse_ioreg_int(line, "\"InstantTimeToFull\"") {
            if v > 0 && v < 65535 {
                time_to_full = Some(v as i32);
            }
        } else if time_to_full.is_none() {
            if let Some(v) = parse_ioreg_int(line, "\"AvgTimeToFull\"") {
                if v > 0 && v < 65535 {
                    time_to_full = Some(v as i32);
                }
            }
        }
        // 适配器瓦数（嵌在 AdapterDetails 字典里）
        if line.contains("\"AdapterDetails\"") || line.contains("\"AppleRawAdapterDetails\"") {
            if let Some(v) = parse_ioreg_dict_int(line, "\"Watts\"") {
                if v > 0 && v < 1000 {
                    adapter_watts = Some(v as u32);
                }
            }
        }
        // 电池温度（单位 100×°C，如 3000 = 30.0°C）；键精确匹配避免命中 TemperatureSamples
        if let Some(v) = parse_ioreg_int(line, "\"Temperature\"") {
            temperature = Some(v as f32 / 100.0);
        }
    }

    let level = current?;
    let level = level.min(100) as u8;

    let state = if fully_charged == Some(true) || (external == Some(true) && level >= 100) {
        "full"
    } else if is_charging == Some(true) || external == Some(true) {
        "charging"
    } else {
        "discharging"
    }
    .to_string();

    // 仅放电显示剩余；仅充电显示充满；未接电源清空适配器瓦数
    let time_to_empty = if state == "discharging" {
        time_to_empty
    } else {
        None
    };
    let time_to_full = if state == "charging" {
        time_to_full
    } else {
        None
    };
    let adapter_watts = if external == Some(true) {
        adapter_watts
    } else {
        None
    };

    let health = match (max_cap, design_cap) {
        (Some(m), Some(d)) if d > 0 => {
            Some(((m as f64 / d as f64) * 100.0).round().min(100.0) as u8)
        }
        _ => None,
    };

    Some(BatteryInfo {
        level,
        state,
        cycles: cycles.filter(|&c| c > 0).map(|c| c as u32),
        health,
        time_to_empty,
        time_to_full,
        adapter_watts,
        temperature: temperature.filter(|t| *t > 0.0 && *t < 100.0),
    })
}

/// NSProcessInfo.thermalState + isLowPowerModeEnabled（无 root、无子进程）。
fn read_thermal() -> (String, bool) {
    use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
    let info = NSProcessInfo::processInfo();
    let thermal = match info.thermalState() {
        NSProcessInfoThermalState::Nominal => "nominal",
        NSProcessInfoThermalState::Fair => "fair",
        NSProcessInfoThermalState::Serious => "serious",
        NSProcessInfoThermalState::Critical => "critical",
        _ => "nominal",
    };
    (thermal.into(), info.isLowPowerModeEnabled())
}

/// 精确匹配 ioreg 键（`key` 后必须是 `=`，避免 `"Temperature"` 命中 `TemperatureSamples`）。
fn parse_ioreg_int(line: &str, key: &str) -> Option<u64> {
    let mut search = line;
    while let Some(pos) = search.find(key) {
        let after_key = &search[pos + key.len()..];
        let trimmed = after_key.trim_start();
        if let Some(rest) = trimmed.strip_prefix('=') {
            let token = rest
                .trim_start()
                .split(|c: char| c == ',' || c == '}' || c == ')' || c.is_whitespace())
                .next()?;
            return token.parse::<u64>().ok();
        }
        search = &search[pos + key.len()..];
    }
    None
}

/// 从 ioreg 字典行内解析嵌套键值（如 AdapterDetails 内的 "Watts"=65）。
fn parse_ioreg_dict_int(line: &str, key: &str) -> Option<u64> {
    parse_ioreg_int(line, key)
}

fn parse_ioreg_bool(line: &str, key: &str) -> Option<bool> {
    let mut search = line;
    while let Some(pos) = search.find(key) {
        let after_key = &search[pos + key.len()..];
        let trimmed = after_key.trim_start();
        if let Some(rest) = trimmed.strip_prefix('=') {
            let token = rest
                .trim_start()
                .split(|c: char| c == ',' || c == '}' || c == ')' || c.is_whitespace())
                .next()?;
            return match token {
                "Yes" | "true" => Some(true),
                "No" | "false" => Some(false),
                _ => None,
            };
        }
        search = &search[pos + key.len()..];
    }
    None
}

/// 内网 IP：UDP "连接" 外网地址拿出口接口的本地 IP（不发实际数据包）。
fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("8.8.8.8:80").and_then(|_| s.local_addr()))
        .map(|addr| addr.ip().to_string())
        .unwrap_or_default()
}

/// 刷新温度传感器并取 CPU 温度（snapshot 与菜单栏采样共用）。
fn cpu_temp_of(state: &SystemState) -> Option<f32> {
    let mut comps = crate::runtime::lock_or_recover(&state.components);
    comps.refresh(false);
    let sensors: Vec<(&str, Option<f32>)> =
        comps.iter().map(|c| (c.label(), c.temperature())).collect();
    pick_cpu_temp(&sensors)
}

/// CPU 温度传感器选择（标签跨平台形态不同）：Intel 命中 label 含 cpu 的传感器
/// （sysinfo 映射 SMC 键为 PECI CPU / CPU Proximity）；Apple Silicon 无 cpu 标签，
/// IOHID 温度传感器为 PMU tdie*（SoC 各 die 温度，随负载爬升）。两类各取族内
/// 最大值（最热 die 最能代表负载），cpu 标签优先命中。
fn pick_cpu_temp(sensors: &[(&str, Option<f32>)]) -> Option<f32> {
    fn max_valid(sensors: &[(&str, Option<f32>)], matched: impl Fn(&str) -> bool) -> Option<f32> {
        sensors
            .iter()
            .filter(|(label, t)| {
                matched(&label.to_lowercase()) && t.is_some_and(|v| v > 0.0 && v < 200.0)
            })
            .filter_map(|(_, t)| *t)
            .fold(None, |acc: Option<f32>, t| {
                Some(acc.map_or(t, |a| a.max(t)))
            })
    }
    max_valid(sensors, |l| l.contains("cpu"))
        .or_else(|| max_valid(sensors, |l| l.starts_with("pmu tdie")))
}

// ── 菜单栏下拉状态行 ──

/// CPU/内存采样间隔（CPU 差值语义的最小窗口远小于此，5s 粒度足够速览）。
const MENU_FAST_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
/// 温度每 N 轮快采一次（IOHID 传感器刷新开销较高，30s 低频）。
const MENU_SLOW_EVERY: u32 = 6;

/// 下拉菜单状态行（单个可点击项）：「CPU 12% · 内存 44% · 49°C」。
/// 百分比两项带标识表意（纯数值表意不明），温度靠 °C 单位自表意省标签。
/// 温度缺失时省略温度段。
fn format_status_line(cpu: f32, mem_used: u64, mem_total: u64, temp: Option<f32>) -> String {
    let mut parts = vec![format!("CPU {cpu:.0}%")];
    if mem_total > 0 {
        parts.push(format!(
            "内存 {:.0}%",
            mem_used as f64 / mem_total as f64 * 100.0
        ));
    }
    if let Some(t) = temp {
        parts.push(format!("{t:.0}°C"));
    }
    parts.join(" · ")
}

/// 一轮快指标采样（CPU/内存）+ 组装状态行；温度由调用方缓存传入。
fn sample_line(app: &AppHandle, temp: Option<f32>) -> Option<String> {
    let state = app.try_state::<SystemState>()?;
    let (cpu, used, total) = {
        let mut sys = crate::runtime::lock_or_recover(&state.sys);
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        (
            sys.global_cpu_usage(),
            sys.used_memory(),
            sys.total_memory(),
        )
    };
    Some(format_status_line(cpu, used, total, temp))
}

/// 慢指标采样：CPU 温度（IOHID 传感器刷新）。外层 None = 未采到状态（state 缺失），
/// 内层 None = 温度传感器无有效值——两层语义供采样循环的缓存 flatten 区分。
fn sample_slow(app: &AppHandle) -> Option<Option<f32>> {
    let state = app.try_state::<SystemState>()?;
    Some(cpu_temp_of(&state))
}

/// 菜单栏状态行采样任务：快指标每轮采、温度每 `MENU_SLOW_EVERY` 轮采（结果缓存），
/// 每轮写 `menu_line` 缓存并 refresh 重建菜单（打开下拉即见最近快照）。
/// 图标隐藏时跳过采样（零开销挂起），恢复可见后下一轮补上；阻塞采集走 spawn_blocking。
fn spawn_menu_sampler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(MENU_FAST_INTERVAL);
        let mut round: u32 = 0;
        // 启动即采一次温度（interval 首 tick 立即到达，图标显示后即刻有完整状态行）
        let mut slow = None;
        loop {
            tick.tick().await;
            if !crate::runtime::menubar::is_icon_visible() {
                continue;
            }
            if !app
                .state::<SystemState>()
                .menubar_visible
                .load(Ordering::Relaxed)
            {
                continue;
            }
            round = round.wrapping_add(1);
            if round.is_multiple_of(MENU_SLOW_EVERY) || slow.is_none() {
                let a = app.clone();
                if let Ok(s) = tauri::async_runtime::spawn_blocking(move || sample_slow(&a)).await {
                    slow = s;
                }
            }
            if let Some(line) = sample_line(&app, slow.flatten()) {
                let state = app.state::<SystemState>();
                *crate::runtime::lock_or_recover(&state.menu_line) = Some(line);
                crate::runtime::menubar::refresh(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{format_status_line, pick_cpu_temp};

    #[test]
    fn status_line_full_metrics() {
        let s = format_status_line(12.4, 7_000_000_000, 16_000_000_000, Some(48.6));
        assert_eq!(s, "CPU 12% · 内存 44% · 49°C");
    }

    #[test]
    fn status_line_without_temp() {
        let s = format_status_line(8.0, 8_000_000_000, 16_000_000_000, None);
        assert_eq!(s, "CPU 8% · 内存 50%");
    }

    #[test]
    fn status_line_zero_mem_total_skips_mem() {
        let s = format_status_line(3.0, 0, 0, None);
        assert_eq!(s, "CPU 3%");
    }

    #[test]
    fn picks_max_among_cpu_labels_on_intel() {
        let sensors = [
            ("PECI CPU", Some(55.0)),
            ("CPU Proximity", Some(50.0)),
            ("GPU", Some(60.0)),
        ];
        assert_eq!(pick_cpu_temp(&sensors), Some(55.0));
    }

    #[test]
    fn picks_pmu_tdie_max_on_apple_silicon() {
        let sensors = [
            ("PMU tdie1", Some(43.8)),
            ("PMU tdie10", Some(44.2)),
            ("gas gauge battery", Some(34.9)),
            ("NAND CH0 temp", Some(36.0)),
        ];
        assert_eq!(pick_cpu_temp(&sensors), Some(44.2));
    }

    #[test]
    fn cpu_label_preferred_over_tdie() {
        let sensors = [("PMU tdie1", Some(44.0)), ("CPU Proximity", Some(41.0))];
        assert_eq!(pick_cpu_temp(&sensors), Some(41.0));
    }

    #[test]
    fn filters_invalid_and_none() {
        let sensors = [
            ("PMU tdie1", Some(0.0)),
            ("PMU tdie2", Some(250.0)),
            ("PMU tdie3", None),
            ("als-temp", None),
        ];
        assert_eq!(pick_cpu_temp(&sensors), None);
    }

    #[test]
    fn none_when_no_match() {
        let sensors = [("NAND CH0 temp", Some(36.0))];
        assert_eq!(pick_cpu_temp(&sensors), None);
    }
}

/// 系统状态扩展。
pub struct SystemStatusExtension;

#[async_trait::async_trait]
impl Extension for SystemStatusExtension {
    fn id(&self) -> &'static str {
        "system-status"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // 立即 manage 空 collector（SystemState 仅用户打开模块时消费，无需启动时阻塞）。
        // 重采集（refresh_processes(All) + Disks/Networks/Components 全量刷新 ~100ms+）
        // 下沉后台 spawn_blocking，不阻塞 bootstrap join_all。
        // Disks/Networks/Components::new_with_refreshed_list：sysinfo 的 new() 返回空列表，
        // refresh(false) 只更新已有条目（空列表上为 no-op，接口增删也捕获不到）。
        app.manage(SystemState {
            sys: Mutex::new(System::new()),
            disks: Mutex::new(Disks::new_with_refreshed_list()),
            networks: Mutex::new(Networks::new_with_refreshed_list()),
            components: Mutex::new(Components::new_with_refreshed_list()),
            net_stats: Mutex::new(NetStats {
                last_time: None,
                last_rx: 0,
                last_tx: 0,
            }),
            gpu: Mutex::new(None),
            menu_line: Mutex::new(None),
            menubar_visible: AtomicBool::new(false),
        });

        // 后台 prime：refresh_cpu_usage / refresh_processes 后下次调用才有真实使用率（差值语义）。
        // 用户打开模块前完成即可（首屏 ~1s 窗口动画 + 手动导航，远早于用户进入 system-status）。
        let app2 = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Some(state) = app2.try_state::<SystemState>() {
                {
                    let mut sys = state.sys.lock().unwrap();
                    sys.refresh_cpu_usage();
                    sys.refresh_memory();
                    sys.refresh_processes_specifics(
                        ProcessesToUpdate::All,
                        true,
                        process_refresh_kind(),
                    );
                }
                state.disks.lock().unwrap().refresh(false);
                state.networks.lock().unwrap().refresh(true);
                state.components.lock().unwrap().refresh(false);
            }
        });

        // 菜单栏下拉状态行（单个可点击项，点击打开扩展界面；build 读缓存零阻塞）。
        // order 30 置顶（proxy 40 / awake 160 之前）：状态速览先于交互项
        crate::runtime::menubar::register(crate::runtime::menubar::MenuBarContribution {
            title: "系统状态",
            order: 30,
            build: std::sync::Arc::new(|app: &AppHandle| {
                let Some(state) = app.try_state::<SystemState>() else {
                    return vec![];
                };
                if !state.menubar_visible.load(Ordering::Relaxed) {
                    return vec![];
                }
                let line = crate::runtime::lock_or_recover(&state.menu_line).clone();
                match line {
                    Some(label) => vec![crate::runtime::menubar::MenuEntry::Item {
                        id: "system_status_open".into(),
                        label,
                        enabled: true,
                    }],
                    None => vec![],
                }
            }),
            on_event: std::sync::Arc::new(|app: &AppHandle, id: &str| {
                if id == "system_status_open" {
                    let was_visible = crate::runtime::shortcut::is_window_visible();
                    let _ = app.emit(
                        "open-extension",
                        serde_json::json!({ "id": "system-status", "wasVisible": was_visible }),
                    );
                }
            }),
        });

        // 菜单栏状态行采样（图标隐藏时挂起）
        spawn_menu_sampler(app.clone());
        Ok(())
    }
}
