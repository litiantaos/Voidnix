// 框架语义常量（单一真相源，不可配置）。
//
// 这些值定义了搜索模型的核心行为，集中声明以便审查与维护。
// 可调参数走 config 系统（config/framework.json），不在此处。
//
// ⚠️ 本文件是**空壳，目标删除**（RV §1.1/§3.1）：
//   - 搜索逻辑全在前端（fuzzy.ts + search-engine.ts），Rust 端零消费者。
//   - LLM 请求管道常量（MAX_SSE_BUFFER/MAX_MESSAGE_CONTENT_LEN 等）当前在
//     `runtime/llm/security.rs`，目标随 security.rs 溶解并入 `runtime/llm/client.rs`。
//   - sse.rs 已拆分为 client/security/parser/types 四模块（旧注释引用的 sse.rs 不复存在）。
