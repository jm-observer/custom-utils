//! trace 客户端配置。

use std::time::Duration;

/// 客户端配置。`new()` 给出合理默认，按需链式覆盖。
#[derive(Clone, Debug)]
pub struct TraceConfig {
    /// trace-hub ingest 地址，如 `http://g10:9100/v1/spans`。
    pub endpoint: String,
    /// 本服务名（"zero" | "alarm-server" | "douyin" ...），自动填入未指定 service 的 span。
    pub service_name: String,
    /// 有界队列容量；满则丢弃 + 计数（绝不阻塞）。
    pub max_queue: usize,
    /// 单个 body 字节上限，超出截断 + 标记。
    pub body_limit: usize,
    /// 单次 POST 的最大攒批条数。
    pub batch: usize,
    /// 单次 POST 超时。
    pub timeout: Duration,
    /// 总开关；false 时 record_* 全部 no-op、不起后台任务。
    pub enabled: bool,
}

impl TraceConfig {
    pub fn new(endpoint: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: service_name.into(),
            max_queue: 1024,
            body_limit: 1_000_000,
            batch: 64,
            timeout: Duration::from_secs(3),
            enabled: true,
        }
    }

    /// 关闭追踪（record_* 变 no-op）。
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}
