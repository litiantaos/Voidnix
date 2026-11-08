//! 7z 读路径（sevenz-rust）：条目列表 / 选择性解压 / 单条目读取，含 AES-256 解密。
//! 写入见 compress.rs::write_sevenz。加密档档头不加密（条目名明文可见，与 zip 语义一致），
//! 无密码可列出并标记 encrypted；读内容时按 PasswordRequired/MaybeBadPassword 映射密码文案。

use super::extract::ExtractResult;
use super::format::ArchiveFormat;
use super::inspect::{ArchiveInfo, EntryKind, EntryMeta, MAX_ENTRIES};
use std::io::Read;
use std::path::Path;

/// 密码类错误 → 明确文案（密码错误 / 需要密码），其余原样上抛。
fn map_err(e: sevenz_rust::Error, password: Option<&str>) -> String {
    use sevenz_rust::Error as E;
    let pwd_err = matches!(e, E::PasswordRequired | E::MaybeBadPassword(_));
    if pwd_err {
        return if password.is_some() {
            "密码错误".into()
        } else {
            "压缩包已加密，需要密码".into()
        };
    }
    e.to_string()
}

fn open(
    path: &Path,
    password: Option<&str>,
) -> Result<sevenz_rust::SevenZReader<std::fs::File>, String> {
    let pwd = sevenz_rust::Password::from(password.unwrap_or_default());
    sevenz_rust::SevenZReader::open(path, pwd).map_err(|e| map_err(e, password))
}

/// 是否含加密内容（AES coder）。档头被第三方工具加密时无密码 open 即失败——同样视为加密。
fn is_content_encrypted(path: &Path) -> Result<bool, String> {
    match open(path, None) {
        Ok(r) => Ok(r.archive().folders.iter().any(|f| {
            f.coders
                .iter()
                .any(|c| c.decompression_method_id() == sevenz_rust::SevenZMethod::ID_AES256SHA256)
        })),
        Err(e) if e.contains("密码") => Ok(true),
        Err(e) => Err(e),
    }
}

/// 条目列表（无密码：档头不加密可读；第三方档头加密 → 仅标记 encrypted，条目空，
/// 密码在解压时输入仍可全量解出）。
pub fn inspect(path: &Path) -> Result<ArchiveInfo, String> {
    let r = match open(path, None) {
        Ok(r) => r,
        Err(e) if e.contains("密码") => {
            return Ok(ArchiveInfo {
                path: path.to_string_lossy().into_owned(),
                format: ArchiveFormat::SevenZ,
                entries: Vec::new(),
                total_size: 0,
                encrypted: true,
                truncated: false,
            })
        }
        Err(e) => return Err(e),
    };
    let archive = r.archive();
    let encrypted = archive.folders.iter().any(|f| {
        f.coders
            .iter()
            .any(|c| c.decompression_method_id() == sevenz_rust::SevenZMethod::ID_AES256SHA256)
    });
    let mut info = ArchiveInfo {
        path: path.to_string_lossy().into_owned(),
        format: ArchiveFormat::SevenZ,
        entries: Vec::new(),
        total_size: 0,
        encrypted,
        truncated: false,
    };
    for e in archive.files.iter() {
        if info.entries.len() >= MAX_ENTRIES {
            info.truncated = true;
            break;
        }
        let kind = if e.is_directory {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        if kind == EntryKind::File {
            info.total_size += e.size;
        }
        info.entries.push(EntryMeta {
            path: e.name.clone(),
            kind,
            size: Some(if e.has_stream { e.size } else { 0 }),
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
    // 前置判定：空密码解 AES 内容产垃圾流报「Corrupted input data」而非密码错误，
    // 显式拦截给出准确文案（错密码同理——解码失败即判密码错误）
    let encrypted = is_content_encrypted(path)?;
    if encrypted && password.unwrap_or_default().is_empty() {
        return Err("压缩包已加密，需要密码".into());
    }
    let mut r = open(path, password)?;
    // 取消与 IO 失败经闭包外标志位带出（for_each_entries 回调返回 sevenz Error，不便承载文案）
    let mut err: Option<String> = None;
    let mut cancelled = false;
    r.for_each_entries(|entry, reader| {
        let name = entry.name.clone();
        if !super::extract::selected(selected, &name) {
            return Ok(true);
        }
        let Some(rel) = super::extract::sanitize_rel(&name) else {
            result.skipped_unsafe += 1;
            return Ok(true);
        };
        let out_path = out_root.join(&rel);
        if entry.is_directory {
            if let Err(e) = std::fs::create_dir_all(&out_path) {
                err = Some(e.to_string());
                return Ok(false);
            }
        } else {
            let Some(parent) = out_path.parent() else {
                result.skipped_unsafe += 1;
                return Ok(true);
            };
            if let Err(e) = std::fs::create_dir_all(parent).and_then(|_| {
                let mut out = std::fs::File::create(&out_path)?;
                std::io::copy(reader, &mut out)?;
                Ok(())
            }) {
                err = Some(e.to_string());
                return Ok(false);
            }
            result.file_count += 1;
        }
        if is_cancelled() {
            cancelled = true;
            return Ok(false);
        }
        Ok(true)
    })
    .map_err(|e| {
        if encrypted && password.is_some() {
            "密码错误".into()
        } else {
            map_err(e, password)
        }
    })?;
    if let Some(msg) = err {
        // 解码 IO 失败在加密档上即错密码（垃圾流）；与 map_err 通道同映射
        return Err(if encrypted && password.is_some() {
            "密码错误".into()
        } else {
            msg
        });
    }
    if cancelled {
        return Err("已取消".into());
    }
    Ok(())
}

/// 读单条目字节（预览用，cap 截断）。
pub fn read_entry(
    path: &Path,
    entry: &str,
    password: Option<&str>,
    cap: usize,
) -> Result<(Vec<u8>, bool), String> {
    // 与 extract 同理：空密码解密内容产垃圾流，前置拦截给准确文案
    let encrypted = is_content_encrypted(path)?;
    if encrypted && password.unwrap_or_default().is_empty() {
        return Err("压缩包已加密，需要密码".into());
    }
    let mut r = open(path, password)?;
    let mut out: Option<(Vec<u8>, bool)> = None;
    let mut err: Option<String> = None;
    r.for_each_entries(|e, mut reader| {
        if e.name != entry {
            return Ok(true);
        }
        if e.is_directory {
            err = Some("目录无内容".into());
            return Ok(false);
        }
        let mut buf = Vec::with_capacity(cap.min(256 * 1024));
        match std::io::Read::take(&mut reader, cap as u64).read_to_end(&mut buf) {
            Ok(n) => out = Some((buf, n == cap)),
            Err(e2) => err = Some(e2.to_string()),
        }
        Ok(false)
    })
    .map_err(|e| {
        if encrypted && password.is_some() {
            "密码错误".into()
        } else {
            map_err(e, password)
        }
    })?;
    if let Some(msg) = err {
        return Err(if encrypted && password.is_some() {
            "密码错误".into()
        } else {
            msg
        });
    }
    out.ok_or_else(|| format!("条目不存在: {entry}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::archive::compress::{write_sevenz, ProgressCtx};

    fn no_cancel() -> bool {
        false
    }

    /// 7z 全链路：明文写入 → inspect 列表 → 无密码解压 → 单条目读取。
    #[test]
    fn sevenz_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("voidnix_7z_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("proj/sub")).unwrap();
        std::fs::write(tmp.join("proj/a.txt"), b"alpha").unwrap();
        std::fs::write(tmp.join("proj/sub/b.txt"), b"beta").unwrap();
        let collected = super::super::compress::collect(&[tmp.join("proj")], true, false).unwrap();
        let out = tmp.join("p.7z");
        let progress = |_: f64| {};
        let mut ctx = ProgressCtx {
            done: 0,
            total: collected.file_count,
            last_percent: -2.0,
            progress: &progress,
        };
        write_sevenz(&out, &collected, 6, None, &no_cancel, &mut ctx).unwrap();

        let info = inspect(&out).unwrap();
        assert!(!info.encrypted);
        let names: Vec<&str> = info.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"proj/a.txt"), "names: {names:?}");
        assert!(names.contains(&"proj/sub/b.txt"));

        let dest = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let mut result = ExtractResult {
            output_dir: String::new(),
            file_count: 0,
            skipped_unsafe: 0,
            skipped_symlink: 0,
        };
        extract(&out, &dest, &None, None, &no_cancel, &mut result).unwrap();
        assert_eq!(result.file_count, 2);
        assert_eq!(
            std::fs::read_to_string(dest.join("proj/sub/b.txt")).unwrap(),
            "beta"
        );

        let (bytes, truncated) = read_entry(&out, "proj/a.txt", None, 1024).unwrap();
        assert!(!truncated);
        assert_eq!(String::from_utf8_lossy(&bytes), "alpha");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// AES-256 加密 7z：inspect 无密码可列（档头明文）+ 标记加密；无密码/错密码拒读；正确密码可解。
    #[test]
    fn sevenz_encrypted_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("voidnix_7z_enc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("proj")).unwrap();
        std::fs::write(tmp.join("proj/secret.txt"), b"classified").unwrap();
        let collected = super::super::compress::collect(&[tmp.join("proj")], true, false).unwrap();
        let out = tmp.join("enc.7z");
        let progress = |_: f64| {};
        let mut ctx = ProgressCtx {
            done: 0,
            total: 1,
            last_percent: -2.0,
            progress: &progress,
        };
        write_sevenz(&out, &collected, 6, Some("pw123"), &no_cancel, &mut ctx).unwrap();

        let info = inspect(&out).unwrap();
        assert!(info.encrypted, "内容加密应被标记");
        assert!(
            info.entries.iter().any(|e| e.path == "proj/secret.txt"),
            "档头明文：条目名无密码可见"
        );

        let dest = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let mut result = ExtractResult {
            output_dir: String::new(),
            file_count: 0,
            skipped_unsafe: 0,
            skipped_symlink: 0,
        };
        let err = extract(&out, &dest, &None, None, &no_cancel, &mut result).unwrap_err();
        assert!(err.contains("密码"), "实际: {err}");
        let err2 = extract(&out, &dest, &None, Some("wrong"), &no_cancel, &mut result).unwrap_err();
        assert!(err2.contains("密码错误"), "实际: {err2}");
        extract(&out, &dest, &None, Some("pw123"), &no_cancel, &mut result).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("proj/secret.txt")).unwrap(),
            "classified"
        );

        let (bytes, _) = read_entry(&out, "proj/secret.txt", Some("pw123"), 1024).unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "classified");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
