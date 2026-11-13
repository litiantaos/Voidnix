//! 归档内容列表：crate 直读（zip/tar.gz/7z，含精确大小）或系统 bsdtar（纯文件名，大小未知）。

use super::extract::sanitize_rel;
use super::format::{ArchiveFormat, Engine};
use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// 单条目最多列出数（防超大包刷屏/内存膨胀）。
pub const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    /// 归档文件绝对路径
    pub path: String,
    pub format: ArchiveFormat,
    pub entries: Vec<EntryMeta>,
    /// 全部文件未压缩字节数（bsdtar 路径未知则 0）
    pub total_size: u64,
    /// 含加密条目（当前版本不支持解密，提示后拒绝）
    pub encrypted: bool,
    /// 条目数超 MAX_ENTRIES 被截断
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntryMeta {
    /// 归档内相对路径（目录以 `/` 结尾）
    pub path: String,
    pub kind: EntryKind,
    /// 未压缩大小（bsdtar 路径未知）
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

pub fn inspect(path: &Path, format: ArchiveFormat) -> Result<ArchiveInfo, String> {
    match format.engine() {
        Engine::Native => inspect_native(path, format),
        Engine::Bsdtar => inspect_bsdtar(path, format),
    }
}

fn inspect_native(path: &Path, format: ArchiveFormat) -> Result<ArchiveInfo, String> {
    let mut info = ArchiveInfo {
        path: path.to_string_lossy().into_owned(),
        format,
        entries: Vec::new(),
        total_size: 0,
        encrypted: false,
        truncated: false,
    };
    match format {
        ArchiveFormat::Zip => {
            let file = File::open(path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            for i in 0..zip.len() {
                if info.entries.len() >= MAX_ENTRIES {
                    info.truncated = true;
                    break;
                }
                let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
                // 逃逸条目以原始名展示（zip 9 的 enclosed_name 会剥离绝对前缀而非拒绝，判定收归 sanitize_rel）
                let raw_name = entry
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|e| e.to_string());
                let name = sanitize_rel(&raw_name)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(raw_name);
                if entry.encrypted() {
                    info.encrypted = true;
                }
                let (kind, size) = if entry.is_dir() {
                    (EntryKind::Dir, Some(0))
                } else if entry.is_symlink() {
                    (EntryKind::Symlink, Some(entry.size()))
                } else {
                    (EntryKind::File, Some(entry.size()))
                };
                if kind == EntryKind::File {
                    info.total_size += entry.size();
                }
                info.entries.push(EntryMeta {
                    path: name,
                    kind,
                    size,
                });
            }
        }
        ArchiveFormat::TarGz => {
            let file = File::open(path).map_err(|e| e.to_string())?;
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
            for entry in tar.entries().map_err(|e| e.to_string())? {
                if info.entries.len() >= MAX_ENTRIES {
                    info.truncated = true;
                    break;
                }
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry
                    .path()
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned();
                let (kind, size) = match entry.header().entry_type() {
                    tar::EntryType::Directory => (EntryKind::Dir, Some(0)),
                    tar::EntryType::Symlink => (EntryKind::Symlink, Some(0)),
                    _ => (EntryKind::File, Some(entry.size())),
                };
                if kind == EntryKind::File {
                    info.total_size += entry.size();
                }
                info.entries.push(EntryMeta { path, kind, size });
            }
        }
        ArchiveFormat::SevenZ => return super::sevenz::inspect(path),
        ArchiveFormat::Rar => return super::rar::inspect(path),
        _ => unreachable!("native 引擎仅 zip/tar.gz/7z/rar"),
    }
    Ok(info)
}

/// bsdtar 路径：`tar -tf` 纯文件名列表（目录尾随 `/`）。
/// 刻意不解析 `-tvf` 的权限/日期列——locale 差异 + 文件名含空格让列解析脆弱，大小信息留空。
fn inspect_bsdtar(path: &Path, format: ArchiveFormat) -> Result<ArchiveInfo, String> {
    let output = super::bsdtar::run(&["-tf".to_string(), path.to_string_lossy().into_owned()])?;
    let mut info = ArchiveInfo {
        path: path.to_string_lossy().into_owned(),
        format,
        entries: Vec::new(),
        total_size: 0,
        encrypted: false,
        truncated: false,
    };
    for raw in output.lines() {
        if raw.is_empty() {
            continue;
        }
        // 归一化：剥去 bsdtar 列表的 `./` 前缀（匹配/回传用归一名，`--` 操作数下可直接命中）
        let name = raw.strip_prefix("./").unwrap_or(raw);
        if info.entries.len() >= MAX_ENTRIES {
            info.truncated = true;
            break;
        }
        let kind = if name.ends_with('/') {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        info.entries.push(EntryMeta {
            path: name.to_string(),
            kind,
            size: None,
        });
    }
    Ok(info)
}

/// 读归档内单条目字节（预览用，cap 截断）。native 引擎随机访问 / 顺序扫描，bsdtar 流式。
pub fn read_entry(
    path: &Path,
    format: ArchiveFormat,
    entry: &str,
    password: Option<&str>,
) -> Result<(Vec<u8>, bool), String> {
    let cap = super::preview::PREVIEW_CAP + 1;
    match format.engine() {
        Engine::Native => match format {
            ArchiveFormat::Zip => {
                let file = File::open(path).map_err(|e| e.to_string())?;
                let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
                let idx = zip
                    .index_for_name(entry)
                    .ok_or_else(|| format!("条目不存在: {entry}"))?;
                let mut e = match zip.by_index_decrypt(idx, password.unwrap_or_default().as_bytes())
                {
                    Ok(e) => e,
                    Err(err) => {
                        let msg = err.to_string();
                        if msg.contains("Password") || msg.contains("password") {
                            return Err(if password.is_some() {
                                "密码错误".into()
                            } else {
                                "压缩包已加密，需要密码".into()
                            });
                        }
                        return Err(msg);
                    }
                };
                if e.is_dir() {
                    return Err("目录无内容".into());
                }
                let mut buf = Vec::with_capacity(cap.min(PREVIEW_ALLOC));
                let n = e
                    .by_ref()
                    .take(cap as u64)
                    .read_to_end(&mut buf)
                    .map_err(|e| e.to_string())?;
                Ok((buf, n == cap))
            }
            ArchiveFormat::TarGz => {
                let file = File::open(path).map_err(|e| e.to_string())?;
                let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
                for e in tar.entries().map_err(|e| e.to_string())? {
                    let mut e = e.map_err(|e| e.to_string())?;
                    if e.path().map_err(|e| e.to_string())?.to_string_lossy() != entry {
                        continue;
                    }
                    if e.header().entry_type() == tar::EntryType::Directory {
                        return Err("目录无内容".into());
                    }
                    let mut buf = Vec::with_capacity(cap.min(PREVIEW_ALLOC));
                    let n = e
                        .by_ref()
                        .take(cap as u64)
                        .read_to_end(&mut buf)
                        .map_err(|e| e.to_string())?;
                    return Ok((buf, n == cap));
                }
                Err(format!("条目不存在: {entry}"))
            }
            ArchiveFormat::SevenZ => super::sevenz::read_entry(path, entry, password, cap),
            ArchiveFormat::Rar => super::rar::read_entry(path, entry, password, cap),
            _ => unreachable!(),
        },
        Engine::Bsdtar => super::bsdtar::stream_entry(path, entry, cap),
    }
}

/// 预读分配上限（防 cap 巨大时一次性预分配）。
const PREVIEW_ALLOC: usize = 256 * 1024;
