//! 归档格式探测：magic bytes 优先，扩展名兜底（空文件/截断包仍可给出候选路由）。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 归档格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    SevenZ,
    Rar,
    Iso,
}

impl ArchiveFormat {
    /// 处理引擎：crate 直读（zip/tar.gz/7z/rar）或系统 bsdtar（其余）。
    pub fn engine(self) -> Engine {
        match self {
            ArchiveFormat::Zip
            | ArchiveFormat::TarGz
            | ArchiveFormat::SevenZ
            | ArchiveFormat::Rar => Engine::Native,
            _ => Engine::Bsdtar,
        }
    }
}

/// 解压引擎路由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// crate 直读（zip / tar / 7z / unrar crate：随机访问/顺序扫描/解密，进度精确）
    Native,
    /// 系统 bsdtar（libarchive：bz2/xz/zst/裸 tar）
    Bsdtar,
}

/// 按内容 magic bytes 探测；不匹配时回退扩展名。
/// ISO 的魔数在 0x8001 偏移（`CD001`），单独经 `sniff_iso` 二次探测。
pub fn detect(path: &Path) -> Option<ArchiveFormat> {
    let mut head = [0u8; 8];
    let n = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, head.as_mut_slice()))
        .map(|_| 8)
        .unwrap_or(0);
    let head = &head[..n];
    if let Some(f) = sniff(head) {
        return Some(f);
    }
    if let Some(f) = sniff_iso(path) {
        return Some(f);
    }
    detect_by_ext(path)
}

/// ISO9660 卷描述符魔数 `CD001` 固定在 32769 偏移（前 32KB 是系统区，无固定签名）。
fn sniff_iso(path: &Path) -> Option<ArchiveFormat> {
    use std::io::{Read, Seek, SeekFrom};
    let mut buf = [0u8; 5];
    std::fs::File::open(path)
        .and_then(|mut f| {
            f.seek(SeekFrom::Start(0x8001))?;
            f.read_exact(&mut buf)
        })
        .ok()?;
    (buf == *b"CD001").then_some(ArchiveFormat::Iso)
}

/// magic bytes 判定（`PK\x03\x04` 等标准头）。
fn sniff(head: &[u8]) -> Option<ArchiveFormat> {
    match head {
        [0x50, 0x4B, 0x03, 0x04, ..] | [0x50, 0x4B, 0x05, 0x06, ..] => Some(ArchiveFormat::Zip),
        [0x1F, 0x8B, ..] => Some(ArchiveFormat::TarGz),
        [0x42, 0x5A, 0x68, ..] => Some(ArchiveFormat::TarBz2),
        [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, ..] => Some(ArchiveFormat::TarXz),
        [0x28, 0xB5, 0x2F, 0xFD, ..] => Some(ArchiveFormat::TarZst),
        [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, ..] => Some(ArchiveFormat::SevenZ),
        // RAR4: `Rar!\x1A\x07\x00`；RAR5: `Rar!\x1A\x07\x01\x00`
        [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00 | 0x01, ..] => Some(ArchiveFormat::Rar),
        _ => None,
    }
}

/// 扩展名兜底（内容头缺失/截断时仍能给出候选路由）。
fn detect_by_ext(path: &Path) -> Option<ArchiveFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "zip" | "jar" | "epub" | "docx" | "xlsx" | "pptx" => Some(ArchiveFormat::Zip),
        "7z" => Some(ArchiveFormat::SevenZ),
        "rar" => Some(ArchiveFormat::Rar),
        "iso" => Some(ArchiveFormat::Iso),
        "tar" => Some(ArchiveFormat::Tar),
        // 单扩展缩写（tgz 等）：与 TS 侧 ARCHIVE_EXTENSIONS 对齐，bsdtar 均支持
        "tgz" => Some(ArchiveFormat::TarGz),
        "tbz2" => Some(ArchiveFormat::TarBz2),
        "txz" => Some(ArchiveFormat::TarXz),
        "tzst" => Some(ArchiveFormat::TarZst),
        "gz" => match path.file_stem()?.to_str()?.rsplit('.').next() {
            Some("tar") => Some(ArchiveFormat::TarGz),
            _ => None,
        },
        "bz2" => match path.file_stem()?.to_str()?.rsplit('.').next() {
            Some("tar") => Some(ArchiveFormat::TarBz2),
            _ => None,
        },
        "xz" => match path.file_stem()?.to_str()?.rsplit('.').next() {
            Some("tar") => Some(ArchiveFormat::TarXz),
            _ => None,
        },
        "zst" => match path.file_stem()?.to_str()?.rsplit('.').next() {
            Some("tar") => Some(ArchiveFormat::TarZst),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_known_magics() {
        assert_eq!(
            sniff(&[0x50, 0x4B, 0x03, 0x04, 0x00]),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(sniff(&[0x1F, 0x8B, 0x08]), Some(ArchiveFormat::TarGz));
        assert_eq!(
            sniff(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            sniff(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]),
            Some(ArchiveFormat::SevenZ)
        );
        // RAR4 / RAR5
        assert_eq!(
            sniff(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00, 0x00]),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            sniff(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00]),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(sniff(b"hello world text"), None);
    }

    #[test]
    fn engine_routing() {
        assert_eq!(ArchiveFormat::Zip.engine(), Engine::Native);
        assert_eq!(ArchiveFormat::TarGz.engine(), Engine::Native);
        assert_eq!(ArchiveFormat::SevenZ.engine(), Engine::Native);
        assert_eq!(ArchiveFormat::Rar.engine(), Engine::Native);
        assert_eq!(ArchiveFormat::TarXz.engine(), Engine::Bsdtar);
    }

    #[test]
    fn ext_fallback() {
        assert_eq!(
            detect_by_ext(Path::new("/a/b/archive.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            detect_by_ext(Path::new("/a/b/x.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            detect_by_ext(Path::new("/a/b/y.7z")),
            Some(ArchiveFormat::SevenZ)
        );
        // 单扩展缩写（与 TS 侧 ARCHIVE_EXTENSIONS 对齐）
        assert_eq!(
            detect_by_ext(Path::new("/a/b/p.tgz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            detect_by_ext(Path::new("/a/b/p.tbz2")),
            Some(ArchiveFormat::TarBz2)
        );
        assert_eq!(
            detect_by_ext(Path::new("/a/b/p.txz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            detect_by_ext(Path::new("/a/b/p.tzst")),
            Some(ArchiveFormat::TarZst)
        );
        assert_eq!(detect_by_ext(Path::new("/a/b/plain.txt")), None);
        // app/logicalname.gz 之类非 tar 的 gz 不认定为归档
        assert_eq!(detect_by_ext(Path::new("/a/b/data.gz")), None);
        assert_eq!(
            detect_by_ext(Path::new("/a/b/ubuntu.iso")),
            Some(ArchiveFormat::Iso)
        );
    }
}
