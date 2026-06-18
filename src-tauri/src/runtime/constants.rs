/// 框架语义常量（单一真相源，不可配置）。
///
/// 这些值定义了搜索模型的核心行为，集中声明以便审查与维护。
/// 可调参数走 config 系统（config/framework.json），不在此处。

/// SSE 安全上限
pub const MAX_SSE_BUFFER: usize = 1_048_576; // 1 MiB
