use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, LazyLock, Mutex};
use tauri::Emitter;
use tauri_plugin_global_shortcut::{
    GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::runtime::lock_or_recover;

static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(false);
static LAST_SHOW_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── 快捷键录制模式 ───────────────────────────────────────────────────────────
//
// 录制是「全局模态」：只要任何 ShortcutInput 正在录制，所有已注册的全局
// 快捷键回调都应抑制默认行为，把按下的键位转发回前端由当前录制中的输入框
// 吸收。RECORDING_IDS 仅作前端调试/将来扩展用途，判断是否处于录制态以
// IS_RECORDING_ANY 为准（原子操作，回调热路径无需加锁）。

static RECORDING_IDS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static IS_RECORDING_ANY: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn start_shortcut_recording(id: String) {
    if let Ok(mut set) = RECORDING_IDS.lock() {
        set.insert(id);
        // M-rs5：可见性 flag 用 Release（store）保证后续 Acquire load 见到最新值
        IS_RECORDING_ANY.store(!set.is_empty(), Ordering::Release);
    }
}

#[tauri::command]
pub fn stop_shortcut_recording(id: String) {
    if let Ok(mut set) = RECORDING_IDS.lock() {
        set.remove(&id);
        IS_RECORDING_ANY.store(!set.is_empty(), Ordering::Release);
    }
}

// ── 扩展钩子注册 ─────────────────────────────────────────────────────────────

pub struct ShortcutContext {
    pub window_hidden: bool,
    #[cfg(target_os = "macos")]
    pub front_pid: Option<i32>,
}

type ShortcutHook = Box<dyn Fn(&tauri::AppHandle, &ShortcutContext) -> bool + Send + Sync>;

static SHORTCUT_HOOKS: LazyLock<Mutex<HashMap<String, ShortcutHook>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register_shortcut_hook(id: &str, hook: ShortcutHook) {
    lock_or_recover(&SHORTCUT_HOOKS).insert(id.to_string(), hook);
}

#[tauri::command]
pub async fn is_app_active(app: tauri::AppHandle) -> bool {
    // H4：MainThreadMarker::new() 仅主线程返 Some；Tauri 命令默认在 worker 线程执行，
    // 直接调会恒返 false → 前端误判窗口失焦而隐藏。改走 run_on_main_thread 同步取值。
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = mpsc::channel::<bool>();
        let _ = app.run_on_main_thread(move || {
            let active = crate::platform::focus::is_app_active();
            let _ = tx.send(active);
        });
        // M-rs4：recv_timeout 兜底——主线程闭包 panic（debug 构建下 panic=abort 不触发毒锁
        // 但 closure 可能因 FFI 异常 abort）时不发 channel，避免命令永久挂起
        rx.recv_timeout(std::time::Duration::from_secs(3))
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        true
    }
}

/// 供窗口模块读写窗口可见状态。
pub(crate) fn set_window_visible(v: bool) {
    WINDOW_VISIBLE.store(v, Ordering::Release);
    if v {
        LAST_SHOW_MS.store(now_ms(), Ordering::Release);
    }
}

/// 语义层窗口可见性（alpha=0 隐藏时 NSWindow isVisible 仍返回 true，不可靠）。
/// 供扩展菜单等路径判断是否需要 show。
pub fn is_window_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Acquire)
}

#[tauri::command]
pub fn hide_window(app: tauri::AppHandle, auto: Option<bool>) {
    // 已隐藏窗口直接返回：blur → hideWindow → resignKeyWindow → 派生 blur 反馈环在
    // 第一轮 hide（set_window_visible(false)）后断开，后续冗余 IPC 零主线程开销。
    if !is_window_visible() {
        return;
    }
    if auto.unwrap_or(false) {
        // M-rs5：时间戳单调读用 Acquire；防抖计算本身容忍时钟精度
        let elapsed = now_ms().saturating_sub(LAST_SHOW_MS.load(Ordering::Acquire));
        if elapsed < 500 {
            return;
        }
    }
    let app_for_reload = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        crate::runtime::window::hide_main(&app);
    });
    // 窗口已隐藏（alpha=0），异步检查 WebContent 内存：超阈值 navigate 重载
    // 释放 WKWebView tile backing（PURGE=N 不可回收）。
    // 自测二次触发由 test.rs 的 AtomicBool 一次性守卫防御，reload 可安全执行。
    crate::runtime::window::maybe_reload_webview(&app_for_reload);
}

// ============================================================================
// Shortcut registration
// ============================================================================
//
// Rust 端是注册表的单一真相源：REGISTERED_BY_ID 按业务 id 维护「当前已注册
// 的键位组」（主键 + 安全输入影子键）。前端只传 (id, new_shortcut)，旧值由
// Rust 自查并 unregister，避免前端通过 watch oldVal 推断旧值带来的撕裂。

static REGISTERED_BY_ID: LazyLock<Mutex<HashMap<String, Vec<Shortcut>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 安全输入影子键位：macOS 15 起，secure input（密码框 / 终端「安全键盘输入」）
/// 期间系统不投递只含 ⇧/⌥ 的组合——那类组合会落成 `ç` 这类字符直接进输入框。
/// 因此给 **⌥ 系**组合（含 ⌥、不含 ⌃/⌘）补一条 `⌃` 前缀的影子键位：
/// - 影子键**常驻注册**：正常场景它同样是全局热键（`⌃⌥C` 成为剪贴板的第二键位、
///   不再传给其它 App），secure input 期间主键被系统抑制，它是唯一能落地的入口；
/// - 额外占用的全局键位由声明方按需承担（当前仅剪贴板）。
///
/// 不含 ⌥ 的组合（裸键 / 纯 ⇧）不补影子：`⌃`+字符键是终端控制键（⌃K 杀行、
/// ⌃C 中断…），补影子等于把这些键从所有终端/编辑器手里抢走；而这类组合也不是
/// 本应用的键位族（默认全为 Alt 基）。
///
/// 系统抑制本身即切换：不检测 secure input 状态、不改键位配置、界面仍显示主键位。
fn secure_mirror_chord(sc: Shortcut) -> Option<Shortcut> {
    let taken = Modifiers::CONTROL | Modifiers::SUPER | Modifiers::META;
    if !sc.mods.contains(Modifiers::ALT) || sc.mods.intersects(taken) {
        return None;
    }
    // 必须经 Shortcut::new 按新组合重建：HotKey.id 是 (mods << 16) | key 的派生值，
    // 只改 mods 会让影子与主键共享同一 id——插件与 global-hotkey 均按 id 存注册表，
    // 登记影子会顶掉主键的 EventHotKeyRef，注销后主键的 Carbon 注册泄漏（全局吞键
    // 且无法再注册）。new 会重算 id。
    Some(Shortcut::new(Some(sc.mods | Modifiers::CONTROL), sc.key))
}

#[tauri::command]
pub async fn register_global_shortcut(
    app: tauri::AppHandle,
    id: String,
    shortcut: String,
    secure_mirror: Option<bool>,
) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel::<Result<(), String>>(1);
    let app_clone = app.clone();

    app.run_on_main_thread(move || {
        // 先 unregister 该 id 上一次注册的全部键位（主键 + 影子键）
        if let Some(old) = lock_or_recover(&REGISTERED_BY_ID).remove(&id) {
            for sc in old {
                let _ = app_clone.global_shortcut().unregister(sc);
            }
        }

        // 空字符串视为「仅注销」
        if shortcut.is_empty() {
            let _ = tx.send(Ok(()));
            return;
        }

        let new_sc = match Shortcut::from_str(&shortcut) {
            Ok(sc) => sc,
            Err(e) => {
                let _ = tx.send(Err(format!("parse '{}' failed: {:?}", shortcut, e)));
                return;
            }
        };

        // dev 构建（debug）给所有快捷键叠加 Shift，与 prod（release）区分，
        // 使 dev/prod 可并存且各响各的。配置仍只存一份（Alt 基）。
        // 与影子键同理必须经 Shortcut::new 重算 id（id 是 mods/key 派生值，
        // 只改 mods 会让 dev 键位带着未叠加 Shift 的旧 id 进注册表）。
        #[cfg(debug_assertions)]
        let new_sc = Shortcut::new(Some(new_sc.mods | Modifiers::SHIFT), new_sc.key);

        // 同一个逻辑键位的全部物理注册共用一段回调：主键与影子行为完全一致。
        let make_handler = |sid: String, label: String| {
            move |app: &tauri::AppHandle, _shortcut: &Shortcut, event: ShortcutEvent| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }

                // 录制模态：任何 ShortcutInput 处于录制态，统一抑制默认
                // 行为并把按键字符串转发给前端，由当前录制中的输入框吸收。
                // M-rs5：回调热路径 Acquire load，比 SeqCst 轻量
                if IS_RECORDING_ANY.load(Ordering::Acquire) {
                    let _ = app.emit(
                        "shortcut-recording-captured",
                        serde_json::json!({ "shortcut": label.clone() }),
                    );
                    return;
                }

                let was_visible = WINDOW_VISIBLE.load(Ordering::Acquire);
                let _ = app.emit(
                    "shortcut-pressed",
                    serde_json::json!({ "id": sid, "wasVisible": was_visible }),
                );

                let app_handle = app.clone();
                let id_for_check = sid.clone();

                let _ = app.run_on_main_thread(move || {
                    let window_hidden = !was_visible;

                    #[cfg(target_os = "macos")]
                    let front_pid: Option<i32> = crate::platform::focus::current_frontmost_pid();
                    #[cfg(not(target_os = "macos"))]
                    let front_pid: Option<i32> = None;

                    let ctx = ShortcutContext {
                        window_hidden,
                        front_pid,
                    };

                    if window_hidden {
                        #[cfg(target_os = "macos")]
                        crate::platform::focus::capture_frontmost();
                    }

                    if let Ok(hooks) = SHORTCUT_HOOKS.lock() {
                        if let Some(hook) = hooks.get(&id_for_check) {
                            if hook(&app_handle, &ctx) {
                                return;
                            }
                        }
                    }

                    // 仅 main 快捷键自动 show；扩展快捷键由前端 onExecute 控制时序
                    // （先 setActiveExtension 再 invoke show_window，避免渲染旧视图闪现）
                    if window_hidden && id_for_check == "main" {
                        crate::runtime::window::show_main(&app_handle);
                    }
                });
            }
        };

        // 显式键位永远优先于影子键位（三个方向）：
        // - 主键与其它 id 的主键重复 → 报错（Carbon 同一 chord 重复注册直接失败，
        //   显式报错比插件底层错误更友好，也避免注册表半更新）
        // - 主键与其它 id 的影子重复 → 先摘掉对方那条影子、再注册主键。顺序不可反：
        //   同一 chord 在 Carbon 层重复注册会失败（eventHotKeyExistsErr），影子仍占着
        //   该组合时主键注册必然报错
        // - 影子与其它 id 的主键重复 → 不生成影子
        let duplicate = lock_or_recover(&REGISTERED_BY_ID)
            .iter()
            .any(|(other_id, chords)| other_id != &id && chords.first() == Some(&new_sc));
        if duplicate {
            let _ = tx.send(Err(format!("快捷键 {shortcut} 已被其它动作占用")));
            return;
        }

        // 主键优先：摘掉其它 id 与主键重复的影子（须在 on_shortcut 之前，理由见上）
        {
            let mut map = lock_or_recover(&REGISTERED_BY_ID);
            for (other_id, chords) in map.iter_mut() {
                if other_id == &id {
                    continue;
                }
                if chords.len() == 2 && chords[1] == new_sc {
                    if let Some(dropped) = chords.pop() {
                        let _ = app_clone.global_shortcut().unregister(dropped);
                    }
                }
            }
        }

        if let Err(e) = app_clone
            .global_shortcut()
            .on_shortcut(new_sc, make_handler(id.clone(), shortcut.clone()))
        {
            let _ = tx.send(Err(e.to_string()));
            return;
        }
        let mut registered = vec![new_sc];

        // 影子键按需声明（bindings.secureMirror）：只有必须在安全输入下可用的动作
        // （当前仅剪贴板）才占额外一条全局键位，其余快捷键不注册影子。
        // 注册失败静默降级：主键仍然可用，只是 secure input 期间没有可落地的键。
        if secure_mirror.unwrap_or(false) {
            if let Some(mirror) = secure_mirror_chord(new_sc) {
                let shadowed = lock_or_recover(&REGISTERED_BY_ID)
                    .iter()
                    .any(|(other_id, chords)| other_id != &id && chords.first() == Some(&mirror));
                if !shadowed
                    && app_clone
                        .global_shortcut()
                        .on_shortcut(mirror, make_handler(id.clone(), shortcut.clone()))
                        .is_ok()
                {
                    registered.push(mirror);
                }
            }
        }

        lock_or_recover(&REGISTERED_BY_ID).insert(id, registered);
        let _ = tx.send(Ok(()));
    })
    .map_err(|e| e.to_string())?;

    // M-rs4：recv_timeout 兜底，避免主线程闭包异常时永久阻塞前端 invoke
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn sc(s: &str) -> Shortcut {
        Shortcut::from_str(s).expect("valid shortcut")
    }

    #[test]
    fn secure_mirror_adds_control_to_option_only_chord() {
        let base = sc("Alt+C");
        let mirror = secure_mirror_chord(base).expect("Option-only 组合应有影子键位");
        assert!(mirror.mods.contains(Modifiers::CONTROL));
        assert!(mirror.mods.contains(Modifiers::ALT));
        assert_eq!(mirror.key, base.key);
    }

    #[test]
    fn secure_mirror_has_distinct_id() {
        let base = sc("Alt+C");
        let mirror = secure_mirror_chord(base).expect("应有影子键位");
        // 注册表（plugin / global-hotkey / Carbon EventHotKeyID）全部按 HotKey.id
        // 索引：影子复用主键 id 会让两套物理注册互相顶替，注销时主键 Carbon 注册
        // 泄漏（全局吞键且无法再注册）。id 由 mods/key 派生，必须随新组合重算。
        assert_ne!(mirror.id, base.id);
        assert_eq!(mirror, sc("Ctrl+Alt+C"));
    }

    #[test]
    fn secure_mirror_covers_shift_option_and_space() {
        // 系统同样不投递 ⇧⌥ 组合与 ⌥Space，两者都要影子
        for chord in ["Shift+Alt+C", "Alt+Space"] {
            let mirror = secure_mirror_chord(sc(chord)).expect("应有影子键位");
            assert!(mirror.mods.contains(Modifiers::CONTROL), "{chord}");
        }
    }

    #[test]
    fn secure_mirror_skips_chords_without_option() {
        // 裸键 / 纯 ⇧ 不补 ⌃ 影子：⌃+字符键是终端控制键（⌃K 杀行、⌃C 中断…）
        for chord in ["K", "Shift+K"] {
            assert!(secure_mirror_chord(sc(chord)).is_none(), "{chord}");
        }
    }

    #[test]
    fn secure_mirror_skips_chords_already_carrying_command_or_control() {
        for chord in ["Ctrl+Alt+C", "CommandOrControl+C", "Ctrl+Space"] {
            assert!(secure_mirror_chord(sc(chord)).is_none(), "{chord}");
        }
    }
}
