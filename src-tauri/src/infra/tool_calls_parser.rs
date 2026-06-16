//! OpenAI 兼容 endpoint 的流式 tool_calls 解析器。
//!
//! 按 `delta.tool_calls[].index` 路由，累积 `function.arguments` 分片，
//! 在 `finish_reason == "tool_calls"` 时 finalize 成完整 tool_calls。
//!
//! 兼容 OpenAI / DeepSeek / Kimi / Qwen / 智谱（GLM）等。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 单个 tool_call delta（流式中的一帧）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ToolCallDelta {
    /// 必有：标识本次 delta 属于第几个 tool_call
    pub index: u32,
    /// 仅首个 delta 有：tool_call 唯一 ID（如 `call_xxx` / Kimi 的 `search:0`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 仅首个 delta 有：恒为 `"function"`
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 几乎每个 delta 都有：name 仅首帧，arguments 是分片
    #[serde(default)]
    pub function: Option<FunctionDelta>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FunctionDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// JSON 字符串分片；缺失或无参数时为空串
    #[serde(default)]
    pub arguments: String,
}

/// SSE chunk 的 `choices[0].delta` 子集（仅取本解析器关心的字段）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChoiceDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallDelta>,
}

/// finalize 成功后的完整 tool_call。
#[derive(Debug, Clone, Serialize)]
pub struct FinalizedToolCall {
    pub id: String,
    pub name: String,
    /// 已 parse 的 JSON 值（空 arguments 时为 `{}`）
    pub arguments: serde_json::Value,
}

#[derive(Debug)]
pub enum FinalizeError {
    MissingId { index: u32 },
    MissingName { index: u32 },
    BadJson {
        index: u32,
        #[allow(dead_code)]
        raw: String,
        err: serde_json::Error,
    },
}

impl std::fmt::Display for FinalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingId { index } => write!(f, "tool_call[{}] missing id", index),
            Self::MissingName { index } => write!(f, "tool_call[{}] missing function.name", index),
            Self::BadJson { index, err, .. } => {
                write!(f, "tool_call[{}] arguments JSON parse failed: {}", index, err)
            }
        }
    }
}

impl std::error::Error for FinalizeError {}

/// tool_calls 累积器。
///
/// 使用：
/// ```ignore
/// let mut acc = ToolCallAccumulator::default();
/// for chunk in stream {
///     if let Some(delta) = parse_delta(chunk) {
///         acc.process_delta(&delta);
///     }
/// }
/// let calls = acc.finalize()?;
/// ```
#[derive(Default)]
pub struct ToolCallAccumulator {
    calls: BTreeMap<u32, (Option<String>, Option<String>, String)>,
}

impl ToolCallAccumulator {
    /// 处理一帧 delta。空 tool_calls 数组的 delta 是合法的（无操作）。
    pub fn process_delta(&mut self, delta: &ChoiceDelta) {
        for tc in &delta.tool_calls {
            let entry = self.calls.entry(tc.index).or_default();
            if let Some(id) = &tc.id {
                entry.0 = Some(id.clone());
            }
            if let Some(f) = &tc.function {
                if let Some(name) = &f.name {
                    entry.1 = Some(name.clone());
                }
                if !f.arguments.is_empty() {
                    entry.2.push_str(&f.arguments);
                }
            }
        }
    }

    /// 当前累积的 tool_call 数量（监控用）。
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// 在 `finish_reason == "tool_calls"` 时调用，按 index 升序产出完整 tool_calls。
    ///
    /// 容错：
    /// - 空 arguments 视为 `{}`（兼容无参函数）
    /// - JSON 解析失败返回 `FinalizeError::BadJson`（不执行）
    /// - id 缺失（极少数国产 API）返回 `MissingId`，由上层决定是否兜底生成
    ///
    /// 注意：取 `&self`（不消费）便于失败后调用 `finalize_lenient` 兜底。
    pub fn finalize(&self) -> Result<Vec<FinalizedToolCall>, FinalizeError> {
        let mut out = Vec::with_capacity(self.calls.len());
        for (index, (id, name, args)) in &self.calls {
            let id = id.clone().ok_or(FinalizeError::MissingId { index: *index })?;
            let name = name.clone().ok_or(FinalizeError::MissingName { index: *index })?;
            let parsed = if args.trim().is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(args).map_err(|e| FinalizeError::BadJson {
                    index: *index,
                    raw: args.clone(),
                    err: e,
                })?
            };
            out.push(FinalizedToolCall { id, name, arguments: parsed });
        }
        Ok(out)
    }

    /// 与 `finalize` 相同，但 id 缺失时自动生成 `gen_{index}` 兜底（不报错）。
    ///
    /// 用于已知目标 API 偶发不带 id 的情况（如部分 GLM-4 历史版本）。
    pub fn finalize_lenient(&self) -> Vec<FinalizedToolCall> {
        let mut out = Vec::with_capacity(self.calls.len());
        for (index, (id, name, args)) in &self.calls {
            let id = id.clone().unwrap_or_else(|| format!("gen_{}", index));
            let name = name.clone().unwrap_or_default();
            let parsed = if args.trim().is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(args).unwrap_or(serde_json::Value::Null)
            };
            out.push(FinalizedToolCall { id, name, arguments: parsed });
        }
        out
    }
}

/// 从一个 SSE chunk 的原始 JSON 文本中提取 `choices[0].delta`。
///
/// 返回 `None` 当：
/// - JSON 解析失败
/// - 没有 choices 数组（如 usage-only 末尾 chunk）
///
/// 主要为测试和外部消费者使用；sse.rs 内部用内联解析（功能等价）。
#[allow(dead_code)]
pub fn parse_choice_delta(raw_json: &str) -> Option<ChoiceDelta> {
    #[derive(Deserialize)]
    struct Chunk {
        #[serde(default)]
        choices: Vec<Choice>,
    }
    #[derive(Deserialize)]
    struct Choice {
        #[serde(default)]
        delta: ChoiceDelta,
    }
    let chunk: Chunk = serde_json::from_str(raw_json).ok()?;
    chunk.choices.into_iter().next().map(|c| c.delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 模拟 OpenAI 标准流式 tool_calls 序列（get_weather("Paris, France")）。
    fn openai_stream_chunks() -> Vec<&'static str> {
        vec![
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":null,"type":null,"function":{"name":null,"arguments":"{"}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":null,"type":null,"function":{"name":null,"arguments":"\"location"}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":null,"type":null,"function":{"name":null,"arguments":"\":\"Paris, France\"}"}}]}}]}"#,
        ]
    }

    #[test]
    fn accumulates_openai_standard_sequence() {
        let mut acc = ToolCallAccumulator::default();
        for raw in openai_stream_chunks() {
            let delta = parse_choice_delta(raw).unwrap();
            acc.process_delta(&delta);
        }
        assert_eq!(acc.len(), 1);
        let calls = acc.finalize().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc");
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments["location"], "Paris, France");
    }

    #[test]
    fn handles_empty_arguments_as_empty_object() {
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"no_args","arguments":""}}]}}]}"#;
        let mut acc = ToolCallAccumulator::default();
        acc.process_delta(&parse_choice_delta(raw).unwrap());
        let calls = acc.finalize().unwrap();
        assert_eq!(calls[0].arguments, serde_json::json!({}));
    }

    #[test]
    fn handles_multiple_parallel_tool_calls_by_index() {
        let chunks = vec![
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"a","arguments":""}},{"index":1,"id":"c2","type":"function","function":{"name":"b","arguments":""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"x\":1}"}},{"index":1,"function":{"arguments":"{\"y\":2}"}}]}}]}"#,
        ];
        let mut acc = ToolCallAccumulator::default();
        for raw in chunks {
            acc.process_delta(&parse_choice_delta(raw).unwrap());
        }
        let calls = acc.finalize().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "a");
        assert_eq!(calls[0].arguments["x"], 1);
        assert_eq!(calls[1].name, "b");
        assert_eq!(calls[1].arguments["y"], 2);
    }

    #[test]
    fn handles_kimi_id_format() {
        // Kimi 用 "search:0" 而非 "call_xxx"，不应假设格式
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"search:0","type":"function","function":{"name":"web_search","arguments":"{\"q\":\"rust\"}"}}]}}]}"#;
        let mut acc = ToolCallAccumulator::default();
        acc.process_delta(&parse_choice_delta(raw).unwrap());
        let calls = acc.finalize().unwrap();
        assert_eq!(calls[0].id, "search:0");
        assert_eq!(calls[0].arguments["q"], "rust");
    }

    #[test]
    fn bad_json_returns_finalize_error() {
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c","type":"function","function":{"name":"f","arguments":"{invalid"}}]}}]}"#;
        let mut acc = ToolCallAccumulator::default();
        acc.process_delta(&parse_choice_delta(raw).unwrap());
        let result = acc.finalize();
        assert!(matches!(result, Err(FinalizeError::BadJson { .. })));
    }

    #[test]
    fn lenient_finalize_falls_back_on_errors() {
        // 缺 id + 坏 JSON，lenient 仍返回结果
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"f","arguments":"{bad"}}]}}]}"#;
        let mut acc = ToolCallAccumulator::default();
        acc.process_delta(&parse_choice_delta(raw).unwrap());
        let calls = acc.finalize_lenient();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "gen_0");
        assert_eq!(calls[0].arguments, serde_json::Value::Null);
    }

    #[test]
    fn empty_chunk_choice_returns_none() {
        assert!(parse_choice_delta(r#"{"choices":[]}"#).is_none());
        assert!(parse_choice_delta(r#"not json"#).is_none());
    }

    #[test]
    fn content_only_chunk_does_not_affect_accumulator() {
        let raw = r#"{"choices":[{"index":0,"delta":{"content":"hello"}}]}"#;
        let mut acc = ToolCallAccumulator::default();
        acc.process_delta(&parse_choice_delta(raw).unwrap());
        assert!(acc.is_empty());
    }

    #[test]
    fn interleaved_chunks_for_parallel_calls() {
        // 不同 index 的 arguments 分片可以交错到达，但同 index 内部按序
        let chunks = vec![
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"a","type":"function","function":{"name":"a","arguments":""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"b","type":"function","function":{"name":"b","arguments":""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"x\""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"y\""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":1}"}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"function":{"arguments":":2}"}}]}}]}"#,
        ];
        let mut acc = ToolCallAccumulator::default();
        for raw in chunks {
            acc.process_delta(&parse_choice_delta(raw).unwrap());
        }
        let calls = acc.finalize().unwrap();
        assert_eq!(calls[0].arguments, serde_json::json!({"x": 1}));
        assert_eq!(calls[1].arguments, serde_json::json!({"y": 2}));
    }
}
