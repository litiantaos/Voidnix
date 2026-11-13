//! mdfind 子进程统一入口：stdout 读取线程 + 超时 kill。
//!
//! mds 异常时 mdfind 可能长时间不退出；搜索/启动链路不能因此挂死。读线程与计时
//! 线程分离，超时分支 kill + wait 回收子进程（不留僵尸）。失败/超时返回 None，
//! 调用方保留旧快照而不是把临时故障当成「全部零使用」。

use std::process::{Command, Stdio};
use std::time::Duration;

pub(super) fn run(mut command: Command, timeout: Duration) -> Option<String> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::io::read_to_string(stdout));
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
            ok.then_some(output)
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    #[test]
    fn captures_stdout_on_success() {
        assert_eq!(
            run(sh("printf hi"), Duration::from_secs(1)).as_deref(),
            Some("hi")
        );
    }

    #[test]
    fn returns_none_on_non_zero_exit() {
        assert_eq!(run(sh("exit 1"), Duration::from_secs(1)), None);
    }

    #[test]
    fn kills_child_on_timeout() {
        assert_eq!(run(sh("exec sleep 5"), Duration::from_millis(50)), None);
    }
}
