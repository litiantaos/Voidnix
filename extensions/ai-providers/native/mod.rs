//! AI 提供商扩展：写 `~/.config/voidnix[/dev]/ai.env` + 读 env 快照（供 App 内回退）。
//! 无代理、无热路径。env 文件按构建分流到 voidnix/ 或 voidnix.dev/；shell 全局投影仅 release
//! 注入——外部工具按私有名（`VOIDNIX_*`）显式引用，无法 dev/prod 并存，debug 只写文件供 App 内回退与手动 source。

use crate::runtime::registry::Extension;
use serde::Serialize;
use std::path::PathBuf;
use tauri::AppHandle;

/// 提供商用量的获取与解析（智谱 quota / DeepSeek 余额），每提供商一文件。
pub mod usage;

/// dev 构建用 `.dev` 后缀目录与 scope，release 用基础值。
/// 与 `src-tauri/tauri.conf.json`（`com.litiantao.voidnix`）/ `tauri.dev.conf.json`（`.dev`）的 bundle id 隔离一致。
const DEV_SUFFIX: &str = if cfg!(debug_assertions) { ".dev" } else { "" };

/// 导出目录：release `~/.config/voidnix` / debug `~/.config/voidnix.dev`
fn export_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法解析 home 目录".to_string())?;
    Ok(home.join(".config").join(format!("voidnix{DEV_SUFFIX}")))
}

fn env_file_path() -> Result<PathBuf, String> {
    Ok(export_dir()?.join("ai.env"))
}

fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("替换目标文件失败: {e}")
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// shell rc scope（marker `# voidnix ai-providers`；见 runtime/shell_rc）。
/// 仅 release 注入 source 钩子；debug 用此 scope 摘除历史 dev 块自愈。
const SHELL_SCOPE: &str = if cfg!(debug_assertions) {
    "ai-providers-dev"
} else {
    "ai-providers"
};

/// source 钩子 body：指向 release 的 `ai.env`（仅 release 注入）。
fn shell_hook_body() -> String {
    r#"[ -f "$HOME/.config/voidnix/ai.env" ] && source "$HOME/.config/voidnix/ai.env""#.to_string()
}

/// 迁移：摘除旧版 `>>> voidnix-ai >>>` 成对 marker（走 atomic_write_rc 留 bak）。
fn migrate_legacy_pairs(rc_path: &std::path::Path) -> Result<(), String> {
    if !rc_path.exists() {
        return Ok(());
    }
    let existing = std::fs::read_to_string(rc_path)
        .map_err(|e| format!("读取 {} 失败: {e}", rc_path.display()))?;
    if existing.contains("# >>> voidnix-ai >>>") {
        let cleaned = crate::runtime::shell_rc::filter_legacy_pair_markers(&existing, "ai");
        if cleaned != existing {
            crate::runtime::shell_rc::atomic_write_rc(rc_path, &cleaned)?;
        }
    }
    Ok(())
}

/// 维护 shell rc 钩子（统一 shell_rc 约定）。
/// release：幂等写入 source 块；debug：摘除历史 dev 块自愈——外部工具按私有名（`VOIDNIX_*`）
/// 显式引用，无法 dev/prod 并存，shell 全局投影只保留 prod，debug 凭证仅写 `voidnix.dev/ai.env` 供 App 内回退与手动 source。
fn ensure_shell_hook(rc_path: &std::path::Path) -> Result<bool, String> {
    migrate_legacy_pairs(rc_path)?;
    if cfg!(debug_assertions) {
        crate::runtime::shell_rc::remove_block(rc_path, SHELL_SCOPE)
    } else {
        crate::runtime::shell_rc::upsert_block(rc_path, SHELL_SCOPE, &shell_hook_body())
    }
}

fn ensure_user_shell_hooks() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    // zsh 主用；login 也挂 zprofile 一份（部分终端只读它）
    for name in [".zshrc", ".zprofile"] {
        let path = home.join(name);
        match ensure_shell_hook(&path) {
            Ok(true) => log::info!("[ai-providers] installed shell hook → {}", path.display()),
            Ok(false) => {}
            Err(e) => log::warn!("[ai-providers] shell hook {}: {e}", path.display()),
        }
    }
}

/// 写入 `ai.env`，并幂等安装 shell source 钩子。返回 env 文件绝对路径。
#[tauri::command]
pub fn ai_providers_export(env_text: String) -> Result<String, String> {
    let path = env_file_path()?;
    atomic_write(&path, &env_text)?;
    ensure_user_shell_hooks();
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn ai_providers_export_dir() -> Result<String, String> {
    Ok(export_dir()?.to_string_lossy().into_owned())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEnvSnapshot {
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
    /// process | file | empty
    pub source: String,
}

/// 从 shell 风格 export 行解析 KEY=VALUE（支持单引号/双引号/无引号）。
fn parse_export_line(line: &str) -> Option<(String, String)> {
    let s = line.trim();
    let s = s.strip_prefix("export ").unwrap_or(s).trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    let (k, v) = s.split_once('=')?;
    let key = k.trim();
    if key.is_empty() {
        return None;
    }
    let mut val = v.trim().to_string();
    if (val.starts_with('\'') && val.ends_with('\''))
        || (val.starts_with('"') && val.ends_with('"'))
    {
        val = val[1..val.len().saturating_sub(1)].to_string();
        // 还原 shell 单引号转义 `'\\''` → `'`
        val = val.replace("'\\''", "'");
    }
    Some((key.to_string(), val))
}

fn read_env_file_map(path: &std::path::Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        if let Some((k, v)) = parse_export_line(line) {
            map.insert(k, v);
        }
    }
    map
}

fn pick(map: &std::collections::HashMap<String, String>, keys: &[&str]) -> String {
    for k in keys {
        if let Some(v) = map.get(*k) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    String::new()
}

fn from_process() -> (String, String, String) {
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("VOIDNIX_AI_API_KEY"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let endpoint = std::env::var("OPENAI_BASE_URL")
        .or_else(|_| std::env::var("VOIDNIX_AI_BASE_URL"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let model = std::env::var("OPENAI_MODEL")
        .or_else(|_| std::env::var("VOIDNIX_AI_MODEL"))
        .unwrap_or_default()
        .trim()
        .to_string();
    (api_key, endpoint, model)
}

/// 读 OpenAI 兼容凭证：进程环境优先，否则 `~/.config/voidnix[/dev]/ai.env`。
/// Dock 启动时进程往往无 shell env，文件回退保证 App 内可用。
#[tauri::command]
pub fn ai_providers_env_snapshot() -> AiEnvSnapshot {
    let (mut api_key, mut endpoint, mut model) = from_process();
    let mut source = if !api_key.is_empty() || !endpoint.is_empty() {
        "process"
    } else {
        "empty"
    };

    if api_key.is_empty() || endpoint.is_empty() {
        if let Ok(path) = env_file_path() {
            let map = read_env_file_map(&path);
            if api_key.is_empty() {
                api_key = pick(&map, &["OPENAI_API_KEY", "VOIDNIX_AI_API_KEY"]);
            }
            if endpoint.is_empty() {
                endpoint = pick(&map, &["OPENAI_BASE_URL", "VOIDNIX_AI_BASE_URL"]);
            }
            if model.is_empty() {
                model = pick(&map, &["OPENAI_MODEL", "VOIDNIX_AI_MODEL"]);
            }
            if (!api_key.is_empty() || !endpoint.is_empty()) && source == "empty" {
                source = "file";
            }
        }
    }

    if api_key.is_empty() && endpoint.is_empty() {
        source = "empty";
    }

    AiEnvSnapshot {
        api_key,
        endpoint,
        model,
        source: source.into(),
    }
}

pub struct AiProvidersExtension;

#[async_trait::async_trait]
impl Extension for AiProvidersExtension {
    fn id(&self) -> &'static str {
        "ai-providers"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_export_variants() {
        assert_eq!(
            parse_export_line("export OPENAI_API_KEY='sk-test'"),
            Some(("OPENAI_API_KEY".into(), "sk-test".into()))
        );
        assert_eq!(
            parse_export_line(r#"OPENAI_BASE_URL="https://x.com/v1""#),
            Some(("OPENAI_BASE_URL".into(), "https://x.com/v1".into()))
        );
        assert_eq!(
            parse_export_line("export FOO=bar"),
            Some(("FOO".into(), "bar".into()))
        );
        assert_eq!(parse_export_line("# comment"), None);
    }

    #[test]
    fn shell_hook_dev_never_injects_but_migrates_and_self_heals() {
        let dir = std::env::temp_dir().join(format!("voidnix-ai-hook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        std::fs::write(
            &rc,
            "# existing\n\n# >>> voidnix-ai >>>\nold\n# <<< voidnix-ai <<<\n",
        )
        .unwrap();
        // debug 构建：迁移旧 pair marker，不注入 source 块（无 dev 块可摘 → false）
        assert!(!ensure_shell_hook(&rc).unwrap());
        let text = std::fs::read_to_string(&rc).unwrap();
        assert!(!text.contains("voidnix-ai"));
        assert!(!text.contains("voidnix ai-providers"));
        assert!(!text.contains("ai.env"));
        assert!(text.contains("# existing"));

        // 历史残留 dev 块自愈摘除
        std::fs::write(
            &rc,
            "# existing\n\n# voidnix ai-providers-dev\n[ -f \"$HOME/.config/voidnix.dev/ai.env\" ] && source \"$HOME/.config/voidnix.dev/ai.env\"\n",
        )
        .unwrap();
        assert!(ensure_shell_hook(&rc).unwrap());
        let text = std::fs::read_to_string(&rc).unwrap();
        assert!(!text.contains("voidnix ai-providers-dev"));
        assert!(!text.contains("ai.env"));
        assert!(text.contains("# existing"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
