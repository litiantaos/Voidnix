//! web_search 工具：Tavily 搜索。
//!
//! Tavily（https://tavily.com）专为 AI 设计的搜索 API，
//! 返回结构化 JSON 含 answer 字段（LLM 友好）。免费 1000 次/月，无需信用卡。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::extensions::agent::engine::tool_registry::{AgentTool, ToolResult};

const MAX_RESULTS: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
struct SearchOutcome {
    /// LLM 生成的答案摘要（Tavily 的 answer 字段）
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
    hits: Vec<SearchHit>,
}

pub struct WebSearchTool {
    api_key: String,
}

impl WebSearchTool {
    pub fn new(provider: crate::extensions::agent::SearchProviderConfig) -> Self {
        Self {
            api_key: provider.api_key,
        }
    }
}

#[async_trait]
impl AgentTool for WebSearchTool {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web for up-to-date information using Tavily. Returns top results with title, URL, and snippet. Use for factual queries, recent events, or when you need external knowledge.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query in natural language"
                        }
                    },
                    "required": ["query"]
                }
            }
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim();

        if query.is_empty() {
            return ToolResult::err("query is required");
        }

        if self.api_key.trim().is_empty() {
            return ToolResult::err(
                "Tavily API key not configured. Please add a Tavily provider in settings.",
            );
        }

        let client = crate::http::client();
        match search_tavily(client, &self.api_key, query).await {
            Ok(outcome) => ToolResult::ok(serde_json::to_string(&outcome).unwrap_or_default()),
            Err(e) => ToolResult::err(format!("Tavily search failed: {e}")),
        }
    }
}

/// ─── Tavily ──────────────────────────────────────────────

#[derive(Serialize)]
struct TavilyReq<'a> {
    query: &'a str,
    #[serde(rename = "search_depth")]
    search_depth: &'a str,
    #[serde(rename = "max_results")]
    max_results: u8,
    #[serde(rename = "include_answer")]
    include_answer: bool,
    #[serde(rename = "include_raw_content")]
    include_raw_content: bool,
}

#[derive(Deserialize)]
struct TavilyResp {
    answer: Option<String>,
    results: Vec<TavilyItem>,
}

#[derive(Deserialize)]
struct TavilyItem {
    title: String,
    url: String,
    content: String,
}

async fn search_tavily(
    client: &reqwest::Client,
    api_key: &str,
    query: &str,
) -> Result<SearchOutcome, String> {
    let body = TavilyReq {
        query,
        search_depth: "basic",
        max_results: MAX_RESULTS as u8,
        include_answer: true,
        include_raw_content: false,
    };
    let resp = client
        .post("https://api.tavily.com/search")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;

    match resp.status().as_u16() {
        200 => {
            let parsed: TavilyResp = resp.json().await.map_err(|e| format!("decode: {e}"))?;
            let hits = parsed
                .results
                .into_iter()
                .take(MAX_RESULTS)
                .map(|i| SearchHit {
                    title: i.title,
                    url: i.url,
                    snippet: truncate(&i.content, 280),
                })
                .collect();
            Ok(SearchOutcome {
                answer: parsed.answer,
                hits,
            })
        }
        401 => Err("Tavily key invalid".into()),
        429 | 432 | 433 => Err("Tavily rate/plan limit reached".into()),
        s => Err(format!("HTTP {}", s)),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max_chars).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> crate::extensions::agent::SearchProviderConfig {
        crate::extensions::agent::SearchProviderConfig {
            r#type: "tavily".into(),
            api_key: "tvly-test".into(),
        }
    }

    #[test]
    fn tool_name_and_schema() {
        let t = WebSearchTool::new(provider());
        assert_eq!(t.name(), "web_search");
        let schema = t.schema();
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["parameters"]["required"][0], "query");
    }

    #[tokio::test]
    async fn rejects_empty_query() {
        let t = WebSearchTool::new(provider());
        let result = t.call(serde_json::json!({"query": ""})).await;
        assert!(!result.ok);
    }

    #[tokio::test]
    async fn rejects_missing_api_key() {
        let p = crate::extensions::agent::SearchProviderConfig {
            r#type: "tavily".into(),
            api_key: "  ".into(),
        };
        let t = WebSearchTool::new(p);
        let result = t.call(serde_json::json!({"query": "rust"})).await;
        assert!(!result.ok);
        assert!(result.output.contains("not configured"));
    }

    #[test]
    fn truncate_handles_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_appends_ellipsis() {
        assert_eq!(truncate("abcdefghij", 5), "abcde…");
    }

    #[test]
    fn truncate_handles_multibyte() {
        assert_eq!(truncate("你好世界你好世界", 3), "你好世…");
    }
}
