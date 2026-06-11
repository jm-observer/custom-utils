//! 演示 `LoggerFeatureBuilder::extra_file` 多文件路由。
//!
//! 运行：`cargo run -p example-logger`（默认启用 `prod` 特性，多文件才会落盘）
//!
//! 预期：在 `~/log/` 下生成
//!   - example_logger_*.log     —— 主文件，吃所有未定向的日志
//!   - example_logger-audit_*.log    —— 仅 `target: "{audit}"`
//!   - example_logger-metrics_*.log  —— 仅 `target: "{metrics}"`

use custom_utils::logger::flexi_logger::LevelFilter;
use custom_utils::logger::log::*;
use custom_utils::logger::*;
use std::path::PathBuf;

fn main() {
    let app = "example_logger";
    let home: PathBuf = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .expect("HOME / USERPROFILE 未设置");
    let log_dir = home.join("log");
    println!("日志目录: {}", log_dir.display());

    let _handle = logger_feature(app, "info", LevelFilter::Info, true)
        .extra_file("audit", format!("{app}-audit"))
        .extra_file("metrics", format!("{app}-metrics"))
        .build();

    info!("主文件：普通业务日志");
    warn!("主文件：警告");

    info!(target: "{audit}", "审计：user=42 action=login");
    info!(target: "{audit}", "审计：user=42 action=logout");

    info!(target: "{metrics}", "指标：qps=128 p99=42ms");
    info!(target: "{metrics}", "指标：qps=130 p99=40ms");

    error!("主文件：再来一条 error，确认未被定向 target 污染");
}
