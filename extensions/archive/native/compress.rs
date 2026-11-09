//! 压缩：zip（Deflated）与 7z（LZMA2，均可选 AES-256）；walkdir 收集 + 垃圾/隐藏过滤（junk.rs）。
//!
//! 归档结构：单文件 → 根级以原文件名直存；目录/多输入 → 保留各自顶层名字。
//! RAR 创建不可支持——编码器专有（WinRAR），开源生态仅能解压（libarchive/unrar 只读）。

use super::job::{self, ArchiveEvent};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompressFormat {
    Zip,
    SevenZ,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressRequest {
    /// 输入文件/目录（≥1）
    pub input_paths: Vec<String>,
    /// 归档完整输出路径（前端拼好扩展名）
    pub output_path: String,
    pub format: CompressFormat,
    /// 压缩等级 1-9（UI 三档预设 1/6/9）
    pub level: u8,
    /// 过滤 macOS/Windows 系统垃圾文件（.DS_Store / __MACOSX / ._前缀等）
    pub junk_filter: bool,
    /// 过滤隐藏文件/文件夹（点开头组件，含 .git 等）
    pub hidden_filter: bool,
    /// 压缩密码（zip / 7z：AES-256）
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressResult {
    pub output_path: String,
    pub file_count: u64,
    /// 被垃圾过滤剔除的条目数
    pub skipped_junk: u64,
    pub size_bytes: u64,
}

#[derive(Debug)]
pub(crate) struct Collected {
    /// (归档内相对路径, 磁盘绝对路径, 是目录)
    pub(crate) entries: Vec<(String, PathBuf, bool)>,
    pub(crate) skipped_junk: u64,
    pub(crate) file_count: u64,
    /// 文件总字节数（进度分母；符号链接按 0 计——无内容流）
    pub(crate) total_bytes: u64,
}

pub fn compress_blocking(
    on_event: &Channel<ArchiveEvent>,
    req: &CompressRequest,
    token: &CancellationToken,
) -> Result<CompressResult, String> {
    if req.input_paths.is_empty() {
        return Err("未选择输入".into());
    }
    let inputs: Vec<PathBuf> = req.input_paths.iter().map(PathBuf::from).collect();
    for p in &inputs {
        if !crate::platform::path_guard::validate(p) {
            return Err(format!("路径不可用: {}", p.display()));
        }
        if !p.exists() {
            return Err(format!("路径不存在: {}", p.display()));
        }
    }
    let collected = collect(&inputs, req.junk_filter, req.hidden_filter)?;
    let output = unique_output(Path::new(&req.output_path));
    let output = output.as_path();
    // 收集成功后再建目录：失败不留空目录
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // 进度按字节推进（按文件条目计会让大文件长时间钉在 0%、再整段跳变）
    let total = collected.total_bytes.max(1);
    job::emit(on_event, ArchiveEvent::Started { total_bytes: total });
    let cancelled = || token.is_cancelled();
    let progress = |percent: f64| job::emit(on_event, ArchiveEvent::Progress { percent });
    let mut ctx = ProgressCtx {
        done: 0,
        total,
        last_percent: -2.0,
        progress: &progress,
    };
    let written = match req.format {
        CompressFormat::Zip => write_zip(
            output,
            &collected,
            req.level,
            req.password.as_deref(),
            &cancelled,
            &mut ctx,
        ),
        CompressFormat::SevenZ => write_sevenz(
            output,
            &collected,
            req.level,
            req.password.as_deref(),
            &cancelled,
            &mut ctx,
        ),
    };
    if let Err(e) = written {
        // 失败/取消不留半成品：output 是本次 unique_output 选出的新路径
        let _ = std::fs::remove_file(output);
        return Err(e);
    }
    let size_bytes = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    Ok(CompressResult {
        output_path: output.to_string_lossy().into_owned(),
        file_count: collected.file_count,
        skipped_junk: collected.skipped_junk,
        size_bytes,
    })
}

/// 输出防覆盖：已存在则追加 ` 2`、` 3`…（含扩展名前）。
fn unique_output(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new("."));
    for n in 2.. {
        let cand = parent.join(format!("{stem} {n}{ext}"));
        if !cand.exists() {
            return cand;
        }
    }
    unreachable!()
}

/// 遍历输入收集条目：目录递归；文件直存；垃圾/隐藏条目剔除计数。
pub(crate) fn collect(
    inputs: &[PathBuf],
    junk_filter: bool,
    hidden_filter: bool,
) -> Result<Collected, String> {
    let mut out = Collected {
        entries: Vec::new(),
        skipped_junk: 0,
        file_count: 0,
        total_bytes: 0,
    };
    let single_file = inputs.len() == 1 && inputs[0].is_file();
    for input in inputs {
        let top_name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".into());
        let prefix = if single_file {
            String::new()
        } else {
            format!("{top_name}/")
        };
        for e in WalkDir::new(input).follow_links(false) {
            let e = e.map_err(|err| err.to_string())?;
            let rel_disk = e.path().strip_prefix(input).unwrap_or(e.path());
            let rel = if rel_disk.as_os_str().is_empty() {
                // 单文件输入无前缀：根条目名即文件名（置空会写出无名条目）
                if single_file {
                    top_name.clone()
                } else {
                    prefix.trim_end_matches('/').to_string()
                }
            } else {
                format!("{prefix}{}", rel_disk.to_string_lossy())
            };
            let filtered = if junk_filter && super::junk::is_junk(&rel) {
                true
            } else {
                hidden_filter && super::junk::is_hidden(&rel)
            };
            if filtered {
                // 目录级垃圾/隐藏整树跳过（WalkDir 不支持剪枝中辍，按条目剔即可）
                if e.file_type().is_dir() {
                    continue;
                }
                out.skipped_junk += 1;
                continue;
            }
            if e.file_type().is_dir() {
                if !rel.is_empty() {
                    out.entries
                        .push((format!("{rel}/"), e.path().to_path_buf(), true));
                }
            } else {
                out.file_count += 1;
                out.total_bytes += std::fs::symlink_metadata(e.path())
                    .map(|m| m.len())
                    .unwrap_or(0);
                out.entries.push((rel, e.path().to_path_buf(), false));
            }
        }
    }
    Ok(out)
}

pub(crate) struct ProgressCtx<'a> {
    /// 已处理字节数（进度分母为输入总字节）
    pub(crate) done: u64,
    pub(crate) total: u64,
    pub(crate) last_percent: f64,
    pub(crate) progress: &'a dyn Fn(f64),
}

impl ProgressCtx<'_> {
    pub(crate) fn step_bytes(&mut self, n: u64) {
        self.done = self.done.saturating_add(n);
        let percent = (self.done as f64 / self.total as f64 * 100.0).min(100.0);
        if percent - self.last_percent >= 0.5 {
            self.last_percent = percent;
            (self.progress)(percent);
        }
    }
}

/// 读侧字节计数器：包住源文件随 io 读取流式推进进度（zip / 7z 写路径共用）。
pub(crate) struct CountingReader<'c, 'a, R: std::io::Read> {
    inner: R,
    ctx: &'c mut ProgressCtx<'a>,
}

impl<'c, 'a, R: std::io::Read> CountingReader<'c, 'a, R> {
    pub(crate) fn new(inner: R, ctx: &'c mut ProgressCtx<'a>) -> CountingReader<'c, 'a, R> {
        CountingReader { inner, ctx }
    }
}

impl<R: std::io::Read> std::io::Read for CountingReader<'_, '_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.ctx.step_bytes(n as u64);
        Ok(n)
    }
}

fn write_zip(
    output: &Path,
    c: &Collected,
    level: u8,
    password: Option<&str>,
    is_cancelled: &dyn Fn() -> bool,
    ctx: &mut ProgressCtx,
) -> Result<(), String> {
    let file = File::create(output).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let level = level.clamp(1, 9);
    for (rel, disk, is_dir) in &c.entries {
        if is_cancelled() {
            return Err("已取消".into());
        }
        let meta = std::fs::symlink_metadata(disk).ok();
        let is_symlink = meta.as_ref().is_some_and(|m| m.file_type().is_symlink());
        let mode = meta
            .as_ref()
            .map(|m| std::os::unix::fs::MetadataExt::mode(m) & 0o777)
            .unwrap_or(if *is_dir { 0o755 } else { 0o644 });
        // >4GiB 条目必须显式开 zip64，否则 zip crate 在收尾报 "Large file option has not been set"
        let large_file = meta.as_ref().is_some_and(|m| m.len() > u32::MAX as u64);
        let mut options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(level as i64))
            .unix_permissions(mode)
            .large_file(large_file);
        // 密码仅对文件条目启用（AES-256）：目录条目加密对 Finder / bsdtar 兼容差
        if let (false, Some(pwd)) = (*is_dir, password) {
            options = options.with_aes_encryption(zip::AesMode::Aes256, pwd);
        }
        if *is_dir {
            zip.add_directory(rel.clone(), options)
                .map_err(|e| e.to_string())?;
        } else if is_symlink {
            // 链接按链接条目存（不跟随）：指向目录的链接当文件打开会 EISDIR 打断整次压缩，
            // 指向文件的链接会被复制目标内容并丢失链接语义
            let target = std::fs::read_link(disk).map_err(|e| e.to_string())?;
            zip.add_symlink_from_path(rel.clone(), target, options)
                .map_err(|e| e.to_string())?;
        } else {
            let src = File::open(disk).map_err(|e| e.to_string())?;
            zip.start_file(rel.clone(), options)
                .map_err(|e| e.to_string())?;
            let mut src = CountingReader::new(src, ctx);
            std::io::copy(&mut src, &mut zip).map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// 写 7z（sevenz-rust：LZMA2 + 可选 AES-256 加密链，镜像其 `compress_encypted` 范式）。
/// 7z 无符号链接条目语义——链接按目标内容直存（from_path/File::open 均跟随链接）。
pub(crate) fn write_sevenz(
    output: &Path,
    c: &Collected,
    level: u8,
    password: Option<&str>,
    is_cancelled: &dyn Fn() -> bool,
    ctx: &mut ProgressCtx,
) -> Result<(), String> {
    let mut w = sevenz_rust::SevenZWriter::create(output).map_err(|e| e.to_string())?;
    // 加密档头不加密：条目名无密码可见（与 zip 语义一致，inspect 可标记 encrypted 提示输密码）
    w.set_encrypt_header(false);
    // 方法链顺序：密码在前（AES 包裹）、压缩在后；等级映射 LZMA2 preset（1-9）
    let mut methods: Vec<sevenz_rust::SevenZMethodConfiguration> = Vec::new();
    if let Some(pwd) = password {
        if !pwd.is_empty() {
            methods
                .push(sevenz_rust::AesEncoderOptions::new(sevenz_rust::Password::from(pwd)).into());
        }
    }
    methods.push(
        sevenz_rust::SevenZMethodConfiguration::new(sevenz_rust::SevenZMethod::LZMA2).with_options(
            sevenz_rust::MethodOptions::LZMA2(sevenz_rust::lzma::LZMA2Options::with_preset(
                level.clamp(1, 9) as u32,
            )),
        ),
    );
    w.set_content_methods(methods);
    for (rel, disk, is_dir) in &c.entries {
        if is_cancelled() {
            return Err("已取消".into());
        }
        let entry = sevenz_rust::SevenZArchiveEntry::from_path(disk, rel.clone());
        if *is_dir || !entry.has_stream {
            w.push_archive_entry(entry, None::<&std::fs::File>)
                .map_err(|e| e.to_string())?;
        } else {
            let src = File::open(disk).map_err(|e| e.to_string())?;
            let src = CountingReader::new(src, ctx);
            w.push_archive_entry(entry, Some(src))
                .map_err(|e| e.to_string())?;
        }
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::archive::extract::{extract_zip, ExtractRequest, ExtractResult};
    use crate::extensions::archive::format::ArchiveFormat;
    use crate::extensions::archive::inspect;

    fn no_cancel() -> bool {
        false
    }

    fn make_tree(root: &Path) {
        std::fs::create_dir_all(root.join("proj/sub")).unwrap();
        std::fs::write(root.join("proj/a.txt"), b"alpha").unwrap();
        std::fs::write(root.join("proj/sub/b.txt"), b"beta").unwrap();
        // 垃圾文件：应被过滤
        std::fs::write(root.join("proj/.DS_Store"), b"junk").unwrap();
        std::fs::write(root.join("proj/sub/._b.txt"), b"junk").unwrap();
        std::fs::create_dir_all(root.join("proj/__MACOSX")).unwrap();
        std::fs::write(root.join("proj/__MACOSX/x"), b"junk").unwrap();
    }

    fn req(archive: &str, dest: &str) -> ExtractRequest {
        ExtractRequest {
            archive_path: archive.into(),
            dest_dir: dest.into(),
            selected: None,
            password: None,
        }
    }

    #[test]
    fn zip_roundtrip_with_junk_filter() {
        let tmp = std::env::temp_dir().join(format!("voidnix_cmp_zip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        make_tree(&tmp);
        let collected = collect(&[tmp.join("proj")], true, false).unwrap();
        assert_eq!(collected.file_count, 2);
        assert_eq!(
            collected.skipped_junk, 3,
            ".DS_Store + ._b.txt + __MACOSX/x"
        );

        let out = tmp.join("proj.zip");
        let progress = |_: f64| {};
        let mut ctx = ProgressCtx {
            done: 0,
            total: 2,
            last_percent: -2.0,
            progress: &progress,
        };
        write_zip(&out, &collected, 6, None, &no_cancel, &mut ctx).unwrap();

        let dest = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let mut result = ExtractResult::default_counts();
        extract_zip(
            &out,
            &dest,
            &req(&out.to_string_lossy(), &dest.to_string_lossy()),
            &no_cancel,
            &progress,
            &mut result,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("proj/a.txt")).unwrap(),
            "alpha"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("proj/sub/b.txt")).unwrap(),
            "beta"
        );
        assert!(!dest.join("proj/.DS_Store").exists());
        assert!(!dest.join("proj/__MACOSX").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 单文件压缩：根条目必须是文件名（曾因 prefix 置空写出无名条目）。
    #[test]
    fn single_file_keeps_original_name() {
        let tmp = std::env::temp_dir().join(format!("voidnix_cmp_single_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("solo.txt");
        std::fs::write(&file, b"solo").unwrap();
        let collected = collect(&[file], true, false).unwrap();
        assert_eq!(collected.file_count, 1);
        assert_eq!(collected.entries.len(), 1);
        assert_eq!(collected.entries[0].0, "solo.txt");

        let progress = |_: f64| {};
        let out = tmp.join("solo.zip");
        let mut ctx = ProgressCtx {
            done: 0,
            total: 1,
            last_percent: -2.0,
            progress: &progress,
        };
        write_zip(&out, &collected, 6, None, &no_cancel, &mut ctx).unwrap();
        let info = inspect::inspect(&out, ArchiveFormat::Zip).unwrap();
        assert_eq!(info.entries.len(), 1);
        assert_eq!(info.entries[0].path, "solo.txt");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 符号链接按链接条目存（不跟随）：指向目录的链接不能当文件打开。
    #[cfg(unix)]
    #[test]
    fn zip_preserves_symlink() {
        let tmp = std::env::temp_dir().join(format!("voidnix_cmp_link_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("proj/target_dir")).unwrap();
        std::fs::write(tmp.join("proj/target_dir/inner.txt"), b"inner").unwrap();
        std::fs::write(tmp.join("proj/plain.txt"), b"plain").unwrap();
        std::os::unix::fs::symlink("target_dir", tmp.join("proj/link_dir")).unwrap();

        let collected = collect(&[tmp.join("proj")], true, false).unwrap();
        let out = tmp.join("proj.zip");
        let progress = |_: f64| {};
        let mut ctx = ProgressCtx {
            done: 0,
            total: collected.file_count,
            last_percent: -2.0,
            progress: &progress,
        };
        write_zip(&out, &collected, 6, None, &no_cancel, &mut ctx).unwrap();

        let mut zip = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let link = zip.by_name("proj/link_dir").unwrap();
        assert!(link.is_symlink(), "符号链接应保留为链接条目");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod encrypt_tests {
    use super::*;
    use crate::extensions::archive::extract::{extract_zip, ExtractRequest, ExtractResult};
    use crate::extensions::archive::format::ArchiveFormat;
    use crate::extensions::archive::inspect;

    fn no_cancel() -> bool {
        false
    }

    /// AES-256 加密往返：写入带密码 → inspect 报加密 → 无密码/错密码拒解 → 正确密码可解 + 预览可解。
    #[test]
    fn zip_aes_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("voidnix_aes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("proj")).unwrap();
        std::fs::write(tmp.join("proj/secret.txt"), b"classified").unwrap();
        let collected = collect(&[tmp.join("proj")], true, false).unwrap();
        let out = tmp.join("enc.zip");
        let progress = |_: f64| {};
        let mut ctx = ProgressCtx {
            done: 0,
            total: 1,
            last_percent: -2.0,
            progress: &progress,
        };
        write_zip(&out, &collected, 6, Some("pw123"), &no_cancel, &mut ctx).unwrap();

        let info = inspect::inspect(&out, ArchiveFormat::Zip).unwrap();
        assert!(info.encrypted, "inspect 应标记加密");

        let dest = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let req = |pwd: Option<&str>| ExtractRequest {
            archive_path: out.to_string_lossy().into_owned(),
            dest_dir: dest.to_string_lossy().into_owned(),
            selected: None,
            password: pwd.map(str::to_string),
        };

        let try_extract = |pwd: Option<&str>| -> Result<(), String> {
            let mut r = ExtractResult::default_counts();
            extract_zip(&out, &dest, &req(pwd), &no_cancel, &progress, &mut r)
        };
        let err = try_extract(None).expect_err("无密码应被拒绝");
        assert!(err.contains("密码"), "实际: {err}");
        let err2 = try_extract(Some("wrong")).expect_err("错密码应被拒绝");
        assert!(err2.contains("密码错误"), "实际: {err2}");
        try_extract(Some("pw123")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("proj/secret.txt")).unwrap(),
            "classified"
        );

        let (bytes, _) =
            inspect::read_entry(&out, ArchiveFormat::Zip, "proj/secret.txt", Some("pw123"))
                .unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "classified");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
