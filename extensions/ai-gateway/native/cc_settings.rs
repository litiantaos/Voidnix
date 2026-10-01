//! Claude Code settings.json 自有键接管:读-合-写 + 首次触碰备份 + Rust 侧持有还原快照。
//!
//! 只动 `OWNED_POINTERS` 列出的键,用户自有配置(`CLAUDE_CODE_EFFORT_LEVEL` 等)不碰;
//! 首次触碰前写 `settings.json.voidnix-bak` 原文兜底 + `cc-backup.json` 精确还原快照,
//! 关闭接管时按快照逐键还原。serde_json `preserve_order` 特性保证用户键序不变。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

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
    "/modelPicker",
];

/// 占位凭证:网关注入真实 Key,客户端凭证仅为满足 CC 非空校验
const PLACEHOLDER_TOKEN: &str = "voidnix-gateway";

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

/// 接管:备份(首次)→ 写自有键。幂等,重复 apply 快照不变。
pub fn apply(dir: &Path, payload: &CcApplyPayload) -> Result<(), String> {
    apply_at(&cc_settings_path()?, dir, payload)
}

/// 还原:按快照恢复自有键原始值(快照缺失时退化为直接摘除自有键),并删除快照。
pub fn remove(dir: &Path) -> Result<(), String> {
    remove_at(&cc_settings_path()?, dir)
}

fn apply_at(settings: &Path, dir: &Path, payload: &CcApplyPayload) -> Result<(), String> {
    let mut root = read_settings(settings)?;

    // 首次触碰:原文兜底副本 + 自有键精确快照(只在快照不存在时写,重复 apply 不覆盖首次原貌)。
    // settings.json 已呈接管态时跳过备份:dev/release 共管同一文件,快照按数据目录隔离,
    // 把对方构建的接管态存为「原始态」会在还原时踩出指向对方端口的残留
    let backup = backup_file(dir);
    if !backup.exists() && !is_managed_state(&root) {
        if settings.exists() {
            let raw = std::fs::read_to_string(settings)
                .map_err(|e| format!("读取 {} 失败: {e}", settings.display()))?;
            let bak = settings.with_extension("json.voidnix-bak");
            if !bak.exists() {
                std::fs::write(&bak, &raw).map_err(|e| format!("写备份失败: {e}"))?;
            }
        }
        let snapshot = collect_owned(&root);
        let text = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
        crate::runtime::storage::atomic_write(&backup, &(text + "\n"))?;
    }

    remove_pointer(&mut root, "/apiKeyHelper");
    remove_pointer(&mut root, "/env/ANTHROPIC_API_KEY");
    // opus 档已收敛:不写 + 摘除存量(modelPicker 替换内置阵容后别名不可达,属死配置)
    remove_pointer(&mut root, "/env/ANTHROPIC_DEFAULT_OPUS_MODEL");
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
    let aliases = [
        ("/env/ANTHROPIC_DEFAULT_SONNET_MODEL", &payload.sonnet_model),
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

fn remove_at(settings: &Path, dir: &Path) -> Result<(), String> {
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

/// settings.json 是否已呈网关接管态(BASE_URL 指向本地网关端口或凭证为占位符)
fn is_managed_state(root: &Value) -> bool {
    let base = root
        .pointer("/env/ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let token = root
        .pointer("/env/ANTHROPIC_AUTH_TOKEN")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    base.starts_with("http://127.0.0.1:87") || token == PLACEHOLDER_TOKEN
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

        apply_at(&settings, &dir, &payload()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();

        assert_eq!(v["apiKeyHelper"], Value::Null);
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8788");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "voidnix-gateway");
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "glm-5.3");
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "glm-5.3-flash");
        // opus 档已收敛:历史残留的映射键被摘除
        assert_eq!(v["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"], Value::Null);
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
        let original = r#"{"apiKeyHelper":"echo $OLD","env":{"ANTHROPIC_BASE_URL":"http://old","CLAUDE_CODE_EFFORT_LEVEL":"max"},"tui":"fullscreen"}"#;
        std::fs::write(&settings, original).unwrap();

        apply_at(&settings, &dir, &payload()).unwrap();
        apply_at(&settings, &dir, &payload()).unwrap(); // 幂等:快照仍是首次原貌
        remove_at(&settings, &dir).unwrap();

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["apiKeyHelper"], "echo $OLD");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://old");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], Value::Null);
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

        apply_at(&settings, &dir, &payload()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8788");

        // 无原文:还原后 env 清空被整体摘除,settings 回到空对象
        remove_at(&settings, &dir).unwrap();
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
        apply_at(&settings, &dir, &p).unwrap();
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
}
