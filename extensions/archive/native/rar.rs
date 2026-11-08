//! RAR 读路径（unrar crate：RarLab unRAR C++ 源码静态编译）：条目列表 / 选择性解压 /
//! 单条目读取，支持 RAR3/RAR5 加密档（内容加密 + 档头加密）。写入不可行——unRAR 许可
//! 禁止用其源码实现 RAR 压缩器（压缩侧仅 ZIP/7Z）。
//!
//! 密码语义与 7z 对齐：内容加密（-p）无密码可列条目并标记 encrypted；档头加密（-hp）
//! 无密码连列表都失败 → 空条目 + encrypted（解压时输入密码仍可全量解出）。

use super::extract::ExtractResult;
use super::format::ArchiveFormat;
use super::inspect::{ArchiveInfo, EntryKind, EntryMeta, MAX_ENTRIES};
use std::path::Path;

/// 密码类错误映射（MissingPassword 缺密码 / BadPassword 错密码）。
fn map_err(e: unrar::error::UnrarError, password: Option<&str>) -> String {
    use unrar::error::Code;
    match e.code {
        Code::MissingPassword => {
            if password.is_some() {
                "密码错误".into()
            } else {
                "压缩包已加密，需要密码".into()
            }
        }
        Code::BadPassword => "密码错误".into(),
        _ => e.to_string(),
    }
}

fn is_pwd_err(e: &unrar::error::UnrarError) -> bool {
    use unrar::error::Code;
    e.code == Code::MissingPassword || e.code == Code::BadPassword
}

fn archive<'a>(path: &'a Path, password: Option<&'a str>) -> UnrarArchive<'a> {
    match password {
        Some(pwd) if !pwd.is_empty() => unrar::Archive::with_password(path, pwd),
        _ => unrar::Archive::new(path),
    }
}

/// 错密码在 unrar 层常表现为数据损坏（File CRC error）而非 BadPassword——加密档上
/// 有密码 + 解码类失败一律映射「密码错误」（与 7z 垃圾流同理）。
fn err_with_pwd_hint(
    e: unrar::error::UnrarError,
    password: Option<&str>,
    encrypted: bool,
) -> String {
    let m = map_err(e, password);
    if m.contains("密码") {
        m
    } else if encrypted && password.is_some() {
        "密码错误".into()
    } else {
        m
    }
}

/// 条目列表。内容加密（-p）：无密码可列、经 entry 标记 encrypted；
/// 档头加密（-hp）：无密码列表失败 → 空条目 + encrypted（同 7z 档头加密语义）。
pub fn inspect(path: &Path) -> Result<ArchiveInfo, String> {
    let open = archive(path, None).open_for_listing();
    let mut open = match open {
        Ok(o) => o,
        Err(e) if is_pwd_err(&e) => {
            // 档头加密：列表需要密码，退化为「已加密 + 空条目」
            return Ok(ArchiveInfo {
                path: path.to_string_lossy().into_owned(),
                format: ArchiveFormat::Rar,
                entries: Vec::new(),
                total_size: 0,
                encrypted: true,
                truncated: false,
            });
        }
        Err(e) => return Err(e.to_string()),
    };
    let mut info = ArchiveInfo {
        path: path.to_string_lossy().into_owned(),
        format: ArchiveFormat::Rar,
        entries: Vec::new(),
        total_size: 0,
        encrypted: open.has_encrypted_headers(),
        truncated: false,
    };
    for header in &mut open {
        let h = match header {
            Ok(h) => h,
            // 档头加密（-hp）无密码：错误延迟到首个条目读取才抛出——同样退化为「已加密 + 空条目」
            Err(e) if is_pwd_err(&e) => {
                return Ok(ArchiveInfo {
                    path: path.to_string_lossy().into_owned(),
                    format: ArchiveFormat::Rar,
                    entries: Vec::new(),
                    total_size: 0,
                    encrypted: true,
                    truncated: false,
                })
            }
            Err(e) => return Err(e.to_string()),
        };
        if info.entries.len() >= MAX_ENTRIES {
            info.truncated = true;
            break;
        }
        if h.is_encrypted() {
            info.encrypted = true;
        }
        let kind = if h.is_directory() {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        if kind == EntryKind::File {
            info.total_size += h.unpacked_size;
        }
        info.entries.push(EntryMeta {
            path: h.filename.to_string_lossy().into_owned(),
            kind,
            size: Some(h.unpacked_size),
        });
    }
    Ok(info)
}

/// 解压（全量或选中条目）。条目名经 extract 的 sanitize_rel 清洗（防 `..`/绝对路径逃逸）。
pub fn extract(
    path: &Path,
    out_root: &Path,
    selected: &Option<Vec<String>>,
    password: Option<&str>,
    is_cancelled: &dyn Fn() -> bool,
    result: &mut ExtractResult,
) -> Result<(), String> {
    // 错密码在 unrar 层报 CRC 类错误，前置探测加密标记供文案映射
    let encrypted = inspect(path).map(|i| i.encrypted).unwrap_or(false);
    let mut open = archive(path, password)
        .open_for_processing()
        .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
    loop {
        if is_cancelled() {
            return Err("已取消".into());
        }
        let cursor = match open.read_header() {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => return Err(err_with_pwd_hint(e, password, encrypted)),
        };
        // FileHeader 未实现 Clone：先拷出所需字段再移动 cursor
        let entry = cursor.entry();
        let name = entry.filename.to_string_lossy().into_owned();
        let is_dir = entry.is_directory();
        if !super::extract::selected(selected, &name) {
            open = cursor
                .skip()
                .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
            continue;
        }
        let Some(rel) = super::extract::sanitize_rel(&name) else {
            result.skipped_unsafe += 1;
            open = cursor
                .skip()
                .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
            continue;
        };
        let out_path = out_root.join(&rel);
        if is_dir {
            if let Err(e) = std::fs::create_dir_all(&out_path) {
                return Err(e.to_string());
            }
            open = cursor
                .skip()
                .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
        } else {
            if let Some(parent) = out_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err(e.to_string());
                }
            }
            open = cursor
                .extract_to(&out_path)
                .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
            result.file_count += 1;
        }
    }
    Ok(())
}

/// 读单条目字节（预览用，cap 截断；超过 cap 的大条目不读全量，返回空 + truncated）。
pub fn read_entry(
    path: &Path,
    entry: &str,
    password: Option<&str>,
    cap: usize,
) -> Result<(Vec<u8>, bool), String> {
    let encrypted = inspect(path).map(|i| i.encrypted).unwrap_or(false);
    let mut open = archive(path, password)
        .open_for_processing()
        .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
    loop {
        let cursor = match open.read_header() {
            Ok(Some(c)) => c,
            Ok(None) => return Err(format!("条目不存在: {entry}")),
            Err(e) => return Err(err_with_pwd_hint(e, password, encrypted)),
        };
        let header = cursor.entry();
        if header.filename.to_string_lossy() != entry {
            open = cursor
                .skip()
                .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
            continue;
        }
        if header.is_directory() {
            return Err("目录无内容".into());
        }
        if header.unpacked_size > cap as u64 {
            // read() 是全量读取：大条目不整读进内存（超限直接判 truncated 走二进制展示）；
            // 中尺寸（≤8MB 内存护栏）读后截断到 cap——与 zip/7z 的部分预览语义一致
            if header.unpacked_size > 8 * 1024 * 1024 {
                return Ok((Vec::new(), true));
            }
            let (bytes, _) = cursor
                .read()
                .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
            let truncated = bytes.len() > cap;
            return Ok((bytes[..cap.min(bytes.len())].to_vec(), truncated));
        }
        let (bytes, _) = cursor
            .read()
            .map_err(|e| err_with_pwd_hint(e, password, encrypted))?;
        let truncated = bytes.len() == cap;
        return Ok((bytes, truncated));
    }
}

// unrar::Archive 借用 path/password 的生命周期参数别名（省略泛型样板）
type UnrarArchive<'a> = unrar::Archive<'a>;

#[cfg(test)]
mod tests {
    use super::*;
    // 测试档来自 unrar crate 官方 fixtures（RarLab 样例，unRAR 许可允许再分发）：
    // crypted.rar：内容加密（-p，密码 unrar），无密码可列；comment-hpw：档头加密（-hp，密码 password）
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../extensions/archive/native/testdata")
            .join(name)
    }

    fn no_cancel() -> bool {
        false
    }

    #[test]
    fn rar_content_encrypted_roundtrip() {
        let path = fixture("crypted.rar");
        // 列表：无密码可列（-p 只加密内容），条目标记 encrypted
        let info = inspect(&path).unwrap();
        assert!(info.encrypted);
        assert!(
            info.entries.iter().any(|e| e.path == ".gitignore"),
            "entries: {:?}",
            info.entries
        );

        let tmp = std::env::temp_dir().join(format!("voidnix_rar_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let mut result = ExtractResult {
            output_dir: String::new(),
            file_count: 0,
            skipped_unsafe: 0,
            skipped_symlink: 0,
        };
        // 无密码 → 需要密码；错密码 → 密码错误；正确密码 → 解出内容
        let err = extract(&path, &tmp, &None, None, &no_cancel, &mut result).unwrap_err();
        assert!(err.contains("密码"), "实际: {err}");
        let err2 = extract(&path, &tmp, &None, Some("wrong"), &no_cancel, &mut result).unwrap_err();
        assert!(err2.contains("密码错误"), "实际: {err2}");
        extract(&path, &tmp, &None, Some("unrar"), &no_cancel, &mut result).unwrap();
        assert!(result.file_count >= 1, "file_count: {}", result.file_count);

        let (bytes, _) = read_entry(&path, ".gitignore", Some("unrar"), 1024).unwrap();
        assert!(!bytes.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rar_header_encrypted_inspect() {
        // 档头加密（-hp）：无密码列表失败 → 空条目 + encrypted 标记
        let info = inspect(&fixture("comment-hpw-password.rar")).unwrap();
        assert!(info.encrypted);
        assert!(info.entries.is_empty());
    }
}
