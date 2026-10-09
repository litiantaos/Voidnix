//! 使用事件流:网关转发请求的内存事件记录——驱动扩展视图的活跃链路可视化
//! (「正在使用哪个提供商的哪个模型」)。有意新增的内存态成功记录,gateway.log
//! 日志纪律不变(仍只记异常路径);不落盘、重启清零、不含 Key 与请求体。
//! 记录点 = 上游 2xx 响应头到达(路由成功、应答开始),非流完整结束——统计语义是
//! 「使用过」而非「应答成功」,与粘性/亲和的严格提交点(prompt cache 语义)不同层;
//! count_tokens 等核算调用同记,保持零特例。

use super::server::Gateway;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// 内存事件上限:须覆盖前端 90s 活跃窗口(ACTIVE_MS)在最密集会话下的全部事件——
/// CC 每回合 messages + count_tokens 约 2-4 事件、并行会话成倍放大,>1.4 事件/s
/// 持续 90s 即 128,故取 512 留倍余;事件约 120B,上限合计 ~60KB。
/// 调整任一侧须同步另一侧(前端 FlowStage.vue::ACTIVE_MS)
const EVENT_CAP: usize = 512;

/// 单次转发事件(经 StatusReport → 前端,camelCase);seq 单调递增供前端识别
/// 新事件驱动粒子动画(同毫秒多请求时 ts 不保唯一)
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvent {
    pub seq: u64,
    /// 归一裸名(norm_model 剥 [Nm] 后缀,glm-5.3[1m] 与 glm-5.3 合并)
    pub model: String,
    /// 提供商展示名(记录时的 route.name)
    pub provider: String,
    /// 接入协议面("anthropic" | "responses" | "chat")——前端据此归并消费者节点
    pub protocol: &'static str,
    /// 接入身份标签(占位 Key 匹配登记表;None = 未登记,前端按协议面兜底)
    pub consumer: Option<String>,
    /// 事件时间(epoch 毫秒,与 gateway.log 时间戳同源)
    pub ts: i64,
}

/// 时间正序追加的环形事件缓冲(超 cap 弹最老);换表保留(事件是历史事实,
/// 无 (provider, idx) 错位问题),重启清零(内存态)
#[derive(Default)]
pub(super) struct UsageTable {
    inner: Mutex<Vec<UsageEvent>>,
    seq: AtomicU64,
}

impl UsageTable {
    /// 记录一次使用(2xx 响应头到达);seq 在锁内分配——锁外 fetch_add 会因并发
    /// 加锁顺序颠倒使 snapshot「最新在前」不变量被打破,前端 seq 判新漏/重
    pub(super) fn record(
        &self,
        model: &str,
        provider: &str,
        protocol: &'static str,
        consumer: Option<String>,
    ) {
        let mut v = self.inner.lock().unwrap();
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        v.push(UsageEvent {
            seq,
            model: model.to_string(),
            provider: provider.to_string(),
            protocol,
            consumer,
            ts: now_ms(),
        });
        let n = v.len();
        if n > EVENT_CAP {
            v.drain(..n - EVENT_CAP);
        }
    }

    /// 读侧快照:最新在前(前端活跃窗口与增量判新都以头部为准)
    pub(super) fn snapshot(&self) -> Vec<UsageEvent> {
        self.inner.lock().unwrap().iter().rev().cloned().collect()
    }
}

/// epoch 毫秒(state.rs 日志时间戳同源复用)
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Gateway {
    /// proxy 成功路径的记录入口(model 传归一裸名)
    pub(super) fn record_usage(
        &self,
        model: &str,
        provider: &str,
        protocol: &'static str,
        consumer: Option<String>,
    ) {
        self.usage.record(model, provider, protocol, consumer);
    }

    /// 测试断言用:最新在前的快照
    #[cfg(test)]
    pub(super) fn usage_snapshot(&self) -> Vec<UsageEvent> {
        self.usage.snapshot()
    }
}

// ─── 单元测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::ai_gateway::server::tests::{
        anthropic_post, gateway_with, route, spawn_upstream,
    };
    use crate::extensions::ai_gateway::server::{proxy, Protocol};
    use axum::body::Body;
    use axum::extract::Request;
    use std::sync::Arc;

    #[test]
    fn record_appends_with_monotonic_seq_and_caps() {
        let t = UsageTable::default();
        for i in 0..(EVENT_CAP + 20) as u64 {
            t.record("glm-5.3", "智谱", "anthropic", None);
            let snap = t.snapshot();
            assert_eq!(snap[0].seq, i, "seq 单调递增");
            assert!(snap.len() <= EVENT_CAP, "超 cap 弹最老");
        }
        let snap = t.snapshot();
        assert_eq!(snap.len(), EVENT_CAP);
        assert_eq!(snap[0].seq, EVENT_CAP as u64 + 19, "最新在前");
        assert_eq!(snap[snap.len() - 1].seq, 20, "最老被弹出");
        // 事件字段完整
        assert_eq!(
            snap[0],
            UsageEvent {
                seq: snap[0].seq,
                model: "glm-5.3".into(),
                provider: "智谱".into(),
                protocol: "anthropic",
                consumer: None,
                ts: snap[0].ts,
            }
        );
    }

    /// 2xx 即记 + [Nm] 后缀归一合并(记录 key 语义的回归锚点)
    #[tokio::test]
    async fn usage_records_on_2xx_and_merges_suffix() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let upstream = spawn_upstream(200, b"{}", vec![], seen).await;
        let g = gateway_with(vec![route("zhipu", &upstream, &["glm-5.3"])]);
        for model in ["glm-5.3", "glm-5.3[1m]"] {
            let body =
                format!(r#"{{"model":"{model}","messages":[{{"role":"user","content":"x"}}]}}"#);
            let resp = proxy(g.clone(), Protocol::Anthropic, anthropic_post(&body)).await;
            assert!(resp.status().is_success());
        }
        let snap = g.usage_snapshot();
        assert_eq!(snap.len(), 2);
        assert!(snap.iter().all(|e| e.model == "glm-5.3"), "后缀变体归一");
        assert_eq!(snap[0].seq, 1, "最新在前");
        assert!(snap
            .iter()
            .all(|e| e.provider == "zhipu" && e.protocol == "anthropic"));
    }

    /// 「占位 Key 即身份」:voidnix-<slug> 凭证解析出工具名,不匹配约定为 None;
    /// 自带登录态的 CLI(session token 凭证)经 User-Agent 特征兜底识别
    #[tokio::test]
    async fn usage_records_consumer_from_key_slug() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let upstream = spawn_upstream(200, b"{}", vec![], seen).await;
        let g = gateway_with(vec![route("zhipu", &upstream, &["glm-5.3"])]);
        let req = |auth: &str| {
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {auth}"))
                .body(Body::from(
                    r#"{"model":"glm-5.3","messages":[{"role":"user","content":"x"}]}"#.to_string(),
                ))
                .unwrap()
        };
        for auth in ["voidnix-codex", "sk-random", "voidnix-claude-code"] {
            let resp = proxy(g.clone(), Protocol::Anthropic, req(auth)).await;
            assert!(resp.status().is_success());
        }
        let snap = g.usage_snapshot();
        assert_eq!(snap[2].consumer.as_deref(), Some("Codex"), "slug 转显示名");
        assert_eq!(snap[1].consumer.as_deref(), None, "不匹配约定且无 UA 特征");
        assert_eq!(snap[0].consumer.as_deref(), Some("Claude Code"));
    }

    /// grok 类 CLI 发自家 session token(非配置 key),UA 特征兜底识别
    #[tokio::test]
    async fn usage_records_consumer_from_user_agent() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let upstream = spawn_upstream(200, b"{}", vec![], seen).await;
        let g = gateway_with(vec![route("zhipu", &upstream, &["glm-5.3"])]);
        let body = r#"{"model":"glm-5.3","messages":[{"role":"user","content":"x"}]}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("content-type", "application/json")
            .header("authorization", "Bearer eyJ0eXAiOiJhdCtqd3Qi")
            .header(
                "user-agent",
                "grok-pager/1.0.50 grok-shell/1.0.50 (macos; aarch64)",
            )
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = proxy(g.clone(), Protocol::Anthropic, req).await;
        assert!(resp.status().is_success());
        assert_eq!(g.usage_snapshot()[0].consumer.as_deref(), Some("Grok"));
    }

    /// 失败路径零记录:429 耗尽回放与未知模型 404 都不产生事件
    #[tokio::test]
    async fn usage_not_recorded_on_failures() {
        let upstream = spawn_upstream(
            429,
            b"{\"error\":{\"message\":\"rate limited\"}}",
            vec![("retry-after", "0".to_string())],
            Arc::new(Mutex::new(Vec::new())),
        )
        .await;
        let g = gateway_with(vec![route("zhipu", &upstream, &["glm-5.3"])]);
        let resp = proxy(
            g.clone(),
            Protocol::Anthropic,
            anthropic_post(r#"{"model":"glm-5.3","messages":[{"role":"user","content":"x"}]}"#),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 429);
        let resp = proxy(
            g.clone(),
            Protocol::Anthropic,
            anthropic_post(r#"{"model":"unknown-m","messages":[{"role":"user","content":"x"}]}"#),
        )
        .await;
        assert_eq!(resp.status().as_u16(), 404);
        assert!(g.usage_snapshot().is_empty());
    }
}
