//! Agent 命令执行资源约束。
//!
//! 权威源：资源上限 floor/cap 定义于此 Rust const，`agent_run` 入口强制 clamp。
//! 命令无白名单/黑名单拦截——所有命令直接放行，仅 run_command 的断路器
//! （`rm -rf /` 等灾难性操作）兜底。TS 端 `BOUNDS` 仅 CI 镜像（无 Settings UI）。

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

fn clamp_u64(v: u64, (floor, cap): (u64, u64)) -> u64 {
    v.clamp(floor, cap)
}
fn clamp_usize(v: usize, (floor, cap): (usize, usize)) -> usize {
    v.clamp(floor, cap)
}

/// 解析后的执行策略（agent_run 入口构造，run_command 消费）。
/// 仅含资源上限（数值已 clamp 到 [floor, cap]）；命令无白/黑名单拦截。
#[derive(Clone)]
pub struct ExecPolicy {
    pub max_cpu_secs: u64,
    pub max_memory_mb: u64,
    pub max_open_files: u64,
    pub execution_timeout_secs: u64,
    pub max_output_bytes: usize,
}

impl ExecPolicy {
    /// 从用户配置值解析（agent_run 入口调用，强制 clamp）。
    pub fn resolve(
        max_cpu_secs: u64,
        max_memory_mb: u64,
        max_open_files: u64,
        execution_timeout_secs: u64,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            max_cpu_secs: clamp_u64(max_cpu_secs, MAX_CPU_SECS),
            max_memory_mb: clamp_u64(max_memory_mb, MAX_MEMORY_MB),
            max_open_files: clamp_u64(max_open_files, MAX_OPEN_FILES),
            execution_timeout_secs: clamp_u64(execution_timeout_secs, EXECUTION_TIMEOUT_SECS),
            max_output_bytes: clamp_usize(max_output_bytes, MAX_OUTPUT_BYTES),
        }
    }
}

#[cfg(test)]
impl Default for ExecPolicy {
    fn default() -> Self {
        Self::resolve(
            DEFAULT_MAX_CPU_SECS,
            DEFAULT_MAX_MEMORY_MB,
            DEFAULT_MAX_OPEN_FILES,
            DEFAULT_EXECUTION_TIMEOUT_SECS,
            DEFAULT_MAX_OUTPUT_BYTES,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_clamps_values() {
        // 超 cap → clamp 到 cap；低于 floor → clamp 到 floor
        let p = ExecPolicy::resolve(9999, 1, 1, 9999, 1);
        assert_eq!(p.max_cpu_secs, MAX_CPU_SECS.1);
        assert_eq!(p.max_memory_mb, MAX_MEMORY_MB.0);
        assert_eq!(p.max_open_files, MAX_OPEN_FILES.0);
        assert_eq!(p.execution_timeout_secs, EXECUTION_TIMEOUT_SECS.1);
        assert_eq!(p.max_output_bytes, MAX_OUTPUT_BYTES.0);
    }

    #[test]
    fn resolve_keeps_in_range() {
        let p = ExecPolicy::resolve(30, 512, 64, 30, 1024);
        assert_eq!(p.max_cpu_secs, 30);
        assert_eq!(p.max_memory_mb, 512);
        assert_eq!(p.max_open_files, 64);
        assert_eq!(p.execution_timeout_secs, 30);
        assert_eq!(p.max_output_bytes, 1024);
    }
}
