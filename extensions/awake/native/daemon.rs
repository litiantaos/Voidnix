//! awake 睡眠守护 daemon 的装配层：LaunchDaemon label/路径族、flag/beat 三文件、
//! 安装与活性验证。daemon 本体是 `platform::sleep::daemon_loop_script` 的常驻 sh
//! 循环，经 launchd 托管（RunAtLoad + KeepAlive），首次安装提权一次、此后跨
//! app 重启与重启机零弹窗。

use crate::platform::elevate::{shell_quote, ElevateError};
use crate::platform::sleep;
use crate::runtime::storage::ext_data_dir;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::AppHandle;

/// LaunchDaemon label（按 bundle identifier 区分 dev/prod，镜像 proxy mihomo）。
pub fn daemon_label(app: &AppHandle) -> String {
    format!("{}.awake", app.config().identifier)
}

/// 已安装 plist 路径（`/Library/LaunchDaemons` 全局可读，无需 root 比对内容）。
fn installed_plist_path(label: &str) -> PathBuf {
    PathBuf::from("/Library/LaunchDaemons").join(format!("{label}.plist"))
}

/// 持有意图 flag（稳定名：daemon 常驻跨 app 生死，不再按 pid 命名）。
fn flag_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "awake")?.join("awake.flag"))
}

/// app 心跳文件（enabled 时每 2s touch；daemon 以其 mtime 新鲜度判定 app 存活）。
fn beat_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "awake")?.join("awake.beat"))
}

/// daemon 存活标记（daemon 每周期 root touch，安装验证与活性判定的唯一信号）。
fn daemon_beat_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "awake")?.join("awake.daemon-beat"))
}

/// plist 草稿（app 生成落盘，供与安装件内容比对的版本化与安装源）。
fn draft_plist_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "awake")?.join("awake-daemon.plist"))
}

/// touch 心跳（`File::create` 截断更新 mtime，零新依赖）。enabled 为真的硬前提。
pub fn touch_beat(app: &AppHandle) -> Result<(), String> {
    let beat = beat_path(app)?;
    std::fs::File::create(&beat)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 落持有 flag（engage 在 touch beat 之后调用——写 flag 前 beat 必新鲜）。
pub fn write_flag(app: &AppHandle) -> Result<(), String> {
    let flag = flag_path(app)?;
    if let Some(parent) = flag.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&flag, b"").map_err(|e| e.to_string())
}

/// 删持有 flag（daemon ≤2s 内回落 disablesleep 0）。须先置 enabled=false（心跳
/// 任务按 enabled 决定是否补写 flag，顺序颠倒会被补写竞态复活）。
pub fn remove_flag(app: &AppHandle) {
    let _ = flag_path(app).and_then(|f| std::fs::remove_file(&f).map_err(|e| e.to_string()));
}

/// flag 是否缺席（心跳任务自愈判据）。
pub fn flag_exists(app: &AppHandle) -> bool {
    flag_path(app).map(|f| f.exists()).unwrap_or(false)
}

/// 文件 mtime 距今是否 ≤ max_age（心跳新鲜度判定；缺失/读取失败一律不新鲜）。
fn beat_is_fresh(path: &Path, max_age: u64) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() <= max_age)
}

/// daemon 是否存活（存活标记 3 周期内新鲜）。
pub fn daemon_alive(app: &AppHandle) -> bool {
    daemon_beat_path(app)
        .map(|p| beat_is_fresh(&p, sleep::DAEMON_CYCLE_SECS * 3))
        .unwrap_or(false)
}

/// 安装 LaunchDaemon（osascript 提权一次）：bootout 旧实例忽略错误 → 草稿 cat
/// 覆盖安装件 → root:wheel 644 → bootstrap 拉起。脚本零双引号，AppleScript 天然
/// 安全；bootout 前置使重装（版本升级/数据目录迁移后的内容比对不一致）幂等。
async fn install(
    app: &AppHandle,
    label: &str,
    draft: &Path,
    dest: &Path,
) -> Result<(), ElevateError> {
    let cmd = format!(
        "launchctl bootout system/{label} 2>/dev/null; \
         cat {draft} > {dest}; \
         chown root:wheel {dest}; \
         chmod 644 {dest}; \
         launchctl bootstrap system {dest}",
        draft = shell_quote(&draft.display().to_string()),
        dest = shell_quote(&dest.display().to_string()),
    );
    crate::platform::elevate::run_admin_shell(app, &cmd).await?;
    Ok(())
}

/// 确保 daemon 健康在场：plist 未装 / 内容与新生成的不一致（版本升级）/ 存活
/// 心跳过期 → 提权安装；健康时纯文件读零弹窗。安装后本进程侧轮询存活心跳
/// 验证（launchd KeepAlive 之下 bootstrap 即活，轮询兜极端启动失败）。
pub async fn ensure_daemon(app: &AppHandle) -> Result<(), String> {
    let script = sleep::daemon_loop_script(
        &flag_path(app)?.display().to_string(),
        &beat_path(app)?.display().to_string(),
        &daemon_beat_path(app)?.display().to_string(),
    );
    let label = daemon_label(app);
    let plist = sleep::awake_daemon_plist(&label, &script);
    let draft = draft_plist_path(app)?;
    std::fs::write(&draft, &plist).map_err(|e| e.to_string())?;

    let installed = installed_plist_path(&label);
    let current = std::fs::read_to_string(&installed).unwrap_or_default();
    if current == plist && daemon_alive(app) {
        return Ok(());
    }
    install(app, &label, &draft, &installed)
        .await
        .map_err(|e| match e {
            ElevateError::Cancelled => "已取消管理员授权".into(),
            ElevateError::Failed(msg) => format!("安装睡眠守护失败：{msg}"),
        })?;
    let daemon_beat = daemon_beat_path(app)?;
    for _ in 0..16 {
        if beat_is_fresh(&daemon_beat, sleep::DAEMON_CYCLE_SECS * 3) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err("睡眠守护已安装但未检测到运行心跳".into())
}
