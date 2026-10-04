//! 2xx 应答流的守护:提交时机与异常流收尾。
//!
//! GuardStream 包装上游字节流原样透传(零解析),流终止时一次性结算——语义完整才
//! 提交粘性/亲和(GuardCommit 回写 server 的 Gateway 状态;收到响应头 ≠ 应答成功),
//! 尾部哨兵(TailWatch)判异常则注入协议原生 error 事件收尾。字节扫描基元
//! (find_bytes/contains_bytes/extract_json_string)随哨兵居此,normalize 的分类器
//! 标签判据复用 contains_bytes。

use super::server::{log_gateway, Gateway};
use axum::body::Bytes;
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// 2xx 应答流的提交数据:流完整结束才执行——收到响应头 ≠ 应答成功,流中途断开时
/// 上游 prompt cache 并未建立,粘坏 Key = 客户端重试再撞一次同款失败
pub(super) struct GuardCommit {
    pub(super) g: Arc<Gateway>,
    pub(super) provider_id: String,
    pub(super) idx: usize,
    pub(super) session: Option<u64>,
}

impl GuardCommit {
    fn run(self) {
        self.g.mark_good(&self.provider_id, Some(self.idx));
        if let Some(s) = self.session {
            self.g.bind_session(s, &self.provider_id, self.idx);
        }
    }
}

/// 2xx 应答流的守护参数:commit 恒挂(流完整结束才提交);sentinel 仅 Anthropic 面
/// 填 Some(model, route) 且响应确为 SSE 时生效(非流式 JSON 无 message_stop 事件帧,
/// 误判为噪声)
pub(super) struct GuardCtx {
    pub(super) commit: GuardCommit,
    pub(super) sentinel: Option<(String, String)>,
}

/// 直 pipe 流的守护包装:chunk 原样透传 + 哨兵过路探测;流终止时一次性结算
/// (take 即空,天然幂等)——
/// ① 哨兵(Anthropic SSE)语义完整(内容 + stop_reason + message_stop)或无哨兵时
///   干净 EOF → 提交粘性/亲和(应答成功的唯一判定点)
/// ② 哨兵判异常 → 落 tail 日志 + 注入协议原生 error 事件收尾(「200 但语义可疑」
///   从纯日志观测升级为客户端可感知失败),不提交
/// ③ 无哨兵(非 Anthropic 面 / 非流式 JSON):传输错误即截断,不提交不注入
///   (客户端由连接断开感知);哨兵在场时上游传输错误一律吞掉转干净 EOF——
///   内容已完整则无害收尾,内容残缺则由注入的 error 事件承载失败语义
pub(super) struct GuardStream<S> {
    inner: S,
    /// Anthropic 面 SSE 的哨兵观测 + 日志所需的 model/route
    sentinel: Option<(TailWatch, String, String)>,
    /// 流完整结束才执行的提交
    commit: Option<GuardCommit>,
    /// 哨兵判异常后待注入的 error 事件字节(stream 协议 None 后不再 poll,须先行冲出)
    pending: Option<Bytes>,
    /// inner 已终止(EOF/Err)且已结算:终止态由本层自持,不再 poll inner——
    /// 上游流在 Err 后的重复 poll 行为无合约(可能重复 Err 或 Pending 挂死,
    /// 后者悬挂客户端连接)
    ended: bool,
}

impl<S, E> GuardStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    pub(super) fn new(inner: S, commit: GuardCommit, meta: Option<(String, String)>) -> Self {
        Self {
            inner,
            sentinel: meta.map(|(model, route)| (TailWatch::new(), model, route)),
            commit: Some(commit),
            pending: None,
            ended: false,
        }
    }

    /// 流终止的一次性结算;返回 true = 吞掉上游传输错误转干净 EOF
    fn settle(&mut self, errored: bool) -> bool {
        let Some(commit) = self.commit.take() else {
            return false;
        };
        match self.sentinel.take() {
            Some((watch, model, route)) => match watch.finish() {
                None => {
                    commit.run();
                    true
                }
                Some(detail) => {
                    log_gateway("tail", &format!("{model} · {route} · {detail}"));
                    self.pending = Some(anthropic_error_event(&detail));
                    true
                }
            },
            None => {
                if !errored {
                    commit.run();
                }
                false
            }
        }
    }

    /// 终止收尾:先冲注入事件再 EOF
    fn emit_end(&mut self) -> Poll<Option<std::io::Result<Bytes>>> {
        match self.pending.take() {
            Some(b) => Poll::Ready(Some(Ok(b))),
            None => Poll::Ready(None),
        }
    }
}

impl<S, E> Stream for GuardStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    type Item = std::io::Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.ended {
            return this.emit_end();
        }
        match this.inner.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.settle(false);
                this.ended = true;
                this.emit_end()
            }
            Poll::Ready(Some(Err(e))) => {
                let swallowed = this.settle(true);
                this.ended = true;
                if swallowed {
                    this.emit_end()
                } else {
                    Poll::Ready(Some(Err(std::io::Error::other(e.to_string()))))
                }
            }
            Poll::Ready(Some(Ok(bytes))) => {
                if let Some((watch, _, _)) = this.sentinel.as_mut() {
                    watch.observe(&bytes);
                }
                Poll::Ready(Some(Ok(bytes)))
            }
        }
    }
}

/// Anthropic 面 SSE 异常流的收尾错误事件帧:流以协议原生错误事件而非静默截断结束,
/// 客户端感知失败可重试(参考 magpie streamFailure 的「绝不伪造正常终止」不变量)
fn anthropic_error_event(detail: &str) -> Bytes {
    let payload = serde_json::json!({
        "type": "error",
        "error": { "type": "api_error", "message": format!("upstream stream incomplete: {detail}") },
    });
    Bytes::from(format!("event: error\ndata: {payload}\n\n"))
}

// ─── 字节扫描基元(哨兵探测主消费;normalize 分类器标签判据复用)───

/// 朴素子串查找(windows 线性扫;release 实测 ~0.8ms/MB,相对网络传输可忽略,
/// 无需真 memchr 算法——命名如实)
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub(super) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    find_bytes(haystack, needle).is_some()
}

/// 从缓冲提取 JSON 字符串值(`"key":"` 前缀定位读到下一个引号;哨兵专用,非通用解析)
fn extract_json_string<'a>(buf: &'a [u8], prefix: &[u8]) -> Option<&'a str> {
    let start = find_bytes(buf, prefix)? + prefix.len();
    let end = start + buf[start..].iter().position(|&b| b == b'"')?;
    std::str::from_utf8(&buf[start..end]).ok()
}

// ─── 尾部哨兵 ─────────────────────────────────────────────

/// 标记子串残余窗口:子串可能恰好跨 chunk 边界被切,扫描窗口拼上次残余防漏检
const MARKER_CARRY: usize = 16;

/// 末尾环形缓冲容量:message_delta(stop_reason + usage)与 message_stop 恒在流尾
const TAIL_CAP: usize = 4 * 1024;

/// 尾部哨兵:Anthropic 面 2xx SSE 流保持零解析直 pipe,字节过路做子串探测——
/// text/tool/thinking 内容证据 + stop_reason 到达标记 + 末尾环形缓冲提取
/// stop_reason 值与 message_stop。流终止(干净 EOF 或断流)时判异常:零内容
/// (无 text 无 tool)或未收 stop_reason 或未收 message_stop,即「200 但语义可疑」
/// ——GLM 无视 thinking disabled 耗尽预算产出空响应、裸 JSON 错误行致流静默
/// 终止等故障此前零痕迹(CC 只报「模型不可用」);正常完成的流零记录
/// (成功请求零记录约定不变)。
struct TailWatch {
    text: bool,
    tool: bool,
    think: bool,
    saw_stop: bool,
    tail: Vec<u8>,
    carry: Vec<u8>,
}

impl TailWatch {
    fn new() -> Self {
        Self {
            text: false,
            tool: false,
            think: false,
            saw_stop: false,
            tail: Vec::new(),
            carry: Vec::new(),
        }
    }

    /// 内容证据采样:text/tool/think 以 delta 事件为准(tool_use 块开始即证据,
    /// 块内无 delta),~2ms/MB 相对流解析可忽略
    fn observe(&mut self, chunk: &[u8]) {
        let mut window = std::mem::take(&mut self.carry);
        window.extend_from_slice(chunk);
        self.text |= contains_bytes(&window, b"\"text_delta\"");
        self.tool |= contains_bytes(&window, b"\"tool_use\"");
        self.think |= contains_bytes(&window, b"\"thinking_delta\"");
        self.saw_stop |= contains_bytes(&window, b"\"stop_reason\"");
        self.carry = window[window.len().saturating_sub(MARKER_CARRY)..].to_vec();
        if chunk.len() >= TAIL_CAP {
            self.tail.clear();
            self.tail
                .extend_from_slice(&chunk[chunk.len() - TAIL_CAP..]);
        } else {
            let keep = TAIL_CAP - chunk.len();
            if self.tail.len() > keep {
                self.tail.drain(..self.tail.len() - keep);
            }
            self.tail.extend_from_slice(chunk);
        }
    }

    /// 异常判定:Some(日志详情) 才落 tail 行;
    /// stop:具体值 / null(键在值空) / -(键缺席,未收到 message_delta)
    fn finish(&self) -> Option<String> {
        let ended = contains_bytes(&self.tail, b"\"message_stop\"");
        if (self.text || self.tool) && self.saw_stop && ended {
            return None;
        }
        let stop = match extract_json_string(&self.tail, b"\"stop_reason\":\"") {
            Some(v) => v.to_string(),
            None if self.saw_stop => "null".to_string(),
            None => "-".to_string(),
        };
        Some(format!(
            "text={} tool={} think={} stop={stop} end={}",
            yn(self.text),
            yn(self.tool),
            yn(self.think),
            if ended { "ok" } else { "trunc" }
        ))
    }
}

fn yn(b: bool) -> &'static str {
    if b {
        "Y"
    } else {
        "N"
    }
}

// ─── 单元测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_watch_silent_on_normal_stream() {
        // 正常流(text/tool 内容 + stop_reason + message_stop 俱全):零记录
        let mut w = TailWatch::new();
        w.observe(b"event: message_start\ndata: {\"type\":\"message_start\"}\n\n");
        w.observe(b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n");
        w.observe(b"data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{}}\n\n");
        w.observe(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
        assert_eq!(w.finish(), None);
        // 纯 tool_use 轮次(agent 函数调用)同样正常
        let mut w = TailWatch::new();
        w.observe(b"data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"tool_use\",\"name\":\"bash\"}}\n\n");
        w.observe(
            b"data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
        );
        w.observe(b"data: {\"type\":\"message_stop\"}\n\n");
        assert_eq!(w.finish(), None);
    }

    #[test]
    fn tail_watch_flags_budget_eaten_empty() {
        // 预算被思考耗尽:仅 thinking_delta,stop_reason=max_tokens——分类器空响应签名
        let mut w = TailWatch::new();
        w.observe(b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"...\"}}\n\n");
        w.observe(
            b"data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"}}\n\n",
        );
        w.observe(b"data: {\"type\":\"message_stop\"}\n\n");
        let line = w.finish().unwrap();
        assert!(line.contains("text=N"));
        assert!(line.contains("think=Y"));
        assert!(line.contains("stop=max_tokens"));
        assert!(line.contains("end=ok"));
    }

    #[test]
    fn tail_watch_flags_premature_end_and_ring_buffer() {
        // 裸 JSON 错误行 + 流静默终止:无 stop_reason 无 message_stop(SSE 解析器
        // 按规范忽略无前缀行,客户端只见流结束零内容)
        let mut w = TailWatch::new();
        w.observe(b"{\"error\":{\"type\":\"forbidden\",\"code\":\"1301\"}}\n");
        let line = w.finish().unwrap();
        assert!(line.contains("stop=-"));
        assert!(line.contains("end=trunc"));
        // 有部分内容但流截断(未收 message_delta):部分输出后断流,同样可疑
        let mut w = TailWatch::new();
        w.observe(b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n\n");
        let line = w.finish().unwrap();
        assert!(line.contains("text=Y"));
        assert!(line.contains("stop=-"));
        assert!(line.contains("end=trunc"));
        // 环形缓冲:超容量内容后尾部事件仍可提取(长思考流的 stop_reason 不丢)
        let mut w = TailWatch::new();
        let big = vec![b'x'; TAIL_CAP * 2];
        w.observe(&big);
        w.observe(b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n");
        w.observe(
            b"data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
        );
        w.observe(b"data: {\"type\":\"message_stop\"}\n\n");
        assert!(w.text);
        assert_eq!(w.finish(), None);
        // 标记子串恰好跨 chunk 边界切开也不漏检(残余拼接)
        let mut w = TailWatch::new();
        w.observe(b"prefix \"text_de");
        w.observe(b"lta\" suffix");
        assert!(w.text);
    }

    // ─── GuardStream(提交时机 + 异常注入)──────────────────

    /// 完整 Anthropic SSE 流的内容帧(text + stop_reason + message_stop 俱全)
    const FULL_STREAM: &[&[u8]] = &[
        b"event: message_start\ndata: {\"type\":\"message_start\"}\n\n",
        b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        b"data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{}}\n\n",
        b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];

    fn guard_commit(g: &Arc<Gateway>, idx: usize, session: Option<u64>) -> GuardCommit {
        GuardCommit {
            g: Arc::clone(g),
            provider_id: "zhipu".to_string(),
            idx,
            session,
        }
    }

    async fn drain<S, E>(mut s: S) -> (Vec<Bytes>, bool)
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
        E: std::fmt::Display,
    {
        let mut out = Vec::new();
        let mut saw_err = false;
        while let Some(item) = s.next().await {
            match item {
                Ok(b) => out.push(b),
                Err(_) => saw_err = true,
            }
        }
        (out, saw_err)
    }

    #[tokio::test]
    async fn guard_stream_commits_on_complete_stream() {
        // 完整流:字节原样透传、无注入、无错误,流结束即提交粘性与亲和
        let g = Arc::new(Gateway::bare());
        let inner = futures_util::stream::iter(
            FULL_STREAM
                .iter()
                .map(|c| Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(c))),
        );
        let guard = GuardStream::new(
            inner,
            guard_commit(&g, 1, Some(42)),
            Some(("glm-5.3".into(), "zhipu".into())),
        );
        let (out, saw_err) = drain(guard).await;
        assert!(!saw_err);
        assert_eq!(out.len(), FULL_STREAM.len(), "全部字节透传");
        assert!(!out.iter().any(|b| contains_bytes(b, b"event: error")));
        assert!(g.affinity_has(42));
        assert_eq!(g.last_good_of("zhipu"), Some(1));
    }

    #[tokio::test]
    async fn guard_stream_withholds_and_injects_on_empty_stream() {
        // 零内容流(GLM 无视 disabled 耗尽预算等):不提交 + 尾部注入协议原生 error 事件
        let g = Arc::new(Gateway::bare());
        let inner = futures_util::stream::iter(vec![Ok::<Bytes, std::io::Error>(Bytes::from(
            "data: {\"type\":\"message_start\"}\n\n".to_string(),
        ))]);
        let guard = GuardStream::new(
            inner,
            guard_commit(&g, 1, Some(42)),
            Some(("glm-5.3".into(), "zhipu".into())),
        );
        let (out, saw_err) = drain(guard).await;
        assert!(!saw_err, "注入事件后干净收尾,不透出传输错误");
        let last = out.last().unwrap();
        assert!(contains_bytes(last, b"event: error"));
        assert!(contains_bytes(last, b"upstream stream incomplete"));
        assert!(!g.affinity_has(42), "异常流不提交亲和");
        assert_eq!(g.last_good_of("zhipu"), None);
    }

    #[tokio::test]
    async fn guard_stream_withholds_and_injects_on_truncated_stream() {
        // 部分内容后断流(有 text 无 stop/end):同样注入 + 不提交
        let g = Arc::new(Gateway::bare());
        let inner = futures_util::stream::iter(vec![Ok::<Bytes, std::io::Error>(Bytes::from(
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n\n"
                .to_string(),
        ))]);
        let guard = GuardStream::new(
            inner,
            guard_commit(&g, 0, None),
            Some(("glm-5.3".into(), "zhipu".into())),
        );
        let (out, saw_err) = drain(guard).await;
        assert!(!saw_err);
        assert!(contains_bytes(out.last().unwrap(), b"event: error"));
        assert_eq!(g.last_good_of("zhipu"), None);
    }

    #[tokio::test]
    async fn guard_stream_swallows_tail_error_after_complete_content() {
        // 内容完整后连接才断(message_stop 已到,Err 只是连接尾部噪声):吞错转干净
        // EOF 并照常提交——传输层错误不代表应答失败
        let g = Arc::new(Gateway::bare());
        let mut items: Vec<std::io::Result<Bytes>> = FULL_STREAM
            .iter()
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        items.push(Err(std::io::Error::other("connection reset")));
        let guard = GuardStream::new(
            futures_util::stream::iter(items),
            guard_commit(&g, 2, Some(7)),
            Some(("glm-5.3".into(), "zhipu".into())),
        );
        let (out, saw_err) = drain(guard).await;
        assert!(!saw_err, "哨兵在场时传输错误一律吞掉");
        assert_eq!(out.len(), FULL_STREAM.len());
        assert!(g.affinity_has(7));
    }

    #[tokio::test]
    async fn guard_stream_stays_terminated_after_error() {
        // 病态 inner(Err 后重复 Err):终止态由守护层自持,后续 poll 恒 None——
        // 不随 inner 摇摆重复透出错误(真实 reqwest 流 Err 后的 poll 行为无合约)
        let g = Arc::new(Gateway::bare());
        let mut polls = 0u32;
        let inner = futures_util::stream::poll_fn(move |_cx| {
            polls += 1;
            Poll::Ready(Some(Err::<Bytes, std::io::Error>(std::io::Error::other(
                format!("poll {polls}"),
            ))))
        });
        let mut guard = GuardStream::new(inner, guard_commit(&g, 0, None), None);
        let first = guard.next().await;
        assert!(first.is_some_and(|r| r.is_err()), "首个 Err 原样透出");
        assert!(guard.next().await.is_none(), "终止后恒 None");
        assert!(guard.next().await.is_none());
        // 哨兵场景:注入事件冲出后恒 None,不重复注入不透 Err
        let inner = futures_util::stream::poll_fn(|_cx| {
            Poll::Ready(Some(Err::<Bytes, std::io::Error>(std::io::Error::other(
                "broken",
            ))))
        });
        let mut guard = GuardStream::new(
            inner,
            guard_commit(&g, 0, None),
            Some(("glm-5.3".into(), "zhipu".into())),
        );
        let (out, saw_err) = drain(&mut guard).await;
        assert!(!saw_err);
        assert!(contains_bytes(out.last().unwrap(), b"event: error"));
    }

    #[tokio::test]
    async fn guard_stream_err_without_sentinel_withholds_only() {
        // 无哨兵(非 Anthropic 面 / 非流式):传输错误即截断——不提交、不注入(无法
        // 构造彼协议错误事件),错误原样透出让客户端由断连感知
        let g = Arc::new(Gateway::bare());
        let inner = futures_util::stream::iter(vec![
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b"{\"id\":\"x\"}")),
            Err(std::io::Error::other("broken pipe")),
        ]);
        let guard = GuardStream::new(inner, guard_commit(&g, 0, Some(9)), None);
        let (out, saw_err) = drain(guard).await;
        assert!(saw_err, "无哨兵的截断原样透出");
        assert_eq!(out.len(), 1);
        assert!(!out.iter().any(|b| contains_bytes(b, b"event: error")));
        assert!(!g.affinity_has(9));
        // 同流干净 EOF = 完整应答,照常提交
        let inner = futures_util::stream::iter(vec![Ok::<Bytes, std::io::Error>(
            Bytes::from_static(b"{\"id\":\"x\"}"),
        )]);
        let guard = GuardStream::new(inner, guard_commit(&g, 0, Some(10)), None);
        drain(guard).await;
        assert!(g.affinity_has(10));
    }
}
