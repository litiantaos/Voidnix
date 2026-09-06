//! Shell rc（`.zshrc` 等）注入约定。
//!
//! ## Marker 规范
//!
//! 单行注释 marker，格式固定：
//!
//! ```text
//! # voidnix <scope>
//! ```
//!
//! - `scope`：扩展 id（kebab-case），与 `extensions/<id>/` 一致  
//!   例：`zsh-autosuggestions`、`ai-providers`
//! - 块布局（与文件其余内容用空行分隔）：
//!
//! ```text
//!
//! # voidnix <scope>
//! <body 行…（一条或多条，均为非空）>
//!
//! ```
//!
//! - 识别：`line.trim() == "# voidnix <scope>"`（全行精确匹配，防误伤用户注释）
//! - 摘除：marker + 其后连续非空 body 行 + 相邻空行
//! - 写入：先摘除旧块再追加新块；内容不变则短路
//! - 落盘：`*.voidnix-bak` 备份 + tmp+rename 原子写
//!
//! 禁止使用 `>>> ... <<<` 成对 marker 或其它前缀，避免多扩展风格分裂。

use std::path::Path;

/// 生成 marker 注释行：`# voidnix <scope>`
pub fn marker_line(scope: &str) -> String {
    format!("# voidnix {scope}")
}

/// 判断 content 是否已含指定 scope 的 marker。
pub fn has_marker(content: &str, scope: &str) -> bool {
    let m = marker_line(scope);
    content.lines().any(|l| l.trim() == m)
}

/// shell 单引号包裹（POSIX）。
pub fn quote_shell(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// 从 content 摘除 `scope` 对应块（marker + 连续 body + 邻接空行）。
pub fn filter_scope(content: &str, scope: &str) -> String {
    let marker = marker_line(scope);
    let lines: Vec<&str> = content.lines().collect();
    let mut keep = vec![true; lines.len()];

    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != marker {
            i += 1;
            continue;
        }
        // 上空行
        if i > 0 && lines[i - 1].trim().is_empty() {
            keep[i - 1] = false;
        }
        keep[i] = false;
        // body：marker 后连续非空行
        let mut j = i + 1;
        while j < lines.len() && !lines[j].trim().is_empty() {
            keep[j] = false;
            j += 1;
        }
        // 下空行
        if j < lines.len() && lines[j].trim().is_empty() {
            keep[j] = false;
            j += 1;
        }
        i = j;
    }

    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| keep[*idx])
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 摘除旧块后写入新块。`body` 为无首尾换行的一行或多行。
/// 返回 `true` 表示文件有变更。
pub fn upsert_block(rc_path: &Path, scope: &str, body: &str) -> Result<bool, String> {
    let marker = marker_line(scope);
    let body = body.trim_matches('\n');
    if body.is_empty() {
        return Err("shell_rc body 不能为空".into());
    }
    // body 不得含空行（否则与「连续非空 body」摘除语义冲突）
    if body.lines().any(|l| l.trim().is_empty()) {
        return Err("shell_rc body 不得含空行".into());
    }

    let block = format!("{marker}\n{body}");
    let existing = if rc_path.exists() {
        std::fs::read_to_string(rc_path)
            .map_err(|e| format!("读取 {} 失败: {e}", rc_path.display()))?
    } else {
        String::new()
    };

    // 先摘除再拼期望全文，再与现文相等比较短路。
    // 禁止用 `existing.contains(&block)`：新 body 为旧 body 子串时会误短路并残留旧行。
    let filtered = filter_scope(&existing, scope);
    let mut new_content = filtered.trim_end_matches('\n').to_string();
    if !new_content.is_empty() {
        new_content.push_str("\n\n");
    }
    new_content.push_str(&block);
    new_content.push_str("\n\n");

    if new_content == existing {
        return Ok(false);
    }
    atomic_write_rc(rc_path, &new_content)?;
    Ok(true)
}

/// 判断行（已 trim）是否为任意 scope 的 voidnix marker（`# voidnix <scope>`，scope 非空）。
fn is_voidnix_marker(trimmed: &str) -> bool {
    trimmed.starts_with("# voidnix ") && trimmed.len() > "# voidnix ".len()
}

/// 摘除 content 中**全部** `# voidnix <scope>` 块（任意 scope，含相邻空行）。
/// 卸载清理用：不依赖 scope 清单，未来新增扩展注入自动覆盖。
pub fn filter_all_voidnix(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut keep = vec![true; lines.len()];

    let mut i = 0;
    while i < lines.len() {
        if !is_voidnix_marker(lines[i].trim()) {
            i += 1;
            continue;
        }
        // 与 filter_scope 同构：上空行 + marker + 连续非空 body + 下空行
        if i > 0 && lines[i - 1].trim().is_empty() {
            keep[i - 1] = false;
        }
        keep[i] = false;
        let mut j = i + 1;
        while j < lines.len() && !lines[j].trim().is_empty() {
            keep[j] = false;
            j += 1;
        }
        if j < lines.len() && lines[j].trim().is_empty() {
            keep[j] = false;
            j += 1;
        }
        i = j;
    }

    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| keep[*idx])
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 摘除 scope 块。返回 `true` 表示有删除。
pub fn remove_block(rc_path: &Path, scope: &str) -> Result<bool, String> {
    if !rc_path.exists() {
        return Ok(false);
    }
    let existing = std::fs::read_to_string(rc_path)
        .map_err(|e| format!("读取 {} 失败: {e}", rc_path.display()))?;
    if !has_marker(&existing, scope) {
        return Ok(false);
    }
    let filtered = filter_scope(&existing, scope);
    let new_content = if existing.ends_with('\n') && !filtered.is_empty() {
        format!("{filtered}\n")
    } else {
        filtered
    };
    if new_content == existing {
        return Ok(false);
    }
    atomic_write_rc(rc_path, &new_content)?;
    Ok(true)
}

/// 原子写 rc：备份 `*.voidnix-bak` + tmp+rename。
pub(crate) fn atomic_write_rc(rc_path: &Path, content: &str) -> Result<(), String> {
    let parent = rc_path.parent().unwrap_or(Path::new("."));
    let file_name = rc_path.file_name().and_then(|s| s.to_str()).unwrap_or("rc");
    let bak = parent.join(format!("{file_name}.voidnix-bak"));
    let tmp = parent.join(format!("{file_name}.voidnix-tmp"));

    if rc_path.exists() {
        if let Err(e) = std::fs::copy(rc_path, &bak) {
            log::warn!("shell_rc: backup {} failed: {e}", rc_path.display());
        }
    }
    std::fs::write(&tmp, content).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("write {} tmp: {e}", rc_path.display())
    })?;
    std::fs::rename(&tmp, rc_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename {}: {e}", rc_path.display())
    })?;
    Ok(())
}

/// 摘除历史遗留的成对 marker 块（仅迁移用）。
/// 匹配 `# >>> voidnix-<tag> >>>` … `# <<< voidnix-<tag> <<<`。
/// 缺 end 时**整段保留**（不把 EOF 当隐式 end，避免删半个 rc）。
pub fn filter_legacy_pair_markers(content: &str, tag: &str) -> String {
    let begin = format!("# >>> voidnix-{tag} >>>");
    let end = format!("# <<< voidnix-{tag} <<<");
    let lines: Vec<&str> = content.lines().collect();
    let mut keep = vec![true; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != begin {
            i += 1;
            continue;
        }
        // 先找 end；找不到则跳过 begin，不改任何 keep
        let mut j = i + 1;
        let mut found_end = false;
        while j < lines.len() {
            if lines[j].trim() == end {
                found_end = true;
                break;
            }
            j += 1;
        }
        if !found_end {
            i += 1;
            continue;
        }
        if i > 0 && lines[i - 1].trim().is_empty() {
            keep[i - 1] = false;
        }
        // begin..=end
        for slot in keep.iter_mut().take(j + 1).skip(i) {
            *slot = false;
        }
        j += 1;
        if j < lines.len() && lines[j].trim().is_empty() {
            keep[j] = false;
            j += 1;
        }
        i = j;
    }
    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| keep[*idx])
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 提取 content 中出现过的 legacy pair tag（`# >>> voidnix-<tag> >>>` 行）。
fn legacy_pair_tags(content: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("# >>> voidnix-") {
            if let Some(tag) = rest.strip_suffix(" >>>") {
                if !tag.is_empty() && !tags.iter().any(|x| x == tag) {
                    tags.push(tag.to_string());
                }
            }
        }
    }
    tags
}

/// 清理单个 rc 文件：摘除全部 `# voidnix` 块 + legacy pair 块，有变更才落盘（留 bak）。
/// 同时删除 `*.voidnix-bak` 备份。返回 `true` 表示该文件有清理动作。
fn clear_rc_file(rc_path: &Path) -> Result<bool, String> {
    let mut touched = false;
    if rc_path.exists() {
        let existing = std::fs::read_to_string(rc_path)
            .map_err(|e| format!("读取 {} 失败: {e}", rc_path.display()))?;
        let mut cleaned = filter_all_voidnix(&existing);
        for tag in legacy_pair_tags(&existing) {
            cleaned = filter_legacy_pair_markers(&cleaned, &tag);
        }
        if cleaned != existing {
            // 保尾换行（镜像 remove_block）：POSIX 文本文件以换行结尾；
            // 幂等比较用最终内容（cleaned 与 existing 的尾换行差异不构成变更）
            let new_content = if existing.ends_with('\n') && !cleaned.is_empty() {
                format!("{cleaned}\n")
            } else {
                cleaned
            };
            if new_content != existing {
                atomic_write_rc(rc_path, &new_content)?;
                touched = true;
            }
        }
    }
    // 备份路径镜像 atomic_write_rc 的拼接（dotfile 如 .zshrc 的 extension() 为 None）
    let file_name = rc_path.file_name().and_then(|s| s.to_str()).unwrap_or("rc");
    let bak = rc_path.with_file_name(format!("{file_name}.voidnix-bak"));
    if bak.exists() && std::fs::remove_file(&bak).is_ok() {
        touched = true;
    }
    Ok(touched)
}

/// 清除 Voidnix 在用户 shell 环境的全部注入（设置页「清除 Voidnix 注入」）：
/// - `~/.zshrc` / `~/.zprofile`：摘除全部 `# voidnix <scope>` 块 + 旧版成对 marker，删 `*.voidnix-bak` 备份
/// - `~/.config/voidnix[/dev]/ai.env`：AI 凭证明文投影（0600，含 API Key）
///
/// 卸载导向的清理入口：继续使用相关功能时会重新写入（保存提供商配置重导出 ai.env 并
/// 重装 source 钩子，zsh 补全重新启用）。返回被清理的文件路径列表（供前端反馈）。
#[tauri::command]
pub fn clear_voidnix_injections() -> Result<Vec<String>, String> {
    let Some(home) = dirs::home_dir() else {
        return Err("无法解析 home 目录".into());
    };
    let mut cleared: Vec<String> = Vec::new();
    for name in [".zshrc", ".zprofile"] {
        let path = home.join(name);
        match clear_rc_file(&path) {
            Ok(true) => cleared.push(path.display().to_string()),
            Ok(false) => {}
            Err(e) => return Err(e),
        }
    }
    for suffix in ["voidnix", "voidnix.dev"] {
        let dir = home.join(".config").join(suffix);
        let env = dir.join("ai.env");
        if env.exists() && std::fs::remove_file(&env).is_ok() {
            cleared.push(env.display().to_string());
            // 目录仅含 ai.env 时一并移除空壳（非空则 remove_dir 失败，忽略）
            let _ = std::fs::remove_dir(&dir);
        }
    }
    Ok(cleared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn marker_format() {
        assert_eq!(marker_line("ai-providers"), "# voidnix ai-providers");
        assert_eq!(
            marker_line("zsh-autosuggestions"),
            "# voidnix zsh-autosuggestions"
        );
    }

    #[test]
    fn filter_all_voidnix_removes_every_scope() {
        let raw = "export A=1\n\
                   \n\
                   # voidnix ai-providers\n\
                   source ~/.config/voidnix/ai.env\n\
                   \n\
                   # user comment mentions voidnix marker\n\
                   export B=2\n\
                   \n\
                   # voidnix zsh-autosuggestions\n\
                   export ZSH_AS_DIR=/x\n\
                   \n\
                   export C=3\n";
        let out = filter_all_voidnix(raw);
        assert!(!out.contains("# voidnix"));
        assert!(!out.contains("ai.env"));
        assert!(!out.contains("ZSH_AS_DIR"));
        // 用户内容与提及 marker 的普通注释不受影响
        assert!(out.contains("export A=1"));
        assert!(out.contains("# user comment mentions voidnix marker"));
        assert!(out.contains("export B=2"));
        assert!(out.contains("export C=3"));
    }

    #[test]
    fn legacy_pair_tags_extraction() {
        let raw = "# >>> voidnix-ai >>>\nold\n# <<< voidnix-ai <<<\n# >>> voidnix-zsh >>>\n";
        // 缺 end 的 begin 也提取（filter_legacy_pair_markers 自身缺 end 保留整段，安全）
        assert_eq!(
            legacy_pair_tags(raw),
            vec!["ai".to_string(), "zsh".to_string()]
        );
    }

    #[test]
    fn clear_rc_file_strips_all_and_bak() {
        let dir = std::env::temp_dir().join(format!("voidnix-shell-clear-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        fs::write(
            &rc,
            "export A=1\n\n# voidnix ai-providers\nsource x\n\n# >>> voidnix-ai >>>\nold\n# <<< voidnix-ai <<<\n",
        )
        .unwrap();
        fs::write(dir.join(".zshrc.voidnix-bak"), "backup").unwrap();

        assert!(clear_rc_file(&rc).unwrap());
        let text = fs::read_to_string(&rc).unwrap();
        // 用户内容保留 + 尾部换行保持
        assert_eq!(text, "export A=1\n");
        assert!(!dir.join(".zshrc.voidnix-bak").exists());
        // 幂等：再次清理无动作
        assert!(!clear_rc_file(&rc).unwrap());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_and_upsert_roundtrip() {
        let dir = std::env::temp_dir().join(format!("voidnix-shell-rc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        fs::write(&rc, "# user stuff\nexport FOO=1\n").unwrap();

        assert!(upsert_block(
            &rc,
            "ai-providers",
            r#"[ -f "$HOME/x" ] && source "$HOME/x""#
        )
        .unwrap());
        let text = fs::read_to_string(&rc).unwrap();
        assert!(text.contains("# voidnix ai-providers"));
        assert!(text.contains("export FOO=1"));

        // idempotent
        assert!(!upsert_block(
            &rc,
            "ai-providers",
            r#"[ -f "$HOME/x" ] && source "$HOME/x""#
        )
        .unwrap());

        // replace body
        assert!(upsert_block(&rc, "ai-providers", "export BAR=2").unwrap());
        let text = fs::read_to_string(&rc).unwrap();
        assert_eq!(text.matches("# voidnix ai-providers").count(), 1);
        assert!(text.contains("export BAR=2"));
        assert!(!text.contains("$HOME/x"));

        assert!(remove_block(&rc, "ai-providers").unwrap());
        let text = fs::read_to_string(&rc).unwrap();
        assert!(!text.contains("voidnix ai-providers"));
        assert!(text.contains("export FOO=1"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_legacy_pair() {
        let raw = "a\n\n# >>> voidnix-ai >>>\nold\n# <<< voidnix-ai <<<\n\nb\n";
        let out = filter_legacy_pair_markers(raw, "ai");
        assert!(!out.contains("voidnix-ai"));
        assert!(out.contains('a'));
        assert!(out.contains('b'));
    }

    #[test]
    fn filter_legacy_pair_missing_end_preserves_rest() {
        let raw = "a\n# >>> voidnix-ai >>>\nold\nexport KEEP=1\nb\n";
        let out = filter_legacy_pair_markers(raw, "ai");
        assert_eq!(out, "a\n# >>> voidnix-ai >>>\nold\nexport KEEP=1\nb");
    }

    #[test]
    fn upsert_shrink_multiline_body_removes_leftover() {
        let dir =
            std::env::temp_dir().join(format!("voidnix-shell-rc-shrink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let rc = dir.join(".zshrc");
        fs::write(&rc, "export FOO=1\n").unwrap();

        assert!(upsert_block(&rc, "ai-providers", "export BAR=2\nexport BAZ=3").unwrap());
        assert!(upsert_block(&rc, "ai-providers", "export BAR=2").unwrap());
        let text = fs::read_to_string(&rc).unwrap();
        assert!(text.contains("export BAR=2"));
        assert!(!text.contains("export BAZ=3"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn quote_shell_escapes() {
        assert_eq!(quote_shell("a'b"), r"'a'\''b'");
    }
}
