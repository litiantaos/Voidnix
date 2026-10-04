//! AI 网关扩展:进程内 Anthropic Messages / OpenAI Responses / OpenAI Chat Completions
//! 三协议直通反向代理 + Claude Code 接线。
//!
//! 服务器生命周期:前端配置就绪后经 `ai_gateway_sync` 推送路由表并启停;Rust 侧持久化
//! `gateway-state.json`,app 重启时 setup 直接拉起(前端就绪前的冷启动窗口 CC 无感)。
//! Claude Code 侧只接管 settings.json 自有键(备份 + 可还原,见 cc_settings)。

pub mod cc_settings;
pub mod guard;
pub mod normalize;
pub mod server;

use crate::runtime::registry::Extension;
use server::{GatewayRoute, GatewayStatus};
use tauri::AppHandle;

/// 前端单一同步入口:路由表 + 启停,返回最新状态(含绑定错误,UI 直显)。
#[tauri::command]
pub async fn ai_gateway_sync(
    app: AppHandle,
    enabled: bool,
    routes: Vec<GatewayRoute>,
) -> Result<StatusReport, String> {
    let dir = crate::runtime::storage::ext_data_dir(&app, "ai-gateway")?;
    server::GATEWAY.update(enabled, routes, &dir).await;
    status_report(&app).await
}

/// 状态查询:网关运行态 + CC 接管标记
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    #[serde(flatten)]
    gateway: GatewayStatus,
    cc_managed: bool,
}

/// backup_exists 是文件系统调用,走 spawn_blocking(同步命令在主线程执行,
/// 阻塞调用禁用——项目纪律,同 permission.rs)
async fn status_report(app: &AppHandle) -> Result<StatusReport, String> {
    let dir = crate::runtime::storage::ext_data_dir(app, "ai-gateway")?;
    let cc_managed = tokio::task::spawn_blocking(move || cc_settings::backup_exists(&dir))
        .await
        .map_err(|e| e.to_string())?;
    Ok(StatusReport {
        gateway: server::GATEWAY.status(),
        cc_managed,
    })
}

#[tauri::command]
pub async fn ai_gateway_status(app: AppHandle) -> Result<StatusReport, String> {
    status_report(&app).await
}

/// 接管 Claude Code settings.json 自有键(幂等;首次触碰自动备份)。
/// async + spawn_blocking:多文件读写不得占主线程(项目纪律,同 permission.rs)。
/// 跨实例互斥/自愈/快照都在 apply_at 内(单次读 settings,零 TOCTOU 窗口)。
#[tauri::command]
pub async fn ai_gateway_cc_apply(
    app: AppHandle,
    payload: cc_settings::CcApplyPayload,
) -> Result<(), String> {
    let dir = crate::runtime::storage::ext_data_dir(&app, "ai-gateway")?;
    tokio::task::spawn_blocking(move || cc_settings::apply(&dir, server::PORT, &payload))
        .await
        .map_err(|e| e.to_string())?
}

/// 还原 Claude Code settings.json 自有键(按快照精确还原,非本实例接管只清快照)。
#[tauri::command]
pub async fn ai_gateway_cc_remove(app: AppHandle) -> Result<(), String> {
    let dir = crate::runtime::storage::ext_data_dir(&app, "ai-gateway")?;
    tokio::task::spawn_blocking(move || cc_settings::remove(&dir, server::PORT))
        .await
        .map_err(|e| e.to_string())?
}

pub struct AiGatewayExtension;

#[async_trait::async_trait]
impl Extension for AiGatewayExtension {
    fn id(&self) -> &'static str {
        "ai-gateway"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        if let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "ai-gateway") {
            server::GATEWAY.restore_from(&dir).await;
        }
        Ok(())
    }
}
