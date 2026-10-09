//! Claude Code settings.json 自有键接管:读-合-写 + 首次触碰备份 + Rust 侧持有还原快照。
//!
//! 只动 `OWNED_POINTERS` 列出的键,用户自有配置(`CLAUDE_CODE_EFFORT_LEVEL` 等)不碰;
//! 首次触碰前写 `settings.json.voidnix-bak` 原文兜底 + `cc-backup.json` 精确还原快照,
//! 关闭接管时按快照逐键还原。serde_json `preserve_order` 特性保证用户键序不变。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::server::{port_side, GATEWAY_PORTS};

/// 网关接管的键(json pointer 路径)。`apiKeyHelper` 一并摘除:占位凭证经
/// `ANTHROPIC_AUTH_TOKEN` 注入,不再依赖 shell env(ai.env source 链路)
const OWNED_POINTERS: &[&str] = &[
    "/apiKeyHelper",
    "/env/ANTHROPIC_BASE_URL",
    "/env/ANTHROPIC_AUTH_TOKEN",
    "/env/ANTHROPIC_API_KEY",
    "/env/ANTHROPIC_DEFAULT_SONNET_MODEL",
    "/env/ANTHROPIC_DEFAULT_OPUS_MODEL",
    "/env/ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "/env/CLAUDE_CODE_AUTO_MODE_SERVER",
    "/modelPicker",
];

/// 占位凭证:网关注入真实 Key,客户端凭证仅为满足 CC 非空校验
const PLACEHOLDER_TOKEN: &str = "voidnix-claude-code";

/// 旧版占位凭证(is_managed 兼容判定用):存量接管在下一轮 sync 幂等重写前仍指旧值
const LEGACY_TOKEN: &str = "voidnix-gateway";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcPickerRow {
    pub model: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcApplyPayload {
    pub port: u16,
    /// 新会话默认模型(sonnet 档别名;空 = 移除该键)
    pub sonnet_model: String,
    /// 旗舰档模型(opus 档别名;空 = 移除该键)
    pub opus_model: String,
    /// 后台任务模型(haiku 档别名;空 = 移除该键,后台流量走主模型)
    pub haiku_model: String,
    pub picker_rows: Vec<CcPickerRow>,
}

fn cc_settings_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法解析 home 目录".to_string())?;
    Ok(home.join(".claude").join("settings.json"))
}

fn backup_file(dir: &Path) -> PathBuf {
    dir.join("cc-backup.json")
}

/// CC 接管标记(状态查询用):还原快照存在 = 处于接管态
pub fn backup_exists(dir: &Path) -> bool {
    backup_file(dir).exists()
}

/// 接管:互斥校验 → 备份(首次)→ 写自有键。幂等,重复 apply 快照不变。
pub fn apply(dir: &Path, my_port: u16, payload: &CcApplyPayload) -> Result<(), String> {
    apply_at(&cc_settings_path()?, dir, my_port, payload)
}

/// 还原:按快照恢复自有键原始值(快照缺失时退化为直接摘除自有键),并删除快照。
/// base_url 指向另一构建的网关端口时视为对侧实例的接管,本实例快照无权覆写,只清快照。
pub fn remove(dir: &Path, my_port: u16) -> Result<(), String> {
    remove_at(&cc_settings_path()?, dir, my_port)
}

fn apply_at(
    settings: &Path,
    dir: &Path,
    my_port: u16,
    payload: &CcApplyPayload,
) -> Result<(), String> {
    let mut root = read_settings(settings)?;

    // 跨实例互斥:dev/release 共管同一 settings.json,接管是排他资源。base_url 指向另一
    // 构建的网关端口且该网关仍活着(经 /health 验明正身,排除无关进程占端口的误判)→ 拒绝;
    // 探测失败 = 上次异常退出的死残留,放行覆盖自愈(下方全摘除快照保证还原路径成立)
    if let Some(p) = gateway_port_of(&root).filter(|p| *p != my_port) {
        if gateway_alive(p) {
            return Err(format!(
                "CC 正被 {} 实例接管（网关端口 {p}），请先关闭那侧的接管",
                port_side(p)
            ));
        }
    }

    // 首次触碰:原文兜底副本 + 自有键精确快照(只在快照不存在时写,重复 apply 不覆盖首次原貌)。
    // settings.json 已呈接管态时原始态不可考(死掉实例覆盖的残留,原文只存在对方的备份里):
    // 写全摘除快照(还原 = 摘除全部自有键)——既不把对方接管态存为「原始态」在还原时踩出
    // 指向对方端口的残留,又保证接管标记成立(「任一失守即还原」不变式对自愈接管者生效)
    let backup = backup_file(dir);
    if !backup.exists() {
        let snapshot = if is_managed_state(&root) {
            null_owned()
        } else {
            if settings.exists() {
                let raw = std::fs::read_to_string(settings)
                    .map_err(|e| format!("读取 {} 失败: {e}", settings.display()))?;
                let bak = settings.with_extension("json.voidnix-bak");
                if !bak.exists() {
                    std::fs::write(&bak, &raw).map_err(|e| format!("写备份失败: {e}"))?;
                }
            }
            collect_owned(&root)
        };
        let text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
        crate::runtime::storage::atomic_write(&backup, &(text + "\n"))?;
    }

    remove_pointer(&mut root, "/apiKeyHelper");
    remove_pointer(&mut root, "/env/ANTHROPIC_API_KEY");
    set_pointer(
        &mut root,
        "/env/ANTHROPIC_BASE_URL",
        Value::String(format!("http://127.0.0.1:{}", payload.port)),
    );
    set_pointer(
        &mut root,
        "/env/ANTHROPIC_AUTH_TOKEN",
        Value::String(PLACEHOLDER_TOKEN.to_string()),
    );
    // 关闭 auto mode 服务端分类器检查:应答(响应流 safeguard_results)只有 Anthropic
    // 端点会返回,网关上游是第三方时永不 eligible,CC 每会话 fallback 成立即弹计费变更
    // 提示(Enter 仅抑制 24h)。设 0 让 CC 不再请求服务端检查,分类器请求照旧自发走网关
    // 由归一特判兜底
    set_pointer(
        &mut root,
        "/env/CLAUDE_CODE_AUTO_MODE_SERVER",
        Value::String("0".to_string()),
    );
    let aliases = [
        ("/env/ANTHROPIC_DEFAULT_SONNET_MODEL", &payload.sonnet_model),
        ("/env/ANTHROPIC_DEFAULT_OPUS_MODEL", &payload.opus_model),
        ("/env/ANTHROPIC_DEFAULT_HAIKU_MODEL", &payload.haiku_model),
    ];
    for (pointer, model) in aliases {
        if model.trim().is_empty() {
            remove_pointer(&mut root, pointer);
        } else {
            set_pointer(&mut root, pointer, Value::String(model.trim().to_string()));
        }
    }
    if payload.picker_rows.is_empty() {
        remove_pointer(&mut root, "/modelPicker");
    } else {
        set_pointer(
            &mut root,
            "/modelPicker",
            build_picker(&payload.picker_rows),
        );
    }

    write_settings(settings, &root)
}

fn remove_at(settings: &Path, dir: &Path, my_port: u16) -> Result<(), String> {
    let backup = backup_file(dir);
    let snapshot: Value = if backup.exists() {
        let text = std::fs::read_to_string(&backup).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())?
    } else {
        Value::Null
    };

    // settings.json 不存在(用户手删):只清快照
    if !settings.exists() {
        let _ = std::fs::remove_file(&backup);
        return Ok(());
    }
    let mut root = read_settings(settings)?;
    // 归属校验:base_url 指向另一构建的网关端口 = 对侧实例的活动接管,本实例(可能陈旧的)
    // 快照无权覆写——只清自己的快照,防 dev 残留快照拆掉 release 的接管(互踩的还原侧)
    if gateway_port_of(&root).is_some_and(|p| p != my_port) {
        let _ = std::fs::remove_file(&backup);
        return Ok(());
    }

    for pointer in OWNED_POINTERS {
        let restored = snapshot.get(pointer.trim_start_matches('/'));
        match restored {
            Some(v) if !v.is_null() => set_pointer(&mut root, pointer, v.clone()),
            _ => remove_pointer(&mut root, pointer),
        }
    }
    // env 被清空且快照里原本无 env:整体摘除
    if let Some(env) = root.get("env") {
        if env.as_object().is_some_and(Map::is_empty) && snapshot.get("env").is_none() {
            remove_pointer(&mut root, "/env");
        }
    }

    write_settings(settings, &root)?;
    let _ = std::fs::remove_file(&backup);
    Ok(())
}

/// modelPicker(v2.1.242+;旧版本 CC 忽略未知键):注入网关全部可路由模型进 /model 选择器,
/// replaceBuiltInOptions 收敛为纯网关阵容
fn build_picker(rows: &[CcPickerRow]) -> Value {
    let options: Vec<Value> = rows
        .iter()
        .map(|r| serde_json::json!({ "model": r.model, "label": r.label }))
        .collect();
    serde_json::json!({ "options": options, "replaceBuiltInOptions": true })
}

/// 自有键快照:pointer(去首斜杠)→ 原值(null = 原本不存在)
fn collect_owned(root: &Value) -> Value {
    let mut map = Map::new();
    for pointer in OWNED_POINTERS {
        map.insert(
            pointer.trim_start_matches('/').to_string(),
            root.pointer(pointer).cloned().unwrap_or(Value::Null),
        );
    }
    Value::Object(map)
}

fn read_settings(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("settings.json 非合法 JSON: {e}"))?;
    if !v.is_object() {
        return Err("settings.json 顶层不是对象,拒绝改写".to_string());
    }
    Ok(v)
}

/// base_url 指向的网关端口解析(锚定本机回环 + 网关端口对;远程主机/带路径/缺失返回 None)
fn gateway_port_of(root: &Value) -> Option<u16> {
    let base = root.pointer("/env/ANTHROPIC_BASE_URL")?.as_str()?;
    let port = base.strip_prefix("http://127.0.0.1:")?.parse().ok()?;
    GATEWAY_PORTS.contains(&port).then_some(port)
}

/// settings.json 是否已呈网关接管态(BASE_URL 指向本地网关端口或凭证为占位符)
fn is_managed_state(root: &Value) -> bool {
    let token = root
        .pointer("/env/ANTHROPIC_AUTH_TOKEN")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    gateway_port_of(root).is_some() || token == PLACEHOLDER_TOKEN || token == LEGACY_TOKEN
}

/// 全摘除快照:所有自有键记为「原本不存在」,还原时整体摘除(接管态覆盖自愈场景的原始态不可考)
fn null_owned() -> Value {
    let mut map = Map::new();
    for pointer in OWNED_POINTERS {
        map.insert(pointer.trim_start_matches('/').to_string(), Value::Null);
    }
    Value::Object(map)
}

/// 网关端口活性探测:TCP 连接后发 GET /health,状态行 200 才认(区分 Voidnix 网关与恰好
/// 占用端口的无关进程);连接拒绝/超时/非 200 均视为死端口。blocking 调用(spawn_blocking 内)
fn gateway_alive(port: u16) -> bool {
    use std::io::{Read, Write};
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) =
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300))
    else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
    if stream
        .write_all(b"GET /health HTTP/1.0\r\nhost: 127.0.0.1\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 16];
    matches!(stream.read(&mut buf), Ok(n) if String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200"))
}

fn write_settings(path: &Path, root: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    crate::runtime::storage::atomic_write(path, &(text + "\n"))
}

// ─── json pointer 原语(serde_json 无 set/remove pointer,自实现中间层创建)──

fn set_pointer(root: &mut Value, pointer: &str, v: Value) {
    let segments: Vec<&str> = pointer.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return;
    }
    let mut cur = root;
    for seg in &segments[..segments.len() - 1] {
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        cur = cur
            .as_object_mut()
            .unwrap()
            .entry(seg.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if !cur.is_object() {
        *cur = Value::Object(Map::new());
    }
    cur.as_object_mut()
        .unwrap()
        .insert(segments.last().unwrap().to_string(), v);
}

fn remove_pointer(root: &mut Value, pointer: &str) {
    let segments: Vec<&str> = pointer.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return;
    }
    let mut cur = root;
    for seg in &segments[..segments.len() - 1] {
        if !cur.is_object() {
            return;
        }
        match cur.as_object_mut().unwrap().get_mut(*seg) {
            Some(v) => cur = v,
            None => return,
        }
    }
    if let Some(map) = cur.as_object_mut() {
        map.remove(*segments.last().unwrap());
    }
}

// ─── 单元测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> CcApplyPayload {
        CcApplyPayload {
            port: 8788,
            sonnet_model: "glm-5.3".into(),
            opus_model: "glm-5.3".into(),
            haiku_model: "glm-5.3-flash".into(),
            picker_rows: vec![
                CcPickerRow {
                    model: "glm-5.3".into(),
                    label: "GLM 5.3".into(),
                },
                CcPickerRow {
                    model: "deepseek-chat".into(),
                    label: "DeepSeek".into(),
                },
            ],
        }
    }

    fn setup(dir: &Path, settings: &Path) {
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_file(settings);
        let _ = std::fs::remove_file(settings.with_extension("json.voidnix-bak"));
        std::fs::create_dir_all(dir).unwrap();
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    }

    #[test]
    fn apply_writes_owned_keys_and_preserves_foreign() {
        let dir = std::env::temp_dir().join("voidnix-cc-1");
        let settings = dir.join("cc").join("settings.json");
        setup(&dir, &settings);
        std::fs::write(
            &settings,
            r#"{
  "apiKeyHelper": "echo $OLD",
  "env": { "ANTHROPIC_BASE_URL": "http://old", "ANTHROPIC_DEFAULT_OPUS_MODEL": "old-opus", "CLAUDE_CODE_EFFORT_LEVEL": "max" },
  "tui": "fullscreen"
}"#,
        )
        .unwrap();

        apply_at(&settings, &dir, 8788, &payload()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();

        assert_eq!(v["apiKeyHelper"], Value::Null);
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8788");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "voidnix-claude-code");
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "glm-5.3");
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"], "glm-5.3");
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "glm-5.3-flash");
        assert_eq!(v["env"]["CLAUDE_CODE_AUTO_MODE_SERVER"], "0");
        // 用户自有键不动
        assert_eq!(v["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "max");
        assert_eq!(v["tui"], "fullscreen");
        // modelPicker 注入
        assert_eq!(v["modelPicker"]["replaceBuiltInOptions"], true);
        assert_eq!(v["modelPicker"]["options"][1]["model"], "deepseek-chat");

        // 快照与原文兜底已写
        assert!(backup_file(&dir).exists());
        assert!(settings.with_extension("json.voidnix-bak").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_restores_original_exactly() {
        let dir = std::env::temp_dir().join("voidnix-cc-2");
        let settings = dir.join("cc").join("settings.json");
        setup(&dir, &settings);
        let original = r#"{"apiKeyHelper":"echo $OLD","env":{"ANTHROPIC_BASE_URL":"http://old","CLAUDE_CODE_AUTO_MODE_SERVER":"1","CLAUDE_CODE_EFFORT_LEVEL":"max"},"tui":"fullscreen"}"#;
        std::fs::write(&settings, original).unwrap();

        apply_at(&settings, &dir, 8788, &payload()).unwrap();
        apply_at(&settings, &dir, 8788, &payload()).unwrap(); // 幂等:快照仍是首次原貌

        // 用户原设 1 被接管值覆写为 0
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["env"]["CLAUDE_CODE_AUTO_MODE_SERVER"], "0");
        remove_at(&settings, &dir, 8788).unwrap();

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["apiKeyHelper"], "echo $OLD");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://old");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], Value::Null);
        // 用户原设的自有键:还原恢复原值(非自有键不动)
        assert_eq!(v["env"]["CLAUDE_CODE_AUTO_MODE_SERVER"], "1");
        assert_eq!(v["env"]["CLAUDE_CODE_EFFORT_LEVEL"], "max");
        assert_eq!(v["modelPicker"], Value::Null);
        assert!(!backup_file(&dir).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_on_missing_settings_creates_minimal() {
        let dir = std::env::temp_dir().join("voidnix-cc-3");
        let settings = dir.join("cc").join("settings.json");
        setup(&dir, &settings);

        apply_at(&settings, &dir, 8788, &payload()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8788");

        // 无原文:还原后 env 清空被整体摘除,settings 回到空对象
        remove_at(&settings, &dir, 8788).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v.as_object().map(Map::len), Some(0));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_alias_omits_env_key() {
        let dir = std::env::temp_dir().join("voidnix-cc-4");
        let settings = dir.join("cc").join("settings.json");
        setup(&dir, &settings);
        let p = CcApplyPayload {
            haiku_model: String::new(),
            ..payload()
        };
        apply_at(&settings, &dir, 8788, &p).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "glm-5.3");
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], Value::Null);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_shape_matches_cc_schema() {
        let p = build_picker(&[CcPickerRow {
            model: "glm-5.3".into(),
            label: "GLM".into(),
        }]);
        assert_eq!(
            p,
            json!({ "options": [{ "model": "glm-5.3", "label": "GLM" }], "replaceBuiltInOptions": true })
        );
    }

    #[test]
    fn gateway_port_of_recognizes_only_local_gateway_urls() {
        let base = |url: &str| json!({ "env": { "ANTHROPIC_BASE_URL": url } });
        assert_eq!(gateway_port_of(&base("http://127.0.0.1:8788")), Some(8788));
        assert_eq!(gateway_port_of(&base("http://127.0.0.1:8789")), Some(8789));
        // 远程主机同端口不误判(锚定本机回环);非网关端口与带路径后缀均不算
        assert_eq!(gateway_port_of(&base("https://relay.mycorp.io:8788")), None);
        assert_eq!(gateway_port_of(&base("http://127.0.0.1:3000")), None);
        assert_eq!(gateway_port_of(&base("http://127.0.0.1:8788/v1")), None);
        assert_eq!(gateway_port_of(&json!({ "env": {} })), None);
    }

    #[test]
    fn apply_on_managed_state_writes_null_snapshot_for_self_heal() {
        // 死残留(接管态,占位凭证)上自愈接管:原始态不可考,快照记全摘除、无原文兜底副本;
        // 还原 = 摘除全部自有键(用户自有键不动)
        let dir = std::env::temp_dir().join("voidnix-cc-5");
        let settings = dir.join("cc").join("settings.json");
        setup(&dir, &settings);
        std::fs::write(
            &settings,
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://old","ANTHROPIC_AUTH_TOKEN":"voidnix-gateway"},"tui":"fullscreen"}"#,
        )
        .unwrap();

        apply_at(&settings, &dir, 8788, &payload()).unwrap();
        assert!(!settings.with_extension("json.voidnix-bak").exists());
        let snap: Value =
            serde_json::from_str(&std::fs::read_to_string(backup_file(&dir)).unwrap()).unwrap();
        assert_eq!(snap.get("env/ANTHROPIC_BASE_URL"), Some(&Value::Null));

        remove_at(&settings, &dir, 8788).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v.pointer("/env/ANTHROPIC_BASE_URL"), None);
        assert_eq!(v.pointer("/env"), None);
        assert_eq!(v["tui"], "fullscreen");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_skips_foreign_takeover() {
        // settings 指向另一构建的网关端口(对侧活动接管):本实例陈旧快照无权覆写,
        // 只清自己的快照、settings.json 原样
        let dir = std::env::temp_dir().join("voidnix-cc-6");
        let settings = dir.join("cc").join("settings.json");
        setup(&dir, &settings);
        std::fs::write(
            &settings,
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:8789","ANTHROPIC_AUTH_TOKEN":"voidnix-gateway"}}"#,
        )
        .unwrap();
        std::fs::write(
            backup_file(&dir),
            r#"{"env/ANTHROPIC_BASE_URL":"http://mine"}"#,
        )
        .unwrap();

        let before = std::fs::read_to_string(&settings).unwrap();
        remove_at(&settings, &dir, 8788).unwrap();
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);
        assert!(!backup_file(&dir).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
