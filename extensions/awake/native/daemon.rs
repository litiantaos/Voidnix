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
/// mtime 在未来（时钟后跳）按 age 0 判新鲜——与 daemon 侧 sh 的「负 age 新鲜」
/// 同向，防时钟回拨把健康 daemon 误判死亡而触发提权重装。
fn beat_is_fresh(path: &Path, max_age: u64) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| t.elapsed().unwrap_or(Duration::ZERO))
        .is_some_and(|age| age.as_secs() <= max_age)
}

/// daemon 存活判定阈值（秒）：不小于 plist 的 ThrottleInterval（10）+ 周期
/// 余量——KeepAlive 重生最长隔一个 throttle 窗口，阈值过紧会把重生间隙误判
/// 死亡、对健康 daemon 触发多余的提权重装。
const DAEMON_ALIVE_GRACE_SECS: u64 = 15;

/// daemon 是否存活（存活标记新鲜度判定）。
pub fn daemon_alive(app: &AppHandle) -> bool {
    daemon_beat_path(app)
        .map(|p| beat_is_fresh(&p, DAEMON_ALIVE_GRACE_SECS))
        .unwrap_or(false)
}

/// 安装 LaunchDaemon（osascript 提权一次）。安装脚本三重加固：
///
/// 1. **安装源 root 侧校验**：草稿先 cat 到 `.tmp`（root 属主），sha256 与
///    app 侧预期值（内嵌于 AppleScript 源、授权前已定）比对通过才原子 mv
///    覆盖安装件——草稿位于用户可写目录，授权弹窗打开的秒到分钟窗口内被
///    同用户进程替换的话，篡改件过不了哈希，杜绝经此路径的 root 提权。
/// 2. **失败不破坏现状**：草稿缺失/为空或校验失败即中止（`&&` 链），不动
///    既有安装件；`.tmp` 残留清理。
/// 3. **rm daemon 心跳标记后再 bootstrap**：重装路径下旧 daemon 刚被 bootout、
///    其 ≤2s 前的存活标记仍新鲜，会令安装后的心跳轮询空转通过（新 daemon
///    未起也判活）——先删标记，轮询只认新 daemon 的首拍。
async fn install(
    app: &AppHandle,
    label: &str,
    draft: &Path,
    dest: &Path,
    expected_plist: &str,
    daemon_beat: &Path,
) -> Result<(), ElevateError> {
    use sha2::{Digest, Sha256};
    let expected_hash: String = Sha256::digest(expected_plist.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let tmp = PathBuf::from(format!("{}.tmp", dest.display()));
    let q = shell_quote;
    let cmd = format!(
        "launchctl bootout system/{label} 2>/dev/null; \
         rm -f {daemon_beat}; \
         if [ -s {draft} ]; then \
         if cat {draft} > {tmp} \
         && [ \"$(/usr/bin/shasum -a 256 {tmp} | /usr/bin/awk '{{print $1}}')\" = {hash} ] \
         && mv {tmp} {dest} \
         && chown root:wheel {dest} \
         && chmod 644 {dest} \
         && launchctl bootstrap system {dest}; then :; \
         else rm -f {tmp}; echo VOIDNIX_INSTALL_FAILED; fi; \
         else echo VOIDNIX_DRAFT_MISSING; fi",
        draft = q(&draft.display().to_string()),
        dest = q(&dest.display().to_string()),
        tmp = q(&tmp.display().to_string()),
        daemon_beat = q(&daemon_beat.display().to_string()),
        hash = expected_hash,
    );
    let stdout = crate::platform::elevate::run_admin_shell(app, &cmd).await?;
    if stdout.contains("VOIDNIX_DRAFT_MISSING") {
        return Err(ElevateError::Failed("安装草稿缺失，请重试".into()));
    }
    if stdout.contains("VOIDNIX_INSTALL_FAILED") {
        return Err(ElevateError::Failed(
            "安装源校验失败，已保留原安装件，请重试".into(),
        ));
    }
    Ok(())
}

/// 确保 daemon 健康在场：安装件含当前 LoopVersion 令牌（循环逻辑新鲜度）且
/// 存活心跳新鲜 → 纯文件读零弹窗直通；否则提权安装（bootout + 校验安装 +
/// bootstrap）。安装后本进程侧轮询存活心跳验证，15s 覆盖 launchd 慢启动。
/// 健康快路径判定前置——草稿落盘（IO 故障会失败）只发生在确需安装时。
pub async fn ensure_daemon(app: &AppHandle) -> Result<(), String> {
    let label = daemon_label(app);
    let installed = installed_plist_path(&label);
    let version_marker = format!("<string>{}</string>", sleep::DAEMON_LOOP_VERSION);
    let current = std::fs::read_to_string(&installed).unwrap_or_default();
    if current.contains(&version_marker) && daemon_alive(app) {
        return Ok(());
    }

    let script = sleep::daemon_loop_script(
        &flag_path(app)?.display().to_string(),
        &beat_path(app)?.display().to_string(),
        &daemon_beat_path(app)?.display().to_string(),
    );
    let plist = sleep::awake_daemon_plist(&label, &script);
    let draft = draft_plist_path(app)?;
    std::fs::write(&draft, &plist).map_err(|e| e.to_string())?;

    install(
        app,
        &label,
        &draft,
        &installed,
        &plist,
        &daemon_beat_path(app)?,
    )
    .await
    .map_err(|e| match e {
        ElevateError::Cancelled => "已取消管理员授权".into(),
        ElevateError::Failed(msg) => format!("安装睡眠守护失败：{msg}"),
    })?;
    let daemon_beat = daemon_beat_path(app)?;
    for _ in 0..30 {
        if beat_is_fresh(&daemon_beat, sleep::DAEMON_CYCLE_SECS * 3) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err("睡眠守护已安装但未检测到运行心跳".into())
}
