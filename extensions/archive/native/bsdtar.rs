//! 系统 bsdtar（/usr/bin/tar，libarchive）进程封装：7z/rar/bz2/xz/zst/裸 tar 的读与解。
//!
//! spawn 纪律：一次性任务一律 `.output()` 收尾（自动回收）；流式读 / 解包显式 `wait()`
//! 收尾，读满上限或取消时先断管 / kill 子进程再回收。不设 `LC_ALL=C`（C locale 会把
//! 非 ASCII 文件名转义为八进制）。条目名一律经 `--` 操作数传递，杜绝以 `-` 开头的
//! 恶意条目名被解析为 tar 选项（选项注入）。

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn base_cmd() -> Command {
    let mut cmd = Command::new("/usr/bin/tar");
    cmd.stdin(Stdio::null());
    cmd
}

/// 一次性运行（列表等），返回 stdout（失败时 stderr 文案上抛）。
pub fn run(args: &[String]) -> Result<String, String> {
    let output = base_cmd()
        .args(args)
        .output()
        .map_err(|e| format!("启动 tar 失败: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if msg.is_empty() {
            "tar 执行失败".into()
        } else {
            msg
        })
    }
}

/// 流式读单条目到 stdout，读到 `limit` 字节即断管返回（`truncated` = 是否触及上限）。
/// 条目名经 `--` 操作数传递（杜绝以 `-` 开头的条目名被解析为 tar 选项）。
pub fn stream_entry(archive: &Path, entry: &str, limit: usize) -> Result<(Vec<u8>, bool), String> {
    let mut child = base_cmd()
        .args(["-xOf", &archive.to_string_lossy(), "--", entry])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("启动 tar 失败: {e}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut buf = Vec::with_capacity(limit.min(256 * 1024));
    let read = (&mut stdout).take(limit as u64).read_to_end(&mut buf);
    // 必须先把读端从父进程释放：读满上限后若仍持有 stdout 直接 wait()，
    // bsdtar 会阻塞在写满的管道上永不退出（大条目预览死锁）。
    drop(stdout);
    let status = child.wait().map_err(|e| format!("等待 tar 失败: {e}"))?;
    read.map_err(|e| format!("读取条目失败: {e}"))?;
    let truncated = buf.len() == limit;
    if buf.is_empty() && !status.success() {
        return Err(format!("无法读取条目: {entry}"));
    }
    Ok((buf, truncated))
}

/// 解包到目标目录（全量或 `--` 操作数选择性），成功返回空；取消即 kill 子进程。
/// 注意：操作数是 libarchive 的 pattern 匹配——条目名含 `*`/`?`/`[` 时可能误匹配其它
/// 条目（极端如单个 `*` 条目名等同全量解包）；实际归档罕见，接受该边缘不另设转义。
pub fn extract(
    archive: &Path,
    dest: &Path,
    names: Option<&[String]>,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let mut cmd = base_cmd();
    let archive_s = archive.to_string_lossy();
    let dest_s = dest.to_string_lossy();
    cmd.args(["-xf", archive_s.as_ref(), "-C", dest_s.as_ref(), "--"]);
    if let Some(names) = names {
        cmd.args(names);
    }
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 tar 失败: {e}"))?;
    // stderr 独立排空，避免错误输出写满管道阻塞子进程
    let mut stderr = child.stderr.take().expect("piped stderr");
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    loop {
        if is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("已取消".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let msg = err_thread.join().unwrap_or_default();
                return if status.success() {
                    Ok(())
                } else {
                    let msg = msg.trim().to_string();
                    Err(if msg.is_empty() {
                        "tar 执行失败".into()
                    } else {
                        msg
                    })
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 用系统 bsdtar 造一个 .tar.xz（含中文与空格文件名），走完整 inspect → stream → extract 链路。
    #[test]
    fn tar_xz_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("voidnix_bsdtar_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src/子 目录")).unwrap();
        std::fs::write(tmp.join("src/子 目录/文件 名.txt"), b"hello tar").unwrap();
        std::fs::write(tmp.join("src/plain.txt"), b"plain").unwrap();
        let archive = tmp.join("a.tar.xz");
        let out = run(&[
            // -J = xz 压缩（bsdtar 无 --format xz 创建档位）
            "-Jcf".to_string(),
            archive.to_string_lossy().into_owned(),
            "-C".to_string(),
            tmp.join("src").to_string_lossy().into_owned(),
            ".".to_string(),
        ])
        .unwrap();
        assert!(out.is_empty());

        let info = crate::extensions::archive::inspect::inspect(
            &archive,
            crate::extensions::archive::format::ArchiveFormat::TarXz,
        )
        .unwrap();
        let names: Vec<&str> = info.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"子 目录/文件 名.txt"), "names: {names:?}");
        assert!(names.contains(&"plain.txt"));

        // 单条目流读（中文 + 空格名）
        let (bytes, truncated) = stream_entry(&archive, "子 目录/文件 名.txt", 1024).unwrap();
        assert!(!truncated);
        assert_eq!(String::from_utf8_lossy(&bytes), "hello tar");

        // 全量解包 + 选择性解包
        let no_cancel = || false;
        let dest: PathBuf = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract(&archive, &dest, None, &no_cancel).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("子 目录/文件 名.txt")).unwrap(),
            "hello tar"
        );
        let dest2 = tmp.join("out2");
        std::fs::create_dir_all(&dest2).unwrap();
        extract(
            &archive,
            &dest2,
            Some(&["plain.txt".to_string()]),
            &no_cancel,
        )
        .unwrap();
        assert!(dest2.join("plain.txt").exists());
        assert!(!dest2.join("子 目录").exists());

        // 取消：进程立即终止，返回明确文案
        let dest3 = tmp.join("out3");
        std::fs::create_dir_all(&dest3).unwrap();
        let err = extract(&archive, &dest3, None, &|| true).unwrap_err();
        assert!(err.contains("已取消"), "实际: {err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ISO9660（libarchive 读写均支持；魔数 CD001 在 0x8001 偏移经 format::detect 二次探测）。
    #[test]
    fn iso_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("voidnix_iso_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::write(tmp.join("src/a.txt"), b"hello iso").unwrap();
        let archive = tmp.join("a.iso");
        run(&[
            "-cf".to_string(),
            archive.to_string_lossy().into_owned(),
            "--format".to_string(),
            "iso9660".to_string(),
            "-C".to_string(),
            tmp.to_string_lossy().into_owned(),
            "src".to_string(),
        ])
        .unwrap();

        assert_eq!(
            crate::extensions::archive::format::detect(&archive),
            Some(crate::extensions::archive::format::ArchiveFormat::Iso)
        );
        let info = crate::extensions::archive::inspect::inspect(
            &archive,
            crate::extensions::archive::format::ArchiveFormat::Iso,
        )
        .unwrap();
        assert!(
            info.entries.iter().any(|e| e.path.ends_with("src/a.txt")),
            "entries: {:?}",
            info.entries
        );

        let dest = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let no_cancel = || false;
        extract(&archive, &dest, None, &no_cancel).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("src/a.txt")).unwrap(),
            "hello iso"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 读满上限后必须先断管再 wait：否则 bsdtar 写满管道阻塞，预览永久卡死。
    #[test]
    fn stream_entry_large_entry_does_not_deadlock() {
        let tmp = std::env::temp_dir().join(format!("voidnix_bsdtar_big_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let big: Vec<u8> = (0..4 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        std::fs::write(tmp.join("big.bin"), &big).unwrap();
        let archive = tmp.join("big.tar.xz");
        run(&[
            "-Jcf".to_string(),
            archive.to_string_lossy().into_owned(),
            "-C".to_string(),
            tmp.to_string_lossy().into_owned(),
            "big.bin".to_string(),
        ])
        .unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let path = archive.clone();
        std::thread::spawn(move || {
            let _ = tx.send(stream_entry(&path, "big.bin", 1024 * 1024));
        });
        let (bytes, truncated) = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("stream_entry 超时：读满上限后未断管导致死锁")
            .unwrap();
        assert_eq!(bytes.len(), 1024 * 1024);
        assert!(truncated);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
