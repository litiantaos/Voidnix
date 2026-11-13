//! 解压：native（zip/tar.gz，逐 entry Slip 校验 + 精确进度）与 bsdtar（其余格式）。
//!
//! 输出目录策略：恒解压到 `<dest>/<压缩包去扩展名>/`（已存在则追加序号），
//! 防止多顶层条目包把文件喷洒进用户目录；Slip 防线在此基础上再拦 entry 级逃逸。

use super::bsdtar;
use super::format::{ArchiveFormat, Engine};
use super::job::{self, ArchiveEvent};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    pub archive_path: String,
    /// 目标目录（用户 picker 选择或设置固定目录）
    pub dest_dir: String,
    /// 选中条目（None = 全部；目录条目隐含其子树）
    pub selected: Option<Vec<String>>,
    /// 解密密码（加密 zip；错误密码返回 InvalidPassword 文案）
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractResult {
    pub output_dir: String,
    pub file_count: u64,
    /// 因绝对路径 / `..` 逃逸被拒绝的条目数（恶意包信号）
    pub skipped_unsafe: u64,
    /// 符号链接目标逃逸被跳过数
    pub skipped_symlink: u64,
}

impl ExtractResult {
    /// 测试便捷构造（零计数字段）。
    #[cfg(test)]
    pub(crate) fn default_counts() -> Self {
        Self {
            output_dir: String::new(),
            file_count: 0,
            skipped_unsafe: 0,
            skipped_symlink: 0,
        }
    }
}

/// 进度节流：每 ≥2% 或完成时投递。
const PROGRESS_STEP: f64 = 2.0;

pub fn extract_blocking(
    on_event: &Channel<ArchiveEvent>,
    req: &ExtractRequest,
    token: &CancellationToken,
) -> Result<ExtractResult, String> {
    let archive = Path::new(&req.archive_path);
    if !crate::platform::path_guard::validate(archive) {
        return Err("路径不可用".into());
    }
    let format = super::format::detect(archive).ok_or("无法识别的归档格式")?;
    // 条目数一次算清（tar.gz 需完整解压一遍，后续提取复用同一分母，勿重复扫描）
    let total = match format.engine() {
        Engine::Native => estimate_total(archive, format)?,
        Engine::Bsdtar => 0,
    };
    let out_root = unique_output_dir(Path::new(&req.dest_dir), archive)?;
    std::fs::create_dir_all(&out_root).map_err(|e| e.to_string())?;

    let cancelled = || token.is_cancelled();
    let progress = |percent: f64| job::emit(on_event, ArchiveEvent::Progress { percent });
    let mut result = ExtractResult {
        output_dir: out_root.to_string_lossy().into_owned(),
        file_count: 0,
        skipped_unsafe: 0,
        skipped_symlink: 0,
    };
    let extracted = match format.engine() {
        Engine::Native => {
            job::emit(on_event, ArchiveEvent::Started { total_bytes: total });
            match format {
                ArchiveFormat::Zip => {
                    extract_zip(archive, &out_root, req, &cancelled, &progress, &mut result)
                }
                ArchiveFormat::TarGz => extract_tar_gz(
                    archive,
                    &out_root,
                    req,
                    &cancelled,
                    &progress,
                    total,
                    &mut result,
                ),
                ArchiveFormat::SevenZ => super::sevenz::extract(
                    archive,
                    &out_root,
                    &req.selected,
                    req.password.as_deref(),
                    &cancelled,
                    &mut result,
                ),
                ArchiveFormat::Rar => super::rar::extract(
                    archive,
                    &out_root,
                    &req.selected,
                    req.password.as_deref(),
                    &cancelled,
                    &mut result,
                ),
                _ => unreachable!(),
            }
        }
        Engine::Bsdtar => {
            job::emit(on_event, ArchiveEvent::Started { total_bytes: 0 });
            if token.is_cancelled() {
                Err("已取消".into())
            } else {
                match bsdtar::extract(archive, &out_root, req.selected.as_deref(), &cancelled) {
                    Ok(()) => {
                        // bsdtar 不产条目级回调：按选中判定从列表计数（不是全包文件数）
                        result.file_count = super::inspect::inspect(archive, format)
                            .map(|i| {
                                i.entries
                                    .iter()
                                    .filter(|e| {
                                        e.kind == super::inspect::EntryKind::File
                                            && selected(&req.selected, &e.path)
                                    })
                                    .count() as u64
                            })
                            .unwrap_or(0);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        }
    };
    if let Err(e) = extracted {
        // 失败/取消清掉本次新建的输出目录（unique_output_dir 保证此前不存在）
        let _ = std::fs::remove_dir_all(&out_root);
        return Err(e);
    }
    Ok(result)
}

pub(crate) fn extract_zip(
    archive: &Path,
    out_root: &Path,
    req: &ExtractRequest,
    is_cancelled: &dyn Fn() -> bool,
    progress: &dyn Fn(f64),
    result: &mut ExtractResult,
) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let total: u64 = req
        .selected
        .as_ref()
        .map(|sel| {
            zip.file_names()
                .filter_map(|n| n.ok())
                .filter(|n| sel.iter().any(|s| n.starts_with(s.as_str())))
                .count() as u64
        })
        .unwrap_or_else(|| zip.len() as u64)
        .max(1);
    let mut done: u64 = 0;
    let mut last_percent = -PROGRESS_STEP;
    for i in 0..zip.len() {
        if is_cancelled() {
            return Err("已取消".into());
        }
        // 加密条目：by_index_decrypt（密码缺失 / 错误均在此上抛明确文案）
        let mut entry =
            match zip.by_index_decrypt(i, req.password.as_deref().unwrap_or_default().as_bytes()) {
                Ok(e) => e,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("Password") || msg.contains("password") {
                        return Err(if req.password.is_some() {
                            "密码错误".into()
                        } else {
                            "压缩包已加密，需要密码".into()
                        });
                    }
                    return Err(msg);
                }
            };
        let name = entry.name().map_err(|e| e.to_string())?.to_string();
        if !selected(&req.selected, &name) {
            continue;
        }
        // 路径 sanitization：绝对路径 / `..` 逃逸一律拒绝
        // （zip 9 的 enclosed_name 会剥离绝对路径前缀而非拒绝，安全判定收归 sanitize_rel）
        let Some(rel) = sanitize_rel(&name) else {
            result.skipped_unsafe += 1;
            continue;
        };
        let out_path = out_root.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else if entry.is_symlink() {
            let mut target = String::new();
            entry
                .by_ref()
                .read_to_string(&mut target)
                .map_err(|e| e.to_string())?;
            if safe_link_target(&target) {
                std::fs::create_dir_all(out_path.parent().unwrap_or(out_root))
                    .map_err(|e| e.to_string())?;
                let _ = std::os::unix::fs::symlink(&target, &out_path);
            } else {
                result.skipped_symlink += 1;
            }
        } else {
            std::fs::create_dir_all(out_path.parent().unwrap_or(out_root))
                .map_err(|e| e.to_string())?;
            let mut out = File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry.by_ref(), &mut out).map_err(|e| e.to_string())?;
            if let Some(mode) = entry.unix_mode() {
                set_mode(&out_path, mode & 0o777);
            }
            result.file_count += 1;
        }
        done += 1;
        emit_progress(progress, done, total, &mut last_percent);
    }
    Ok(())
}

pub(crate) fn extract_tar_gz(
    archive: &Path,
    out_root: &Path,
    req: &ExtractRequest,
    is_cancelled: &dyn Fn() -> bool,
    progress: &dyn Fn(f64),
    total: u64,
    result: &mut ExtractResult,
) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let total = total.max(1);
    let mut done: u64 = 0;
    let mut last_percent = -PROGRESS_STEP;
    for entry in tar.entries().map_err(|e| e.to_string())? {
        if is_cancelled() {
            return Err("已取消".into());
        }
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        if !selected(&req.selected, &name) {
            continue;
        }
        let Some(rel) = sanitize_rel(&name) else {
            result.skipped_unsafe += 1;
            continue;
        };
        let out_path = out_root.join(&rel);
        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            }
            tar::EntryType::Symlink | tar::EntryType::Link => {
                let target = entry
                    .link_name()
                    .map_err(|e| e.to_string())?
                    .ok_or("链接条目缺少目标")?
                    .to_string_lossy()
                    .into_owned();
                if safe_link_target(&target) {
                    std::fs::create_dir_all(out_path.parent().unwrap_or(out_root))
                        .map_err(|e| e.to_string())?;
                    let _ = std::os::unix::fs::symlink(&target, &out_path);
                } else {
                    result.skipped_symlink += 1;
                }
            }
            _ => {
                std::fs::create_dir_all(out_path.parent().unwrap_or(out_root))
                    .map_err(|e| e.to_string())?;
                let mut out = File::create(&out_path).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry.by_ref(), &mut out).map_err(|e| e.to_string())?;
                if let Ok(mode) = entry.header().mode() {
                    set_mode(&out_path, mode & 0o777);
                }
                result.file_count += 1;
            }
        }
        done += 1;
        emit_progress(progress, done, total, &mut last_percent);
    }
    Ok(())
}

/// 条目选中判定：None 全选；选中目录条目隐含其子树（`dir/` 前缀匹配）。
pub(crate) fn selected(sel: &Option<Vec<String>>, name: &str) -> bool {
    match sel {
        None => true,
        Some(list) => list.iter().any(|s| {
            let prefix = if s.ends_with('/') {
                s.to_string()
            } else {
                format!("{s}/")
            };
            name == s || name.starts_with(&prefix)
        }),
    }
}

/// 相对路径清洗：拒绝绝对路径与任何 `..` 组件（tar 路径未经 zip crate sanitization）。
pub(crate) fn sanitize_rel(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return None;
    }
    let mut clean = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None, // ParentDir / RootDir / Prefix
        }
    }
    Some(clean)
}

/// 符号链接目标须为「包内相对路径」（绝对或含 `..` 一律拒绝）。
fn safe_link_target(target: &str) -> bool {
    !target.is_empty() && sanitize_rel(target).is_some()
}

/// 输出子目录：`<dest>/<stem>`，冲突追加 ` 2`、` 3`…
fn unique_output_dir(dest: &Path, archive: &Path) -> Result<PathBuf, String> {
    if !crate::platform::path_guard::validate(dest) {
        return Err("目标目录不可用".into());
    }
    let stem = archive
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".into());
    // tar.gz 等双扩展只去最后一层
    let stem = stem
        .strip_suffix(".tar")
        .map(str::to_string)
        .unwrap_or(stem);
    let base = dest.join(&stem);
    if !base.exists() {
        return Ok(base);
    }
    for n in 2.. {
        let cand = dest.join(format!("{stem} {n}"));
        if !cand.exists() {
            return Ok(cand);
        }
    }
    unreachable!()
}

/// 预估总条目数（进度分母）。
fn estimate_total(archive: &Path, format: ArchiveFormat) -> Result<u64, String> {
    Ok(super::inspect::inspect(archive, format)?.entries.len() as u64)
}

fn emit_progress(progress: &dyn Fn(f64), done: u64, total: u64, last_percent: &mut f64) {
    let percent = (done as f64 / total as f64 * 100.0).min(100.0);
    if percent - *last_percent >= PROGRESS_STEP {
        *last_percent = percent;
        progress(percent);
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

// 兼容 fmt/clippy：非 unix 分支不会编译（项目纯 macOS）
#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn no_cancel() -> bool {
        false
    }
    fn no_progress(_: f64) {}

    /// 构造含逃逸条目的恶意 zip：`../evil.txt`、绝对路径 `/abs.txt`、符号链接逃逸。
    fn malicious_zip(path: &Path) {
        let f = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("normal.txt", opts).unwrap();
        zip.write_all(b"ok").unwrap();
        zip.start_file("../evil.txt", opts).unwrap();
        zip.write_all(b"evil").unwrap();
        zip.start_file("/abs.txt", opts).unwrap();
        zip.write_all(b"abs").unwrap();
        zip.add_directory("sub/", opts).unwrap();
        zip.start_file("sub/inner.txt", opts).unwrap();
        zip.write_all(b"inner").unwrap();
        // 符号链接指向包外（逃逸）与包内（允许）；add_symlink 才携带 S_IFLNK 元数据
        zip.add_symlink("bad_link", "../../outside", opts).unwrap();
        zip.add_symlink("good_link", "sub/inner.txt", opts).unwrap();
        zip.finish().unwrap();
    }

    fn extract_zip_test(
        archive: &Path,
        out: &Path,
        selected: Option<Vec<String>>,
    ) -> ExtractResult {
        let req = ExtractRequest {
            archive_path: archive.to_string_lossy().into_owned(),
            dest_dir: out.parent().unwrap().to_string_lossy().into_owned(),
            selected,
            password: None,
        };
        let mut result = ExtractResult {
            output_dir: String::new(),
            file_count: 0,
            skipped_unsafe: 0,
            skipped_symlink: 0,
        };
        extract_zip(archive, out, &req, &no_cancel, &no_progress, &mut result).unwrap();
        result
    }

    #[test]
    fn zip_slip_and_links() {
        let tmp = std::env::temp_dir().join(format!("voidnix_archive_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let archive = tmp.join("evil.zip");
        malicious_zip(&archive);
        let out = tmp.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let result = extract_zip_test(&archive, &out, None);

        assert_eq!(result.file_count, 2, "normal + inner");
        // 逃逸条目：../evil.txt 与 /abs.txt 均被 enclosed_name 拒绝
        assert_eq!(result.skipped_unsafe, 2);
        // 逃逸符号链接被拒；包内相对符号链接放行
        assert_eq!(result.skipped_symlink, 1);
        assert_eq!(
            std::fs::read_to_string(out.join("normal.txt")).unwrap(),
            "ok"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("sub/inner.txt")).unwrap(),
            "inner"
        );
        assert!(!tmp.join("evil.txt").exists(), "../evil 不得写出");
        assert!(!Path::new("/abs.txt").exists(), "绝对路径不得写出");
        assert!(out.join("good_link").is_symlink());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn zip_selective_includes_subtree() {
        let tmp = std::env::temp_dir().join(format!("voidnix_archive_sel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let archive = tmp.join("sel.zip");
        malicious_zip(&archive);
        let out = tmp.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let result = extract_zip_test(&archive, &out, Some(vec!["sub/".to_string()]));
        assert_eq!(result.file_count, 1);
        assert!(out.join("sub/inner.txt").exists());
        assert!(!out.join("normal.txt").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rel_sanitize() {
        assert!(sanitize_rel("a/b.txt").is_some());
        assert!(sanitize_rel("./a").is_some());
        assert!(sanitize_rel("../a").is_none());
        assert!(sanitize_rel("/a").is_none());
        assert!(safe_link_target("b/c.txt"));
        assert!(!safe_link_target("/etc/passwd"));
        assert!(!safe_link_target("../../x"));
    }
}
