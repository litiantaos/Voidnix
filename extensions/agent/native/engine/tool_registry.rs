//! AgentTool trait + ToolRegistry。

use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// 工具执行结果。
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    /// 业务是否成功（仅驱动前端成功/失败态；loop 仍把 output 原文回灌 LLM）
    pub ok: bool,
    /// 已净化 secret 的输出；ok=false 时可以是短错误描述，也可以是完整工具输出（如非 0 退出的 stdout/stderr）
    pub output: String,
}

impl ToolResult {
    pub fn ok<S: Into<String>>(output: S) -> Self {
        Self {
            ok: true,
            output: output.into(),
        }
    }

    pub fn err<S: Into<String>>(output: S) -> Self {
        Self {
            ok: false,
            output: output.into(),
        }
    }
}

/// 工具 trait：name + JSON Schema + 异步执行。
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// 工具名（LLM 看到的 function name，唯一）
    fn name(&self) -> &'static str;

    /// OpenAI tools schema（`{type:"function", function:{name, description, parameters}}`）
    fn schema(&self) -> serde_json::Value;

    /// 执行工具。args 已经过 JSON parse；返回结果（ok/err）。
    async fn call(&self, args: serde_json::Value) -> ToolResult;
}

/// 工具注册中心。
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<&'static str, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: AgentTool + 'static>(mut self, tool: T) -> Self {
        let name = tool.name();
        self.tools.insert(name, Arc::new(tool));
        self
    }

    /// 聚合成 OpenAI `tools[]` 数组，传给 LLM。
    pub fn collect_tools_schema(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    /// 按 name 查找工具。
    pub fn find(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl AgentTool for EchoTool {
        fn name(&self) -> &'static str {
            "echo"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": "echo",
                    "description": "echo back",
                    "parameters": {"type":"object","properties":{"msg":{"type":"string"}}}
                }
            })
        }
        async fn call(&self, args: serde_json::Value) -> ToolResult {
            ToolResult::ok(args.to_string())
        }
    }

    #[test]
    fn register_and_find() {
        let reg = ToolRegistry::new().register(EchoTool);
        assert!(reg.find("echo").is_some());
        assert!(reg.find("missing").is_none());
    }

    #[test]
    fn collect_schema_returns_array() {
        let reg = ToolRegistry::new().register(EchoTool);
        let schema = reg.collect_tools_schema();
        assert_eq!(schema.len(), 1);
        assert_eq!(schema[0]["type"], "function");
        assert_eq!(schema[0]["function"]["name"], "echo");
    }
}
