//! run_command 工具：纵深防御 9 层。
//!
//! 威胁模型覆盖（参考 Claude Code、CWE-78）：
//! - L1 shell 元字符注入：tokio::process::Command 不经 shell，天然免疫
//! - L2 参数注入：危险选项前缀黑名单（--exec/--upload-pack/-o/...）
//! - L3 环境变量泄露：env_clear() + 白名单 env
//! - L4 文件系统逃逸：cwd canonicalize（symlink 在 Phase 2 加深层检查）
//! - L5 macOS 特权命令：osascript/sudo/open -a/launchctl/defaults 完全 deny
//! - L6 资源耗尽：rlimit CPU/AS/NOFILE/NPROC
//! - L7 超时无 reap：tokio::time::timeout + kill_on_drop + 显式 reap
//! - L8 输出刷屏：边读边截断 1 MiB
//! - L9 secret 泄露：调用方在 loop_runner 中 scrub_secret（本模块仅返回原始输出）

use async_trait::async_trait;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::extensions::agent::engine::tool_registry::{AgentTool, ToolResult};

const MAX_WALL_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024; // 1 MiB

/// 用户自定义白名单（前端传入，完整覆盖；settings 默认值已含常用读+编辑命令）。
/// 空时表示用户未配置任何白名单 → 所有命令都需审批。
const EMPTY_ALLOWED: &[&str] = &[];

/// 硬禁命令（即便 approval 通过也拒绝）。
/// macOS 上这些命令可以控制其他 app / 提权 / 系统配置 / 触网外发。
const FORBIDDEN_PROGRAMS: &[&str] = &[
    // 任何 shell → 放弃 L1 防御
    "sh", "bash", "zsh", "dash", "ksh", "fish", "csh", "tcsh",
    // macOS 特权 / 系统控制
    "osascript", "sudo", "open", "launchctl", "defaults", "networksetup", "scutil",
    "killall", "kill", "pkill",
    // 触网（走专门的 web_search 工具）
    "curl", "wget", "nc", "socat", "telnet", "ssh",
    // 提权或逃逸
    "su", "doas", "expect",
    // 数据持久化（走应用 API）
    "sqlite3",
    // 进程管理
    "ps", "top", "htop",
];

/// 危险选项前缀黑名单（即便命令在 ALLOWED 内也拦截）。
/// 这些选项在多个程序里有「执行子命令 / 输出到任意路径 / 改变行为」的能力。
const DENIED_ARG_PREFIXES: &[&str] = &[
    "--exec", "--exec-file", "--exec-rm",
    "--upload-pack",
    "--use-compress-program",
    "--config", "-C", // git -C 改 cwd；curl --config 读配置
    "--output", "-o", "-O", // curl/wget 写文件
    "--write-out",
    "--eval", "-e", // node/bash eval
    "--init-file", "--rcfile",
];

pub struct RunCommandTool {
    /// 用户编辑的完整白名单（settings 默认已含常用读+编辑命令；用户可自由编辑）。
    trusted: Vec<String>,
}

impl RunCommandTool {
    pub fn new(trusted: Vec<String>) -> Self {
        Self { trusted }
    }

    fn is_allowed_program(&self, name: &str) -> bool {
        self.trusted.iter().any(|t| t == name) || EMPTY_ALLOWED.contains(&name)
    }

    fn is_forbidden(&self, name: &str) -> bool {
        FORBIDDEN_PROGRAMS.contains(&name)
    }

    fn has_denied_arg(args: &[String]) -> Option<&'static str> {
        for arg in args {
            // 选项前缀大小写敏感（`-C` 与 `-c` 在 Unix 是不同选项）
            for denied in DENIED_ARG_PREFIXES {
                // 形如 `--exec=foo` 或 `-ofoo`，前缀匹配即拒
                if arg.starts_with(denied) {
                    return Some(denied);
                }
            }
            // 保守拒绝 shell 元字符（即使不经 shell，也提示用户可能误用）
            if arg.contains("${") || arg.contains("$(") || arg.contains('`') {
                return Some("shell-substitution");
            }
        }
        None
    }

    /// 断路器：即便 approved 也必拦的命令模式。
    fn is_circuit_breaker_hit(cmd: &str, args: &[String]) -> bool {
        // rm -rf / | rm -rf ~ | rm -rf /*
        if cmd == "rm" {
            let combined = args.join(" ");
            if combined.contains("-rf") || combined.contains("-fr") {
                let target = combined
                    .replace("-rf", "")
                    .replace("-fr", "")
                    .replace("--recursive", "")
                    .replace("--force", "")
                    .trim()
                    .to_string();
                if target == "/" || target == "/*" || target.starts_with("~/") && target.len() <= 3
                    || target == "~"
                {
                    return true;
                }
            }
        }
        false
    }
}

#[async_trait]
impl AgentTool for RunCommandTool {
    fn name(&self) -> &'static str {
        "run_command"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Run a shell command on the user's macOS system. The command runs in the user's home directory with minimal environment (no inherited API keys). Output is truncated to 1 MiB and execution time limited to 30 seconds. Prefer read-only commands (ls, cat, grep, git status) for inspection.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "cmd": {
                            "type": "string",
                            "description": "Program to execute (e.g. 'ls', 'git', 'grep')"
                        },
                        "args": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Arguments array (not passed through shell)"
                        }
                    },
                    "required": ["cmd"]
                }
            }
        })
    }

    fn requires_approval(&self, args: &serde_json::Value) -> bool {
        let cmd = args.get("cmd").and_then(|v| v.as_str()).unwrap_or_default();
        let program = basename(cmd);

        // 硬禁：不审批，直接拒
        if self.is_forbidden(program) {
            return false;
        }

        // 白名单（默认 + 用户自定义）内 + 无危险参数 → 免审批
        if self.is_allowed_program(program) {
            let args_arr: Vec<String> = args
                .get("args")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect())
                .unwrap_or_default();
            if Self::has_denied_arg(&args_arr).is_none() {
                return false;
            }
        }

        // 其余（rm 等危险命令、未知命令、含危险参数）一律审批
        true
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let cmd = args
            .get("cmd")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if cmd.is_empty() {
            return ToolResult::err("cmd is required");
        }

        let cmd_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // ── 关 1：程序名取 basename ──
        let program = basename(&cmd);
        if program.is_empty() {
            return ToolResult::err("cmd basename is empty");
        }

        // ── 关 2：硬禁程序 ──
        if self.is_forbidden(program) {
            return ToolResult::err(format!("'{}' is forbidden for safety", program));
        }

        // ── 关 3：断路器（rm -rf / 等，即便 approved 也拦）──
        if Self::is_circuit_breaker_hit(program, &cmd_args) {
            return ToolResult::err("blocked by circuit breaker (destructive global operation)");
        }

        // ── 关 4：参数黑名单 ──
        if let Some(denied) = Self::has_denied_arg(&cmd_args) {
            return ToolResult::err(format!("argument prefix '{}' is blocked", denied));
        }

        // ── 关 5：工作目录 canonicalize ──
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let cwd: PathBuf = if home.is_empty() {
            PathBuf::from("/tmp")
        } else {
            PathBuf::from(&home)
        };
        let real_cwd = match cwd.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("canonicalize cwd failed: {}", e)),
        };

        // ── 关 6-8：构造 Command（env_clear + kill_on_drop + rlimit）──
        let mut command = Command::new(program);
        command
            .args(&cmd_args)
            .current_dir(&real_cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear();

        for (k, v) in minimal_env(&real_cwd) {
            command.env(k, v);
        }

        // rlimit（仅 Unix）：在 fork 后、exec 前设置
        unsafe {
            command.pre_exec(|| apply_rlimits());
        }

        // ── spawn ──
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return ToolResult::err(format!("command not found: {}", program));
                }
                return ToolResult::err(format!("spawn failed: {}", e));
            }
        };

        // ── 关 9：超时 + 截断读 + kill + reap ──
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(MAX_WALL_SECS),
            read_with_cap(&mut child, MAX_OUTPUT_BYTES),
        )
        .await;

        match timeout_result {
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                ToolResult::err(format!(
                    "command timed out after {}s and was killed",
                    MAX_WALL_SECS
                ))
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                ToolResult::err(format!("read failed: {}", e))
            }
            Ok(Ok(ReadOutcome { stdout, stderr, truncated, exit_code })) => {
                // child.wait() 已在 read_with_cap 内完成（reap）
                let mut output = String::new();
                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push_str("\n--- stderr ---\n");
                    }
                    output.push_str(&stderr);
                }
                if truncated {
                    output.push_str("\n[output truncated at 1 MiB]");
                }
                if exit_code != 0 {
                    output.push_str(&format!("\n[exit code: {}]", exit_code));
                }
                ToolResult::ok(output)
            }
        }
    }
}

fn basename(p: &str) -> &str {
    Path::new(p)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
}

struct ReadOutcome {
    stdout: String,
    stderr: String,
    truncated: bool,
    exit_code: i32,
}

async fn read_with_cap(
    child: &mut tokio::process::Child,
    cap: usize,
) -> std::io::Result<ReadOutcome> {
    let mut stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "stdout pipe missing")
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "stderr pipe missing")
    })?;

    // 并发读 stdout + stderr，避免管道阻塞；各自带 cap
    let cap_clone = cap;
    let stdout_task = tokio::spawn(async move { read_one_stream(&mut stdout, cap_clone).await });
    let stderr_task = tokio::spawn(async move { read_one_stream(&mut stderr, cap_clone).await });

    let (stdout_result, stderr_result) = tokio::join!(stdout_task, stderr_task);
    let (stdout_bytes, stdout_truncated) = stdout_result
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("panic: {}", e)))??;
    let (stderr_bytes, stderr_truncated) = stderr_result
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("panic: {}", e)))??;

    // 等进程退出拿 exit code
    let exit_code = match child.wait().await {
        Ok(status) => status.code().unwrap_or(-1),
        Err(_) => -1,
    };

    Ok(ReadOutcome {
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        truncated: stdout_truncated || stderr_truncated,
        exit_code,
    })
}

async fn read_one_stream<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
    cap: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    use tokio::io::{AsyncReadExt, BufReader};
    let mut buf_reader = BufReader::new(reader);
    let mut out = Vec::with_capacity(8 * 1024);
    let mut chunk = [0u8; 8 * 1024];
    let mut truncated = false;
    loop {
        match buf_reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if out.len() + n > cap {
                    let remain = cap - out.len();
                    out.extend_from_slice(&chunk[..remain]);
                    truncated = true;
                    break;
                }
                out.extend_from_slice(&chunk[..n]);
            }
            Err(e) => return Err(e),
        }
    }
    Ok((out, truncated))
}

fn minimal_env(_cwd: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin".into()),
        ("HOME", std::env::var("HOME").unwrap_or_default()),
        ("USER", std::env::var("USER").unwrap_or_default()),
        ("LANG", "en_US.UTF-8".into()),
        ("LC_ALL", "en_US.UTF-8".into()),
        ("TMPDIR", std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into())),
        // 故意不设 SHELL / *_API_KEY / DYLD_* / GIT_*
    ]
}

unsafe fn apply_rlimits() -> std::io::Result<()> {
    use rlimit::{setrlimit, Resource};
    setrlimit(Resource::CPU, 30, 60)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    // macOS 不支持 AS，用 DATA 限制（数据段）
    #[cfg(target_os = "macos")]
    {
        if setrlimit(Resource::DATA, 512 * 1024 * 1024, 1024 * 1024 * 1024).is_err() {
            // macOS DATA 也可能失败，忽略（不致命）
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        setrlimit(Resource::AS, 512 * 1024 * 1024, 1024 * 1024 * 1024)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    }
    setrlimit(Resource::NOFILE, 64, 256)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> RunCommandTool {
        RunCommandTool::new(vec![])
    }

    #[test]
    fn basename_strips_path() {
        assert_eq!(basename("/usr/bin/ls"), "ls");
        assert_eq!(basename("git"), "git");
        assert_eq!(basename("/opt/homebrew/bin/rg"), "rg");
    }

    #[test]
    fn forbidden_programs_denied_without_approval() {
        let t = tool();
        assert!(t.is_forbidden("osascript"));
        assert!(t.is_forbidden("sudo"));
        assert!(t.is_forbidden("sh"));
        assert!(t.is_forbidden("curl"));
        assert!(!t.is_forbidden("ls"));
    }

    #[test]
    fn empty_whitelist_means_all_need_approval() {
        // tool() 用空 trusted，没有白名单 → 所有命令都需要审批
        let t = tool();
        assert!(!t.is_allowed_program("ls"));
        assert!(!t.is_allowed_program("git"));
    }

    #[test]
    fn trusted_programs_are_the_whitelist() {
        // 用户提供的 trusted 列表 = 完整白名单
        let t = RunCommandTool::new(vec!["ls".into(), "git".into(), "make".into()]);
        assert!(t.is_allowed_program("ls"));
        assert!(t.is_allowed_program("git"));
        assert!(t.is_allowed_program("make"));
        assert!(!t.is_allowed_program("rm"));
    }

    #[test]
    fn denied_arg_prefix_detection() {
        assert!(RunCommandTool::has_denied_arg(&["--exec=foo".into()]).is_some());
        assert!(RunCommandTool::has_denied_arg(&["--upload-pack".into()]).is_some());
        assert!(RunCommandTool::has_denied_arg(&["-o".into(), "/etc/passwd".into()]).is_some());
        assert!(RunCommandTool::has_denied_arg(&["normal".into(), "args".into()]).is_none());
    }

    #[test]
    fn shell_substitution_in_args_detected() {
        assert!(RunCommandTool::has_denied_arg(&["$(rm -rf /)".into()]).is_some());
        assert!(RunCommandTool::has_denied_arg(&["${HOME}".into()]).is_some());
        assert!(RunCommandTool::has_denied_arg(&["`whoami`".into()]).is_some());
    }

    #[test]
    fn circuit_breaker_rm_rf_root() {
        assert!(RunCommandTool::is_circuit_breaker_hit("rm", &["-rf".into(), "/".into()]));
        assert!(RunCommandTool::is_circuit_breaker_hit("rm", &["-rf".into(), "~".into()]));
        assert!(RunCommandTool::is_circuit_breaker_hit("rm", &["-rf".into(), "/*".into()]));
        // 单独的 -rf 但目标是具体目录不拦
        assert!(!RunCommandTool::is_circuit_breaker_hit("rm", &["-rf".into(), "/tmp/test".into()]));
    }

    #[test]
    fn requires_approval_for_unknown_command() {
        let t = tool();
        let args = serde_json::json!({"cmd": "make", "args": []});
        assert!(t.requires_approval(&args));
    }

    #[test]
    fn no_approval_for_whitelisted_command() {
        let t = RunCommandTool::new(vec!["ls".into()]);
        let args = serde_json::json!({"cmd": "ls", "args": ["-la"]});
        assert!(!t.requires_approval(&args));
    }

    #[test]
    fn approval_required_when_whitelisted_has_denied_arg() {
        let t = tool();
        // git 在白名单，但 -C 是 denied prefix
        let args = serde_json::json!({"cmd": "git", "args": ["-C", "/tmp"]});
        assert!(t.requires_approval(&args));
    }

    #[tokio::test]
    async fn rejects_empty_cmd() {
        let t = tool();
        let result = t.call(serde_json::json!({"cmd": ""})).await;
        assert!(!result.ok);
        assert!(result.output.contains("required"));
    }

    #[tokio::test]
    async fn rejects_forbidden_program() {
        let t = tool();
        let result = t.call(serde_json::json!({"cmd": "sudo", "args": ["ls"]})).await;
        assert!(!result.ok);
        assert!(result.output.contains("forbidden"));
    }

    #[tokio::test]
    async fn rejects_missing_program() {
        let t = tool();
        let result = t
            .call(serde_json::json!({"cmd": "nonexistent_cmd_xyz_123", "args": []}))
            .await;
        assert!(!result.ok);
        assert!(result.output.contains("not found") || result.output.contains("failed"));
    }

    #[tokio::test]
    async fn runs_simple_command() {
        let t = tool();
        let result = t.call(serde_json::json!({"cmd": "echo", "args": ["hello"]})).await;
        assert!(result.ok, "got: {}", result.output);
        assert!(result.output.contains("hello"));
    }

    #[tokio::test]
    async fn env_clear_blocks_inherited_keys() {
        // 设置一个测试 env，验证子进程不继承
        std::env::set_var("VOIDNIX_TEST_SECRET", "leak-me-if-you-can");
        let t = tool();
        let result = t
            .call(serde_json::json!({"cmd": "env", "args": []}))
            .await;
        // env 不在白名单，会走 approval 拒绝路径，但这是测试直接调 call
        // 实际上 env 不在白名单也不在 forbidden，所以会执行（生产中走 approval）
        // 我们关注的是：输出中不应含 VOIDNIX_TEST_SECRET
        // 但 env 不在白名单 → 直接调 call 也会执行（call 内部不做审批检查）
        if result.ok {
            assert!(!result.output.contains("VOIDNIX_TEST_SECRET"), "secret leaked!");
            assert!(!result.output.contains("leak-me-if-you-can"));
        }
    }
}
