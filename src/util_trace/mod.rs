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
mod model;

use std::sync::OnceLock;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

pub use client::dropped_count;
pub use config::TraceConfig;
pub use model::{IngestRequest, SpanLink, SpanRecord, SpanStatus, TraceContext};
// SpanScope 已通过 `pub struct` 暴露；这里把它一并集中在公开 use 列表，方便 IDE
// 跳转和文档生成。

use client::TraceClient;

static CLIENT: OnceLock<TraceClient> = OnceLock::new();

/// 初始化全局客户端（在 tokio 运行时内调用一次；重复调用忽略后续）。
pub fn init(cfg: TraceConfig) {
    let _ = CLIENT.set(TraceClient::spawn(cfg));
}

/// 追踪是否已启用（已 init）。用于决定要不要注入 traceparent 等副作用。
pub fn enabled() -> bool {
    CLIENT.get().is_some()
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

/// **两阶段 span emit 辅助**——把"操作开始时落 request、操作结束时补 response"
/// 这套模式标准化，让所有项目（zero / nova-agent / alarm-server / asr-server /
/// orchestrator …）共用同一份正确实现。
///
/// 设计目的：
/// - **请求出现就有数据**：[`emit_start`](SpanScope::emit_start) 会立即推送一个
///   `state=in_flight`、`end_ms=start_ms`、含 `request_body` 的占位 span。
///   trace-hub UI 在操作还未完成时也能显示输入。
/// - **崩溃也保留请求**：如果工具/服务跑到一半进程挂掉，request 已经落库，
///   后续排查不丢线索。
/// - **同 span_id 自动合并**：[`emit_end`](SpanScope::emit_end) 用同一个 span_id
///   再推送一次（带 response_body + 真实 end_ms + 终态 status），trace-hub
///   `INSERT OR REPLACE` 会用新版覆盖，对查询方而言仍是一个 span。
///
/// # 例
/// ```ignore
/// let scope = SpanScope::new(ctx, "tool_call")
///     .with_flow_name("alarm")
///     .with_summary(serde_json::json!({"tool": "OrchestrateTask"}))
///     .with_request_body(serde_json::to_string(&input).unwrap());
/// scope.emit_start();          // Phase 1: in_flight + request
/// let result = run_tool().await;
/// scope.emit_end(                // Phase 2: response + final status
///     Some(result.output),
///     if result.is_error { SpanStatus::Error("...".into()) } else { SpanStatus::Ok },
///     None,                      // 可选：合并补充 summary 字段
/// );
/// ```
///
/// 简短的"一次性 emit"场景（不需要 in-flight 可见性）直接用 [`record_span`]，
/// 不要因为方便引入 SpanScope。
#[must_use = "SpanScope 必须显式 emit_start / emit_end，否则 span 不会落库"]
pub struct SpanScope {
    ctx: TraceContext,
    service: String,
    kind: String,
    flow_name: Option<String>,
    start_ms: i64,
    summary: serde_json::Value,
    detail: serde_json::Value,
    request_body: Option<String>,
    request_truncated: bool,
}

impl SpanScope {
    /// 构造一个 scope（不立即 emit）。`start_ms` 取构造时刻。
    pub fn new(ctx: TraceContext, kind: impl Into<String>) -> Self {
        Self {
            ctx,
            service: String::new(),
            kind: kind.into(),
            flow_name: None,
            start_ms: now_ms(),
            summary: serde_json::Value::Null,
            detail: serde_json::Value::Null,
            request_body: None,
            request_truncated: false,
        }
    }

    /// service 字段——一般留空让 [`record_span`] 用 init 时的服务名填。仅在跨服务
    /// 代理场景（如代理 emit 别人的 span）才覆盖。
    pub fn with_service(mut self, s: impl Into<String>) -> Self {
        self.service = s.into();
        self
    }

    /// flow_name：UI 节点头展示的主标识（如 skill 名、闹钟名）。
    pub fn with_flow_name(mut self, n: impl Into<String>) -> Self {
        self.flow_name = Some(n.into());
        self
    }

    /// 整个 summary 对象（一般是个 json object）。会和 phase 自动补的
    /// `state` 字段合并；冲突时调用方提供的字段保留。
    pub fn with_summary(mut self, s: serde_json::Value) -> Self {
        self.summary = s;
        self
    }

    /// detail JSON（点开节点详情后展开）。
    pub fn with_detail(mut self, d: serde_json::Value) -> Self {
        self.detail = d;
        self
    }

    /// 请求体（出站请求 JSON / 入参原文）。会被 trace-hub 服务端按
    /// body_limit 截断；这里也可以预先截断并打 [`with_request_truncated(true)`].
    pub fn with_request_body(mut self, body: impl Into<String>) -> Self {
        self.request_body = Some(body.into());
        self
    }

    /// 标记 request_body 是否被调用方主动截断（前端会显示提示）。
    pub fn with_request_truncated(mut self, truncated: bool) -> Self {
        self.request_truncated = truncated;
        self
    }

    /// Phase 1：emit "in flight" 占位 span。可选——只想用一次性 emit（结束时一把
    /// 落库）也可以跳过本调用直接 emit_end。
    pub fn emit_start(&self) {
        let mut summary = self.summary.clone();
        merge_state(&mut summary, "in_flight");
        record_span(SpanRecord {
            trace_id: self.ctx.trace_id.clone(),
            span_id: self.ctx.span_id.clone(),
            parent_span_id: self.ctx.parent_span_id.clone(),
            service: self.service.clone(),
            kind: self.kind.clone(),
            flow_name: self.flow_name.clone(),
            start_ms: self.start_ms,
            end_ms: self.start_ms, // in_flight：先 0ms
            status: SpanStatus::Ok,
            summary,
            detail: self.detail.clone(),
            request_body: self.request_body.clone(),
            response_body: None,
            body_truncated: self.request_truncated,
            links: Vec::new(),
        });
    }

    /// Phase 2：emit 最终 span（真实 end_ms + status + response_body）。
    /// `extra_summary` 是 None 时保留 `with_summary` 配的；Some 时合并补充。
    pub fn emit_end(
        self,
        response_body: Option<String>,
        status: SpanStatus,
        extra_summary: Option<serde_json::Value>,
    ) {
        self.emit_end_inner(response_body, status, extra_summary, false, Vec::new());
    }

    /// 同 [`emit_end`]，但额外控制 body_truncated 标志 + 可附加 SpanLink。
    pub fn emit_end_full(
        self,
        response_body: Option<String>,
        status: SpanStatus,
        extra_summary: Option<serde_json::Value>,
        body_truncated: bool,
        links: Vec<SpanLink>,
    ) {
        self.emit_end_inner(response_body, status, extra_summary, body_truncated, links);
    }

    fn emit_end_inner(
        self,
        response_body: Option<String>,
        status: SpanStatus,
        extra_summary: Option<serde_json::Value>,
        body_truncated: bool,
        links: Vec<SpanLink>,
    ) {
        let end_ms = now_ms();
        let mut summary = self.summary;
        if let Some(extra) = extra_summary {
            merge_object(&mut summary, extra);
        }
        merge_field(&mut summary, "dur_ms", serde_json::json!(end_ms - self.start_ms));
        record_span(SpanRecord {
            trace_id: self.ctx.trace_id,
            span_id: self.ctx.span_id,
            parent_span_id: self.ctx.parent_span_id,
            service: self.service,
            kind: self.kind,
            flow_name: self.flow_name,
            start_ms: self.start_ms,
            end_ms,
            status,
            summary,
            detail: self.detail,
            request_body: self.request_body,
            response_body,
            body_truncated: body_truncated || self.request_truncated,
            links,
        });
    }
}

fn ensure_object(v: &mut serde_json::Value) -> &mut serde_json::Map<String, serde_json::Value> {
    if !v.is_object() {
        *v = serde_json::Value::Object(serde_json::Map::new());
    }
    v.as_object_mut().expect("just ensured object")
}

fn merge_state(summary: &mut serde_json::Value, state: &str) {
    let obj = ensure_object(summary);
    obj.entry("state".to_string())
        .or_insert_with(|| serde_json::Value::String(state.to_string()));
}

fn merge_field(summary: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    let obj = ensure_object(summary);
    obj.insert(key.to_string(), value);
}

fn merge_object(summary: &mut serde_json::Value, extra: serde_json::Value) {
    if let Some(extra_obj) = extra.as_object() {
        let obj = ensure_object(summary);
        for (k, v) in extra_obj {
            obj.insert(k.clone(), v.clone());
        }
    }
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
