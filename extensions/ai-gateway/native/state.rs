//! Gateway 的落盘与日志附件:持久化快照(gateway-state.json)与排障日志(gateway.log)。
//! app 重启后前端就绪前由 Rust 侧按快照直接拉起服务器,消除冷启动窗口内 CC 断连;
//! 日志只记异常路径(路由失败/上游错误/网络错误/Key 耗尽),不含 Key 明文与请求体。

use super::server::{Gateway, GatewayRoute};
use super::usage::now_ms;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// 测试复用 server 测试的路由工厂(usage/state 两模块的测试同源)
#[cfg(test)]
use super::server::tests::route as server_test_route;

/// 持久化快照(extensions/ai-gateway/gateway-state.json):app 重启后前端就绪前
/// 由 Rust 侧直接拉起服务器,消除冷启动窗口内 CC 断连。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PersistedState {
    pub(super) enabled: bool,
    pub(super) routes: Vec<GatewayRoute>,
}

fn state_file(dir: &Path) -> PathBuf {
    dir.join("gateway-state.json")
}

pub(super) fn persist_state(dir: &Path, state: &PersistedState) -> Result<(), String> {
    let path = state_file(dir);
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    crate::runtime::storage::atomic_write(&path, &(text + "\n"))
}

pub(super) fn read_state(dir: &Path) -> Result<PersistedState, String> {
    let text = std::fs::read_to_string(state_file(dir)).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

impl Gateway {
    /// 排障日志:数据目录 gateway.log,行式追加(epoch 毫秒 + 类别 + 详情,`date -r 秒`
    /// 可转)。只记异常路径(路由失败/上游错误/网络错误/Key 耗尽),成功请求零记录;
    /// 不含 Key 明文与请求体;超 512KB 整文件重置(自旋转);新建即 0600(数据目录密级
    /// 统一)。Gateway 方法而非自由函数:调用方(proxy/guard)手握实例,测试装配的
    /// bare 实例同样可写日志,不绕道全局静态
    pub(super) fn log_gateway(&self, kind: &str, detail: &str) {
        let Some(dir) = self.log_dir.read().unwrap().clone() else {
            return;
        };
        let _guard = self.log_lock.lock().unwrap();
        let path = dir.join("gateway.log");
        let line = format!("[{}] {kind} {detail}\n", now_ms());
        let created = !path.exists();
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            if f.metadata().map(|m| m.len()).unwrap_or(0) > 512 * 1024 {
                let _ = std::fs::write(&path, &line);
            } else {
                let _ = f.write_all(line.as_bytes());
            }
        }
        if created {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
}

// ─── 单元测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_state_roundtrip() {
        let dir = std::env::temp_dir().join(format!("voidnix-gw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = PersistedState {
            enabled: true,
            routes: vec![server_test_route(
                "zhipu",
                "https://x.cn/api/anthropic",
                &["glm-5.3"],
            )],
        };
        persist_state(&dir, &state).unwrap();
        let back = read_state(&dir).unwrap();
        assert!(back.enabled);
        assert_eq!(back.routes.len(), 1);
        assert_eq!(back.routes[0].models, vec!["glm-5.3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 旧 gateway-state.json(无 governed 键)反序列化兜默认 false;roundtrip 写回
    /// 恒含该键——serde default 是冷启动恢复不空路由的防线
    #[test]
    fn legacy_snapshot_without_governed_defaults_false() {
        let dir = std::env::temp_dir().join(format!("voidnix-gw-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            state_file(&dir),
            r#"{"enabled":true,"routes":[{"providerId":"z","name":"z","anthropicUrl":"https://x","responsesUrl":"","chatUrl":"","models":["m"],"keys":[{"label":"k","apiKey":"s"}]}]}"#,
        )
        .unwrap();
        let back = read_state(&dir).unwrap();
        assert_eq!(back.routes.len(), 1);
        assert!(!back.routes[0].governed);
        persist_state(&dir, &back).unwrap();
        let text = std::fs::read_to_string(state_file(&dir)).unwrap();
        assert!(text.contains("\"governed\""), "写回恒含该键");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
