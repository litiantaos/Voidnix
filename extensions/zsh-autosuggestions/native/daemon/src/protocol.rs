use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestReq {
    pub buffer: String,
    pub dir: String,
    pub prev: String,
    #[serde(default)]
    pub prev_prev: String,
    #[serde(default)]
    pub prev_exit: i32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SuggestResp {
    pub suggestion: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecordReq {
    pub command: String,
    pub dir: String,
    #[serde(rename = "exit")]
    pub exit_code: i32,
    pub duration: i64,
    pub session: String,
    pub prev: String,
    #[serde(default)]
    pub prev_prev: String,
    #[serde(default)]
    pub prev_exit: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct RecordResp {}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedbackReq {
    pub command: String,
    pub kind: String,
    #[serde(default)]
    pub session: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PingResp {
    pub pong: bool,
}
