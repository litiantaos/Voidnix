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
