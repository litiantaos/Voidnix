use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// .vnext 扩展包的 manifest.toml 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub extension: ExtensionMeta,
    pub entry: Entry,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub settings: Vec<SettingField>,
    #[serde(default)]
    pub shortcuts: Vec<ShortcutDef>,
    #[serde(default)]
    pub signature: Option<Signature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// 图标 class（如 i-ri-calculator-line）
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 兼容的 host API 大版本号
    #[serde(default = "default_voidnix_api")]
    pub voidnix_api: String,
}

fn default_voidnix_api() -> String {
    "^1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// 前端入口文件（相对于包根）
    #[serde(default = "default_entry_main")]
    pub main: String,
}

fn default_entry_main() -> String {
    "index.js".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Capabilities {
    /// 必需能力，缺失则拒绝安装
    #[serde(default)]
    pub required: Vec<String>,
    /// 可选能力，缺失时扩展需优雅降级
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    /// 首选视图类型（list/markdown/form/detail/stream）
    #[serde(default)]
    pub preferred_view: String,
    /// 搜索框占位符
    #[serde(default)]
    pub search_placeholder: String,
    /// 禁用搜索框
    #[serde(default)]
    pub disable_search_input: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SettingField {
    #[serde(rename = "text")]
    Text {
        id: String,
        label: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        default: String,
        #[serde(default)]
        required: bool,
    },
    #[serde(rename = "number")]
    Number {
        id: String,
        label: String,
        #[serde(default)]
        default: f64,
    },
    #[serde(rename = "switch")]
    Switch {
        id: String,
        label: String,
        #[serde(default = "default_false")]
        default: bool,
    },
    #[serde(rename = "select")]
    Select {
        id: String,
        label: String,
        options: Vec<SelectOption>,
        #[serde(default)]
        default: String,
    },
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutDef {
    pub id: String,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    #[serde(default)]
    pub algorithm: String,
    pub public_key: String,
    pub signature: String,
}

/// 已加载的扩展元数据
#[derive(Debug, Clone)]
pub struct LoadedExtension {
    pub manifest: Manifest,
    pub source_dir: PathBuf,
}

/// 解析 .vnext 包的 manifest.toml
pub fn parse_manifest(content: &str) -> Result<Manifest, String> {
    toml::from_str(content).map_err(|e| e.to_string())
}

/// 从目录加载扩展
pub fn load_from_dir(dir: &Path) -> Result<LoadedExtension, String> {
    let manifest_path = dir.join("manifest.toml");
    if !manifest_path.exists() {
        return Err(format!("Missing manifest.toml in {:?}", dir));
    }

    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest = parse_manifest(&content)?;

    // 验证必需文件
    let entry_path = dir.join(&manifest.entry.main);
    if !entry_path.exists() {
        return Err(format!("Missing entry file: {:?}", manifest.entry.main));
    }

    Ok(LoadedExtension {
        manifest,
        source_dir: dir.to_path_buf(),
    })
}

/// 获取 Tier 2 扩展目录：`~/Library/.../voidnix/extensions/`
pub fn tier2_extensions_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    let base = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.join("extensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_toml() -> &'static str {
        r#"
[extension]
id = "test-ext"
name = "Test"
version = "1.0.0"

[entry]
main = "index.js"
"#
    }

    #[test]
    fn parse_minimal_manifest() {
        let m = parse_manifest(minimal_toml()).unwrap();
        assert_eq!(m.extension.id, "test-ext");
        assert_eq!(m.extension.name, "Test");
        assert_eq!(m.extension.version, "1.0.0");
        assert_eq!(m.entry.main, "index.js");
    }

    #[test]
    fn parse_default_values() {
        let m = parse_manifest(minimal_toml()).unwrap();
        assert_eq!(m.extension.voidnix_api, "^1");
        assert!(m.capabilities.required.is_empty());
        assert!(m.capabilities.optional.is_empty());
        assert!(m.settings.is_empty());
        assert!(m.shortcuts.is_empty());
        assert!(m.signature.is_none());
    }

    #[test]
    fn parse_full_manifest() {
        let toml = r#"
[extension]
id = "my-ext"
name = "我的扩展"
version = "2.0.0"
description = "描述"
author = "作者"
icon = "i-ri-puzzle-line"
keywords = ["kw1", "kw2"]
voidnix_api = "^2"

[entry]
main = "main.js"

[capabilities]
required = ["clipboard.write", "http"]
optional = ["storage"]

[ui]
preferred_view = "list"
search_placeholder = "输入内容"

[[settings]]
type = "text"
id = "api_key"
label = "API Key"
placeholder = "sk-..."
required = true

[[settings]]
type = "switch"
id = "dark_mode"
label = "深色模式"

[[shortcuts]]
id = "toggle"
default = "CmdOrCtrl+Shift+T"
description = "切换"

[signature]
algorithm = "ed25519"
public_key = "pk123"
signature = "sig456"
"#;
        let m = parse_manifest(toml).unwrap();
        assert_eq!(m.extension.id, "my-ext");
        assert_eq!(m.extension.keywords, vec!["kw1", "kw2"]);
        assert_eq!(m.capabilities.required, vec!["clipboard.write", "http"]);
        assert_eq!(m.ui.preferred_view, "list");
        assert_eq!(m.settings.len(), 2);
        assert_eq!(m.shortcuts.len(), 1);
        let sig = m.signature.unwrap();
        assert_eq!(sig.algorithm, "ed25519");
    }

    #[test]
    fn parse_setting_field_variants() {
        let toml = r#"
[extension]
id = "s"
name = "S"
version = "0.1"

[entry]
main = "index.js"

[[settings]]
type = "number"
id = "count"
label = "数量"
default = 42.0

[[settings]]
type = "select"
id = "mode"
label = "模式"
default = "a"

[[settings.options]]
value = "a"
label = "A"

[[settings.options]]
value = "b"
label = "B"
"#;
        let m = parse_manifest(toml).unwrap();
        assert_eq!(m.settings.len(), 2);
    }

    #[test]
    fn parse_invalid_toml() {
        let result = parse_manifest("this is not [[[ valid");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_required_field() {
        let toml = r#"
[extension]
name = "No ID"
version = "1.0"
"#;
        let result = parse_manifest(toml);
        assert!(result.is_err());
    }

    #[test]
    fn entry_default_main() {
        let toml = r#"
[extension]
id = "e"
name = "E"
version = "1.0"

[entry]
"#;
        let m = parse_manifest(toml).unwrap();
        assert_eq!(m.entry.main, "index.js");
    }
}
