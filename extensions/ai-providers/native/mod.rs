//! AI 提供商扩展：额度/余额用量获取（智谱 quota / DeepSeek 余额）。
//! 历史的 `ai.env` 导出 + shell source 钩子已随网关接管外部工具而移除——setup 对存量
//! 遗留（rc 注入块 / `~/.config/voidnix[/dev]/ai.env` / 旧版成对 marker）做幂等自清。

use crate::runtime::registry::Extension;
use tauri::AppHandle;

/// 提供商用量的获取与解析（智谱 quota / DeepSeek 余额），每提供商一文件。
pub mod usage;

/// 历史遗留清理：摘除 shell rc 中的 ai-providers 注入块（release 与 dev scope 一并，
/// 用户可能先后跑过两种构建）+ 旧版 `>>> voidnix-ai >>>` 成对 marker + 删 ai.env 文件与空壳目录。
/// 幂等：清理一次后后续调用全部 no-op。
fn cleanup_legacy(rc_path: &std::path::Path, env_file: &std::path::Path) -> Result<bool, String> {
    let mut touched = false;
    if rc_path.exists() {
        let existing = std::fs::read_to_string(rc_path)
            .map_err(|e| format!("读取 {} 失败: {e}", rc_path.display()))?;
        let mut cleaned = existing.clone();
        if cleaned.contains("# >>> voidnix-ai >>>") {
            cleaned = crate::runtime::shell_rc::filter_legacy_pair_markers(&cleaned, "ai");
        }
        cleaned = crate::runtime::shell_rc::filter_scope(&cleaned, "ai-providers");
        cleaned = crate::runtime::shell_rc::filter_scope(&cleaned, "ai-providers-dev");
        if cleaned != existing {
            crate::runtime::shell_rc::atomic_write_rc(rc_path, &cleaned)?;
            touched = true;
        }
    }
    if env_file.exists() {
        std::fs::remove_file(env_file)
            .map_err(|e| format!("删除 {} 失败: {e}", env_file.display()))?;
        // 目录仅含 ai.env 时一并移除空壳（非空则 remove_dir 失败，忽略）
        let _ = std::fs::remove_dir(env_file.parent().unwrap_or(env_file));
        touched = true;
    }
    Ok(touched)
}

fn cleanup_legacy_everywhere() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let config_dir = home.join(".config");
    for suffix in ["", ".dev"] {
        let env_file = config_dir.join(format!("voidnix{suffix}")).join("ai.env");
        for name in [".zshrc", ".zprofile"] {
            let rc = home.join(name);
            match cleanup_legacy(&rc, &env_file) {
                Ok(true) => log::info!(
                    "[ai-providers] cleaned legacy env injection → {}",
                    rc.display()
                ),
                Ok(false) => {}
                Err(e) => log::warn!("[ai-providers] legacy cleanup {}: {e}", rc.display()),
            }
        }
    }
}

pub struct AiProvidersExtension;

#[async_trait::async_trait]
impl Extension for AiProvidersExtension {
    fn id(&self) -> &'static str {
        "ai-providers"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        cleanup_legacy_everywhere();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_removes_blocks_and_env_file_idempotently() {
        let dir = std::env::temp_dir().join(format!("voidnix-ai-cleanup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rc = dir.join("rc").join(".zshrc");
        let env = dir.join("cfg").join("voidnix").join("ai.env");
        std::fs::create_dir_all(rc.parent().unwrap()).unwrap();
        std::fs::create_dir_all(env.parent().unwrap()).unwrap();
        std::fs::write(
            &rc,
            "# existing\n\n# >>> voidnix-ai >>>\nold\n# <<< voidnix-ai <<<\n\n# voidnix ai-providers\n[ -f \"$HOME/.config/voidnix/ai.env\" ] && source \"$HOME/.config/voidnix/ai.env\"\n\n# voidnix ai-providers-dev\n[ -f \"$HOME/.config/voidnix.dev/ai.env\" ] && source \"$HOME/.config/voidnix.dev/ai.env\"\n",
        )
        .unwrap();
        std::fs::write(&env, "export VOIDNIX_X_API_KEY='sk-1'\n").unwrap();

        assert!(cleanup_legacy(&rc, &env).unwrap());
        let text = std::fs::read_to_string(&rc).unwrap();
        assert!(text.contains("# existing"));
        assert!(!text.contains("voidnix-ai"));
        assert!(!text.contains("ai-providers"));
        assert!(!text.contains("ai.env"));
        assert!(!env.exists());
        assert!(!env.parent().unwrap().exists()); // 空壳目录一并移除

        // 幂等：二次清理 no-op
        assert!(!cleanup_legacy(&rc, &env).unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
