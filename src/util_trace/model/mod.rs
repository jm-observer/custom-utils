//! trace 体系的共享数据契约（span/trace 数据结构 + W3C traceparent 传播）。
//!
//! 客户端（本 crate `util_trace`）与后端（`trace-hub`）共享同一套字段定义，
//! 作为**单一事实源**避免两侧漂移。原 `trace-model` 独立 crate 的内容已内联
//! 到此模块，以便 `custom-utils` 能干净发布到 crates.io（crates.io 禁止
//! git/path 依赖）。
//!
//! 核心概念：
//! - 一条完整生命周期 = 一棵树 = 同一个 `trace_id`。
//! - 主流程 / 子流程 / 节点都是 [`SpanRecord`]，靠 `span_id` / `parent_span_id`
//!   建树；有子节点的 span 即「（子）流程」，叶子即「节点」。
//! - 跨异步：一次性场景续用同 `trace_id`（[`TraceContext::continued`]）；
//!   周期场景新起 `trace_id` 并用 [`SpanLink`] 指回原 span。
//! - **信封固定、载荷开放**：信封字段供建树/排序/检索；`summary` / `detail`
//!   是开放 JSON，由各 `kind` 自定，后端不解析其内部结构。

mod context;
mod record;

pub use context::{gen_span_id, gen_trace_id, TraceContext};
pub use record::{IngestRequest, SpanLink, SpanRecord, SpanStatus};
