//! run_command 工具：命令直接放行 + 断路器兜底 + 资源约束。
//!
//! 命令无白名单/黑名单拦截——所有命令直接执行，仅以下机制兜底：
//! - shell 元字符注入免疫：tokio::process::Command 不经 shell
//! - 断路器：rm -rf / 等灾难性全局操作拦截（不可放宽）
//! - 环境变量隔离：env_clear() + 白名单 env（防父进程 API key 进子进程）
//! - 文件系统：cwd canonicalize
//! - 资源耗尽：rlimit CPU/DATA/NOFILE（上限由 config 注入）
//! - 超时无 reap：tokio::time::timeout + kill_on_drop + 显式 reap
//! - 输出刷屏：边读边截断
//! - secret 泄露：调用方在 loop_runner 中 scrub_secret（本模块仅返回原始输出）

use async_trait::async_trait;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::extensions::agent::engine::tool_registry::{AgentTool, ToolResult};
use crate::extensions::agent::policy::ExecPolicy;

pub struct RunCommandTool {
    /// 解析后的执行策略（clamp 后的资源上限）。
    policy: ExecPolicy,
}

impl RunCommandTool {
    pub fn new(policy: ExecPolicy) -> Self {
        Self { policy }
    }

    /// 断路器：必拦的灾难性命令模式（不可 config 化、不可放宽）。
    ///
    /// H1：改用 POSIX getopt 风格结构化解析，覆盖旧字符串拼接法的所有绕过：
    /// 拆分选项（`-r -f` / `-rf` / `-irf` / `--recursive` / `--force`）+
    /// `--` 分隔符识别 + 每个 positional 规范化（`~` 展开、尾部 `/` 剥离）后
    /// 判定是否为根 / 家目录 / 通配根。
    fn is_circuit_breaker_hit(cmd: &str, args: &[String]) -> bool {
        // H2：程序名大小写不敏感
        if !cmd.eq_ignore_ascii_case("rm") {
            return false;
        }
        let mut recursive = false;
        let mut force = false;
        let mut positionals: Vec<&str> = Vec::new();
        let mut after_dd = false; // 遇到 `--` 后，余下都是 positional
        for arg in args {
            if after_dd {
                positionals.push(arg);
                continue;
            }
            if arg == "--" {
                after_dd = true;
                continue;
            }
            if let Some(long) = arg.strip_prefix("--") {
                // 长选项：`--recursive` / `--force` / `--no-preserve-root` 等
                // 处理 `--opt=value` 形式
                let name = long.split('=').next().unwrap_or(long);
                if name == "recursive" {
                    recursive = true;
                } else if name == "force" {
                    force = true;
                }
                continue;
            }
            if let Some(cluster) = arg.strip_prefix('-') {
                // 短选项簇：`-rf` → ['r', 'f']；`-irf` → ['i', 'r', 'f']
                for ch in cluster.chars() {
                    match ch {
                        'r' | 'R' => recursive = true,
                        'f' => force = true,
                        _ => {}
                    }
                }
                continue;
            }
            // 非选项：positional operand
            positionals.push(arg);
        }
        if !recursive || !force {
            return false;
        }
        // 任意 positional 是根 / 家目录 / 通配根 → 拦
        let home = std::env::var("HOME").unwrap_or_default();
        for p in &positionals {
            let s = p.trim();
            // 展开 `~` → HOME 后判定（HOME 已是绝对路径，is_destructive_target 能匹配）
            let expanded: String;
            let check: &str = if s.starts_with('~') && (s.len() == 1 || s.as_bytes()[1] == b'/') {
                expanded = if s.len() == 1 {
                    home.clone()
                } else {
                    format!("{}{}", home, &s[1..])
                };
                &expanded
            } else {
                s
            };
            if Self::is_destructive_target(check, &home) {
                return true;
            }
        }
        false
    }

    /// 判定单个目标是否为「不可删除的根路径」（已规范化）。
    fn is_destructive_target(p: &str, home: &str) -> bool {
        let trimmed = p.trim_end_matches('/');
        if trimmed.is_empty() {
            // 原串全是 `/`（如 `/`、`//`）
            return p.starts_with('/');
        }
        match trimmed {
            "/" | "/*" => return true,
            _ => {}
        }
        if !home.is_empty() {
            // 家目录本身（`~` / `/Users/foo`），不含子目录
            if trimmed == home {
                return true;
            }
            // `~/*` 类通配家目录
            if trimmed == format!("{}/*", home) {
                return true;
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
                "description": "Run a shell command on the user's macOS system. Runs in the user's home directory with minimal environment (no inherited API keys). Execution time and output size are limited. Pass ONLY the program name in 'cmd' (NO arguments or spaces), put all flags/arguments in the 'args' array. Not passed through a shell. Example: cmd='git', args=['status'].",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "cmd": {
                            "type": "string",
                            "description": "Program name ONLY, no arguments/spaces (e.g. 'ls', 'git', 'grep', 'rg')"
                        },
                        "args": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Arguments and flags, e.g. [\"status\"], [\"-la\", \"/tmp\"], [\"-r\", \"foo\"]"
                        }
                    },
                    "required": ["cmd"]
                }
            }
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let raw_cmd = args
            .get("cmd")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if raw_cmd.is_empty() {
            return ToolResult::err("cmd is required");
        }

        let input_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // 宽容拆分：cmd 含空格（如 "git status"）且未单独传 args 时，拆为 program + args
        let (cmd, cmd_args) = split_compound_cmd(&raw_cmd, &input_args);

        // 程序名取 basename
        let program = basename(&cmd);
        if program.is_empty() {
            return ToolResult::err("cmd basename is empty");
        }

        // 断路器：rm -rf / 等灾难性全局操作拦截
        if Self::is_circuit_breaker_hit(program, &cmd_args) {
            return ToolResult::err("blocked by circuit breaker (destructive global operation)");
        }

        // 工作目录 canonicalize
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let cwd: PathBuf = if home.is_empty() {
            PathBuf::from("/tmp")
        } else {
            PathBuf::from(&home)
        };
        let real_cwd = match cwd.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("canonicalize cwd failed: {e}")),
        };

        // 构造 Command（env_clear + kill_on_drop + rlimit）
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

        // rlimit（仅 Unix）：在 fork 后、exec 前设置（数值已 clamp）
        let cpu = self.policy.max_cpu_secs;
        let mem_mb = self.policy.max_memory_mb;
        let nofile = self.policy.max_open_files;
        unsafe {
            command.pre_exec(move || apply_rlimits(cpu, mem_mb, nofile));
        }

        // ── spawn ──
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return ToolResult::err(format!("command not found: {}", program));
                }
                return ToolResult::err(format!("spawn failed: {e}"));
            }
        };

        // 超时 + 截断读 + kill + reap
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(self.policy.execution_timeout_secs),
            read_with_cap(&mut child, self.policy.max_output_bytes),
        )
        .await;

        match timeout_result {
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                ToolResult::err(format!(
                    "command timed out after {}s and was killed",
                    self.policy.execution_timeout_secs
                ))
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                ToolResult::err(format!("read failed: {e}"))
            }
            Ok(Ok(ReadOutcome {
                stdout,
                stderr,
                truncated,
                exit_code,
            })) => {
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
                    output.push_str(&format!(
                        "\n[output truncated at {} bytes]",
                        self.policy.max_output_bytes
                    ));
                }
                if exit_code != 0 {
                    output.push_str(&format!("\n[exit code: {}]", exit_code));
                    // 非 0 仍把完整输出回灌 LLM；ok=false 仅驱动前端「失败」态
                    return ToolResult::err(output);
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

/// 宽容拆分：当 cmd 含空格（如 "git status"）且未单独传 args 时，拆为 program + args。
/// LLM 常误把整条命令塞进 cmd；拆分后仍不经 shell，仅 program + args 传递。
/// 已传 args 时尊重显式参数，不拆分。
fn split_compound_cmd(cmd: &str, args: &[String]) -> (String, Vec<String>) {
    if !args.is_empty() {
        return (cmd.to_string(), args.to_vec());
    }
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    match parts.len() {
        0 => (String::new(), Vec::new()),
        1 => (parts[0].to_string(), Vec::new()),
        _ => (
            parts[0].to_string(),
            parts[1..].iter().map(|s| s.to_string()).collect(),
        ),
    }
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
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("stdout pipe missing"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("stderr pipe missing"))?;

    // 并发读 stdout + stderr，避免管道阻塞；各自带 cap
    let cap_clone = cap;
    let stdout_task = tokio::spawn(async move { read_one_stream(&mut stdout, cap_clone).await });
    let stderr_task = tokio::spawn(async move { read_one_stream(&mut stderr, cap_clone).await });

    let (stdout_result, stderr_result) = tokio::join!(stdout_task, stderr_task);
    let (stdout_bytes, stdout_truncated) =
        stdout_result.map_err(|e| std::io::Error::other(format!("panic: {e}")))??;
    let (stderr_bytes, stderr_truncated) =
        stderr_result.map_err(|e| std::io::Error::other(format!("panic: {e}")))??;

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
        (
            "PATH",
            "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin".into(),
        ),
        ("HOME", std::env::var("HOME").unwrap_or_default()),
        ("USER", std::env::var("USER").unwrap_or_default()),
        ("LANG", "en_US.UTF-8".into()),
        ("LC_ALL", "en_US.UTF-8".into()),
        (
            "TMPDIR",
            std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into()),
        ),
        // 故意不设 SHELL / *_API_KEY / DYLD_* / GIT_*
    ]
}

unsafe fn apply_rlimits(cpu: u64, mem_mb: u64, nofile: u64) -> std::io::Result<()> {
    use rlimit::{setrlimit, Resource};
    let cpu_hard = cpu.saturating_mul(2);
    setrlimit(Resource::CPU, cpu, cpu_hard).map_err(std::io::Error::other)?;
    // macOS 不支持 AS，用 DATA 限制（数据段）
    #[cfg(target_os = "macos")]
    {
        let data_soft = mem_mb * 1024 * 1024;
        let data_hard = data_soft.saturating_mul(2);
        if setrlimit(Resource::DATA, data_soft, data_hard).is_err() {
            // macOS DATA 也可能失败，忽略（不致命）
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let as_soft = mem_mb * 1024 * 1024;
        let as_hard = as_soft.saturating_mul(2);
        setrlimit(Resource::AS, as_soft, as_hard).map_err(std::io::Error::other)?;
    }
    let nofile_hard = nofile.saturating_mul(4);
    setrlimit(Resource::NOFILE, nofile, nofile_hard).map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> RunCommandTool {
        RunCommandTool::new(crate::extensions::agent::policy::ExecPolicy::default())
    }

    #[test]
    fn basename_strips_path() {
        assert_eq!(basename("/usr/bin/ls"), "ls");
        assert_eq!(basename("git"), "git");
        assert_eq!(basename("/opt/homebrew/bin/rg"), "rg");
    }

    #[test]
    fn split_compound_cmd_splits_when_args_empty() {
        let (prog, args) = split_compound_cmd("git status", &[]);
        assert_eq!(prog, "git");
        assert_eq!(args, vec!["status".to_string()]);

        let (prog, args) = split_compound_cmd("echo hello world", &[]);
        assert_eq!(prog, "echo");
        assert_eq!(args, vec!["hello".to_string(), "world".to_string()]);

        // 多空格容错
        let (prog, args) = split_compound_cmd("  ls   -la  ", &[]);
        assert_eq!(prog, "ls");
        assert_eq!(args, vec!["-la".to_string()]);
    }

    #[test]
    fn split_compound_cmd_respects_explicit_args() {
        // 已传 args 时不拆分（尊重显式参数）
        let (prog, args) = split_compound_cmd("ls", &["-la".to_string()]);
        assert_eq!(prog, "ls");
        assert_eq!(args, vec!["-la".to_string()]);
    }

    #[test]
    fn split_compound_cmd_single_word_unchanged() {
        let (prog, args) = split_compound_cmd("ls", &[]);
        assert_eq!(prog, "ls");
        assert!(args.is_empty());
    }

    /// 断路器表驱动：覆盖 H1 结构化解析 + H2 大小写 + 放行边界。
    /// args 用 `&str` 切片再转 Vec，便于一眼扫全表。
    #[test]
    fn circuit_breaker_table() {
        // (cmd, args, expect_blocked)
        let cases: &[(&str, &[&str], bool)] = &[
            // 根 / 家目录 / 通配
            ("rm", &["-rf", "/"], true),
            ("rm", &["-rf", "//"], true),
            ("rm", &["-rf", "/*"], true),
            ("rm", &["-rf", "~"], true),
            ("rm", &["-rf", "~/*"], true),
            // 大小写程序名
            ("RM", &["-rf", "/"], true),
            ("Rm", &["-rf", "~"], true),
            // 拆分短选项 / 顺序无关
            ("rm", &["-r", "-f", "/"], true),
            ("rm", &["-f", "-r", "/"], true),
            ("rm", &["-R", "-f", "/"], true),
            // 长选项
            ("rm", &["--recursive", "--force", "/"], true),
            ("rm", &["--force", "--recursive", "~"], true),
            ("rm", &["--recursive=true", "--force", "/"], true),
            // 选项簇带额外字母
            ("rm", &["-irf", "/"], true),
            ("rm", &["-fri", "/"], true),
            // `--` 后 positional 仍拦
            ("rm", &["-rf", "--", "/"], true),
            ("rm", &["-rf", "--", "~"], true),
            // ── 放行 ──
            ("rm", &["-rf", "/tmp/test"], false),
            ("rm", &["-rf", "~/Documents"], false),
            ("rm", &["-r", "/"], false),  // 缺 force
            ("rm", &["-f", "/"], false),  // 缺 recursive
            ("rm", &["-rf"], false),      // 无目标
            ("ls", &["-rf", "/"], false), // 非 rm
            ("echo", &["-rf", "/"], false),
        ];
        for (cmd, args, blocked) in cases {
            let args_owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            let hit = RunCommandTool::is_circuit_breaker_hit(cmd, &args_owned);
            assert_eq!(
                hit, *blocked,
                "cmd={cmd:?} args={args:?}: expected blocked={blocked}, got {hit}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_empty_cmd() {
        let t = tool();
        let result = t.call(serde_json::json!({"cmd": ""})).await;
        assert!(!result.ok);
        assert!(result.output.contains("required"));
    }

    #[tokio::test]
    async fn rejects_circuit_breaker_in_call() {
        let t = tool();
        let result = t
            .call(serde_json::json!({"cmd": "rm", "args": ["-rf", "/"]}))
            .await;
        assert!(!result.ok);
        assert!(result.output.contains("circuit breaker"));
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
        let result = t
            .call(serde_json::json!({"cmd": "echo", "args": ["hello"]}))
            .await;
        assert!(result.ok, "got: {}", result.output);
        assert!(result.output.contains("hello"));
    }

    #[tokio::test]
    async fn env_clear_blocks_inherited_keys() {
        // 设置一个测试 env，验证子进程不继承
        std::env::set_var("VOIDNIX_TEST_SECRET", "leak-me-if-you-can");
        let t = tool();
        let result = t.call(serde_json::json!({"cmd": "env", "args": []})).await;
        // env 命令直接执行（无白名单拦截），关注输出中不应含 VOIDNIX_TEST_SECRET
        if result.ok {
            assert!(
                !result.output.contains("VOIDNIX_TEST_SECRET"),
                "secret leaked!"
            );
            assert!(!result.output.contains("leak-me-if-you-can"));
        }
    }
}
