#![allow(unused_imports, unused)]

mod util_args;
#[cfg(any(feature = "daemon-sync", feature = "daemon-async", feature = "updater"))]
mod util_daemon;
#[cfg(feature = "logger")]
mod util_logger;
#[cfg(feature = "tls")]
mod util_tls;
#[cfg(feature = "tls-util")]
mod util_tls_util;

#[cfg(feature = "derive")]
mod util_derive;
#[cfg(feature = "derive")]
pub use util_derive::*;

pub mod args {
    pub use crate::util_args::*;
}

#[cfg(feature = "logger")]
pub mod logger {
    pub use crate::util_logger::{
        logger_feature, logger_feature_with_path, logger_stdout, logger_stdout_debug, logger_stdout_info,
    };
    pub mod log {
        pub use log::{debug, error, info, trace, warn};
    }
    pub mod flexi_logger {
        pub use flexi_logger::*;
    }
}

#[cfg(any(feature = "daemon-sync", feature = "daemon-async", feature = "updater"))]
pub mod daemon {
    pub use crate::util_daemon::daemon;
}

#[cfg(feature = "tls")]
pub mod tls {
    pub use crate::util_tls::*;
}

#[cfg(feature = "tls-util")]
pub mod tls_util {
    pub use crate::util_tls_util::print::*;
    pub use crate::util_tls_util::*;
}

#[cfg(feature = "timer")]
pub mod timer {
    pub use timer_util::*;
}

#[cfg(feature = "updater")]
pub mod util_updater;

#[cfg(feature = "updater")]
pub mod updater {
    pub use crate::util_updater::*;
}

#[cfg(feature = "trace")]
mod util_trace;

#[cfg(feature = "trace")]
pub mod trace {
    pub use crate::util_trace::*;
}

/// 跨 crate 透传当前 turn 的 W3C `traceparent`。与 `trace` feature 一同启用——
/// 启用 trace 客户端的工作区里，下游 crate（nova-agent 等）通过此 task-local
/// 拿到外层透下来的 traceparent，进而注入工具子进程环境变量 / 出站 HTTP 头。
/// 未启用 trace 时整段缺席，零依赖、零运行时开销。
///
/// 用法：宿主（zero bridge-claw）在 `app.start_turn(...).await` 外层包一层
/// `trace_propagation::CURRENT_TRACEPARENT.scope(Some(tp), fut).await`；
/// 被嵌套调用的任意点（nova-agent ExternalCommandTool 等）用
/// `CURRENT_TRACEPARENT.try_with(|tp| tp.clone()).ok().flatten()` 取值。
///
/// tokio task-local 通过 `tokio::spawn` 自动继承到子任务（满足 nova 内部并发
/// 模型）；future 跨 await 点也会持有值。
#[cfg(feature = "trace")]
pub mod trace_propagation {
    tokio::task_local! {
        pub static CURRENT_TRACEPARENT: Option<String>;
    }
}
