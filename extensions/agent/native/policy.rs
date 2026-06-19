//! Agent 命令执行安全底线（§3.4 双层安全）。
//!
//! 权威源：floor/cap 定义于此 Rust const，`agent_run` 入口强制 clamp/并集，
//! **不信任任何前端传值**。TS 端 `BOUNDS` 仅 UI 镜像。
//!
//! 不变量：FORBIDDEN_FLOOR / DENIED_ARG_FLOOR 必须 ⊇ 现网 `run_command.rs`
//! 原硬编码集合（迁移即取并集，禁止缩窄）。

// ── 数值上限 floor/cap（用户值 clamp 到此区间）──
pub const MAX_TURNS: (usize, usize) = (1, 50);
pub const MAX_CPU_SECS: (u64, u64) = (1, 300);
pub const MAX_MEMORY_MB: (u64, u64) = (64, 4096);
pub const MAX_OPEN_FILES: (u64, u64) = (8, 1024);
pub const EXECUTION_TIMEOUT_SECS: (u64, u64) = (1, 300);
pub const MAX_OUTPUT_BYTES: (usize, usize) = (1024, 10_485_760);

// ── 用户默认值（config.ts 默认对齐；None 时 fallback）──
pub const DEFAULT_MAX_TURNS: usize = 10;
pub const DEFAULT_MAX_CPU_SECS: u64 = 30;
pub const DEFAULT_MAX_MEMORY_MB: u64 = 512;
pub const DEFAULT_MAX_OPEN_FILES: u64 = 64;
pub const DEFAULT_EXECUTION_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// 硬禁命令底线（31 项）。用户 forbidden_commands 与此取并集——用户只能加严。
pub const FORBIDDEN_FLOOR: &[&str] = &[
    // shell（任何 shell → 放弃 L1「不经 shell」防御）
    "sh", "bash", "zsh", "dash", "ksh", "fish", "csh", "tcsh",
    // macOS 特权 / 系统控制
    "osascript", "sudo", "open", "launchctl", "defaults", "networksetup", "scutil",
    // 进程管理（kill/ps/top 等侦察/控制）
    "killall", "kill", "pkill",
    // 触网（走专用 web_search 工具）
    "curl", "wget", "nc", "socat", "telnet", "ssh",
    // 提权 / 逃逸
    "su", "doas", "expect",
    // 数据持久化（走应用 API，防止直改 sqlite）
    "sqlite3",
    // 侦察
    "ps", "top", "htop",
];

/// 危险选项前缀底线（15 项）。用户 blocked_args 与此取并集——用户只能加严。
pub const DENIED_ARG_FLOOR: &[&str] = &[
    "--exec", "--exec-file", "--exec-rm",
    "--upload-pack",
    "--use-compress-program",
    "--config", "-C",                       // git -C 改 cwd / curl --config 读配置
    "--output", "-o", "-O", "--write-out", // curl/wget 写文件
    "--eval", "-e",                         // node/bash eval
    "--init-file", "--rcfile",
];

fn clamp_u64(v: u64, (floor, cap): (u64, u64)) -> u64 {
    v.clamp(floor, cap)
}
fn clamp_usize(v: usize, (floor, cap): (usize, usize)) -> usize {
    v.clamp(floor, cap)
}

/// 解析后的执行策略（agent_run 入口构造，run_command 消费）。
/// forbidden / denied_args 已取并集（用户值 ∪ 底线）；数值已 clamp 到 [floor, cap]。
#[derive(Clone)]
pub struct ExecPolicy {
    pub trusted: Vec<String>,
    pub forbidden: Vec<String>,
    pub denied_args: Vec<String>,
    pub max_cpu_secs: u64,
    pub max_memory_mb: u64,
    pub max_open_files: u64,
    pub execution_timeout_secs: u64,
    pub max_output_bytes: usize,
}

impl ExecPolicy {
    /// 默认策略（全部用户默认值 + 底线并集；测试与无 config fallback 用）。
    #[cfg(test)]
    pub fn default_with_trusted(trusted: Vec<String>) -> Self {
        Self::resolve(
            trusted,
            Vec::new(),
            Vec::new(),
            DEFAULT_MAX_CPU_SECS,
            DEFAULT_MAX_MEMORY_MB,
            DEFAULT_MAX_OPEN_FILES,
            DEFAULT_EXECUTION_TIMEOUT_SECS,
            DEFAULT_MAX_OUTPUT_BYTES,
        )
    }

    /// 从用户配置值解析（agent_run 入口调用，强制 clamp/并集）。
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        trusted: Vec<String>,
        user_forbidden: Vec<String>,
        user_denied: Vec<String>,
        max_cpu_secs: u64,
        max_memory_mb: u64,
        max_open_files: u64,
        execution_timeout_secs: u64,
        max_output_bytes: usize,
    ) -> Self {
        // forbidden / denied_args：用户值 ∪ 底线（去重，底线优先保留）
        let mut forbidden: Vec<String> = user_forbidden;
        for f in FORBIDDEN_FLOOR {
            if !forbidden.iter().any(|x| x == *f) {
                forbidden.push((*f).to_string());
            }
        }
        let mut denied: Vec<String> = user_denied;
        for d in DENIED_ARG_FLOOR {
            if !denied.iter().any(|x| x == *d) {
                denied.push((*d).to_string());
            }
        }
        Self {
            trusted,
            forbidden,
            denied_args: denied,
            max_cpu_secs: clamp_u64(max_cpu_secs, MAX_CPU_SECS),
            max_memory_mb: clamp_u64(max_memory_mb, MAX_MEMORY_MB),
            max_open_files: clamp_u64(max_open_files, MAX_OPEN_FILES),
            execution_timeout_secs: clamp_u64(execution_timeout_secs, EXECUTION_TIMEOUT_SECS),
            max_output_bytes: clamp_usize(max_output_bytes, MAX_OUTPUT_BYTES),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_unions_forbidden_floor() {
        // 用户自定义 + 底线并集
        let p = ExecPolicy::resolve(vec![], vec!["my_danger".into()], vec![], 30, 512, 64, 30, 1024);
        assert!(p.forbidden.contains(&"my_danger".to_string()));
        assert!(p.forbidden.contains(&"sudo".to_string())); // 底线仍在
        assert!(p.forbidden.contains(&"sh".to_string()));
    }

    #[test]
    fn resolve_clamps_values() {
        // 超 cap → clamp 到 cap；低于 floor → clamp 到 floor
        let p = ExecPolicy::resolve(vec![], vec![], vec![], 9999, 1, 1, 9999, 1);
        assert_eq!(p.max_cpu_secs, MAX_CPU_SECS.1);
        assert_eq!(p.max_memory_mb, MAX_MEMORY_MB.0);
        assert_eq!(p.max_open_files, MAX_OPEN_FILES.0);
        assert_eq!(p.execution_timeout_secs, EXECUTION_TIMEOUT_SECS.1);
        assert_eq!(p.max_output_bytes, MAX_OUTPUT_BYTES.0);
    }

    #[test]
    fn resolve_dedups_floor() {
        // 用户把底线项再写一遍 → 不重复
        let p = ExecPolicy::resolve(vec![], vec!["sudo".into()], vec!["--exec".into()], 30, 512, 64, 30, 1024);
        assert_eq!(p.forbidden.iter().filter(|x| x.as_str() == "sudo").count(), 1);
        assert_eq!(p.denied_args.iter().filter(|x| x.as_str() == "--exec").count(), 1);
    }

    #[test]
    fn floor_superset_of_original_hardcoded() {
        // 不变量：底线 ⊇ 现网原硬编码（31 forbidden + 15 denied）
        assert_eq!(FORBIDDEN_FLOOR.len(), 31);
        assert_eq!(DENIED_ARG_FLOOR.len(), 15);
        // 抽查关键项
        for must in ["sh", "bash", "sudo", "osascript", "curl", "sqlite3", "ps", "top", "kill"] {
            assert!(FORBIDDEN_FLOOR.contains(&must), "missing {must}");
        }
        for must in ["--exec", "--upload-pack", "-C", "-o", "-e", "--rcfile"] {
            assert!(DENIED_ARG_FLOOR.contains(&must), "missing {must}");
        }
    }
}
