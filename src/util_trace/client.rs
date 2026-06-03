//! 客户端内核：有界 mpsc + 后台攒批 POST。`enqueue` 零阻塞，后台任务独占网络 IO。

use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use trace_model::{IngestRequest, SpanRecord};

use super::config::TraceConfig;

static DROPPED: AtomicU64 = AtomicU64::new(0);

/// 因队列满 / 未初始化而被丢弃的 span 累计数（客户端自身可观测）。
pub fn dropped_count() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

pub struct TraceClient {
    tx: mpsc::Sender<SpanRecord>,
    service_name: String,
    body_limit: usize,
    enabled: bool,
}

impl TraceClient {
    /// 建通道并（enabled 时）拉起后台导出任务。须在 tokio 运行时内调用。
    pub fn spawn(cfg: TraceConfig) -> Self {
        let (tx, rx) = mpsc::channel(cfg.max_queue.max(1));
        let client = Self {
            tx,
            service_name: cfg.service_name.clone(),
            body_limit: cfg.body_limit,
            enabled: cfg.enabled,
        };
        if cfg.enabled {
            tokio::spawn(exporter(rx, cfg));
        }
        client
    }

    pub fn service(&self) -> &str {
        &self.service_name
    }

    /// 非阻塞入队：截断超限 body 后 `try_send`，满 / 关闭则丢弃 + 计数。
    pub fn enqueue(&self, mut rec: SpanRecord) {
        if !self.enabled {
            return;
        }
        truncate(&mut rec.request_body, self.body_limit, &mut rec.body_truncated);
        truncate(&mut rec.response_body, self.body_limit, &mut rec.body_truncated);
        if self.tx.try_send(rec).is_err() {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// 后台任务：阻塞 recv 一条后排干当前可得的（至多 batch 条）一次性 POST。
/// 失败仅 debug 日志后丢弃——trace-hub 不可用绝不影响业务。
async fn exporter(mut rx: mpsc::Receiver<SpanRecord>, cfg: TraceConfig) {
    let client = reqwest::Client::builder()
        .timeout(cfg.timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    while let Some(first) = rx.recv().await {
        let mut batch = vec![first];
        while batch.len() < cfg.batch {
            match rx.try_recv() {
                Ok(r) => batch.push(r),
                Err(_) => break,
            }
        }
        let req = IngestRequest { spans: batch };
        if let Err(e) = client.post(&cfg.endpoint).json(&req).send().await {
            log::debug!("trace-hub push failed: {e}");
        }
    }
}

/// 按字节上限截断，保证不切断 UTF-8 边界；截断时置位 `truncated`。
fn truncate(body: &mut Option<String>, limit: usize, truncated: &mut bool) {
    if let Some(b) = body {
        if b.len() > limit {
            let mut end = limit;
            while end > 0 && !b.is_char_boundary(end) {
                end -= 1;
            }
            b.truncate(end);
            *truncated = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_utf8_boundary_and_flags() {
        let mut body = Some("中文很长".repeat(10)); // 每个汉字 3 字节
        let mut flag = false;
        truncate(&mut body, 7, &mut flag);
        assert!(flag);
        let s = body.unwrap();
        assert!(s.len() <= 7);
        // 截断点落在字符边界（重建不报错）
        assert!(s.chars().all(|c| c == '中' || c == '文' || c == '很' || c == '长'));
    }

    #[test]
    fn truncate_noop_under_limit() {
        let mut body = Some("abc".to_string());
        let mut flag = false;
        truncate(&mut body, 100, &mut flag);
        assert!(!flag);
        assert_eq!(body.as_deref(), Some("abc"));
    }
}
