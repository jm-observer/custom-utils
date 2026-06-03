//! 路线 B trace 客户端：把 span(+body) 异步、非阻塞地推送给 trace-hub。
//!
//! 用法：
//! 1. 进程启动（tokio 运行时内）调用 [`init`] 一次。
//! 2. 出站请求前 [`inject_traceparent`] 写头；入站 [`extract_traceparent`] 读头。
//! 3. 关键节点用 [`record_span`] / [`record_llm_call`] 记录（仅入队，零阻塞）。
//!
//! 铁律：trace-hub 不可用 / 队列满 → 丢弃 + 计数，绝不阻塞或影响业务。

mod client;
mod config;

use std::sync::OnceLock;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

pub use client::dropped_count;
pub use config::TraceConfig;
pub use trace_model::{SpanLink, SpanRecord, SpanStatus, TraceContext};

use client::TraceClient;

static CLIENT: OnceLock<TraceClient> = OnceLock::new();

/// 初始化全局客户端（在 tokio 运行时内调用一次；重复调用忽略后续）。
pub fn init(cfg: TraceConfig) {
    let _ = CLIENT.set(TraceClient::spawn(cfg));
}

/// 记录一个 span（仅入队，非阻塞）。未 init / 禁用 / 队列满时静默丢弃。
/// `service` 为空时自动填入 init 时的服务名。
pub fn record_span(mut rec: SpanRecord) {
    if let Some(c) = CLIENT.get() {
        if rec.service.is_empty() {
            rec.service = c.service().to_string();
        }
        c.enqueue(rec);
    }
}

/// 便捷：一次 LLM 调用。kind=`llm_call`，概要含 model/耗时，两个 body 进详情。
pub struct LlmCall {
    pub ctx: TraceContext,
    pub model: String,
    pub request_body: String,
    pub response_body: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub status: SpanStatus,
}

/// 记录一次 LLM 调用。
pub fn record_llm_call(call: LlmCall) {
    let rec = SpanRecord {
        trace_id: call.ctx.trace_id,
        span_id: call.ctx.span_id,
        parent_span_id: call.ctx.parent_span_id,
        service: String::new(),
        kind: "llm_call".to_string(),
        flow_name: None,
        start_ms: call.start_ms,
        end_ms: call.end_ms,
        status: call.status,
        summary: serde_json::json!({
            "model": call.model,
            "dur_ms": (call.end_ms - call.start_ms).max(0),
        }),
        detail: serde_json::Value::Null,
        request_body: Some(call.request_body),
        response_body: Some(call.response_body),
        body_truncated: false,
        links: Vec::new(),
    };
    record_span(rec);
}

/// 出站注入 `traceparent` 头。
pub fn inject_traceparent(ctx: &TraceContext, headers: &mut HeaderMap) {
    if let Ok(v) = HeaderValue::from_str(&ctx.to_traceparent()) {
        headers.insert(HeaderName::from_static("traceparent"), v);
    }
}

/// 入站解析 `traceparent`（框架无关：传一个按名取头的闭包）。
/// 返回的上下文代表**远端当前 span**，本地应随后 `.child()` 出本地 span。
pub fn extract_traceparent<F>(get_header: F) -> Option<TraceContext>
where
    F: Fn(&str) -> Option<String>,
{
    get_header("traceparent")
        .as_deref()
        .and_then(TraceContext::from_traceparent)
}

/// 当前毫秒时间戳（便捷，供 start_ms/end_ms 使用）。
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_before_init_is_noop() {
        // 未 init 时 record_span 不应 panic、静默丢弃。
        record_span(SpanRecord {
            trace_id: "a".repeat(32),
            span_id: "b".repeat(16),
            parent_span_id: None,
            service: String::new(),
            kind: "test".into(),
            flow_name: None,
            start_ms: 1,
            end_ms: 2,
            status: SpanStatus::Ok,
            summary: serde_json::Value::Null,
            detail: serde_json::Value::Null,
            request_body: None,
            response_body: None,
            body_truncated: false,
            links: vec![],
        });
    }

    #[test]
    fn traceparent_inject_extract_round_trip() {
        let ctx = TraceContext::root();
        let mut headers = HeaderMap::new();
        inject_traceparent(&ctx, &mut headers);

        let got = extract_traceparent(|name| headers.get(name).and_then(|v| v.to_str().ok()).map(|s| s.to_string()))
            .expect("extract");
        assert_eq!(got.trace_id, ctx.trace_id);
        assert_eq!(got.span_id, ctx.span_id);

        // 本地子 span 的父应指向远端 span。
        let local = got.child();
        assert_eq!(local.parent_span_id.as_deref(), Some(ctx.span_id.as_str()));
    }

    #[test]
    fn now_ms_is_positive() {
        assert!(now_ms() > 0);
    }
}
