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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestResp {
    pub suggestion: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
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
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct RecordResp {}

#[derive(Debug, Serialize, Deserialize)]
pub struct PingResp {
    pub pong: bool,
}
