use crate::util_logger::builder::{LoggerBuilder, LoggerFeatureBuilder};
use log::LevelFilter;
use std::fs;
use std::path::PathBuf;

mod builder;

/// 简单，纯粹想输出日志而已。适用于临时
/// 控制台输出日志
pub fn logger_stdout(lever: LevelFilter) -> LoggerBuilder {
    LoggerBuilder::default(lever)
}
pub fn logger_stdout_debug() {
    let _res = LoggerBuilder::default(LevelFilter::Debug)
        .build_default()
        .log_to_stdout()
        ._start();
}
pub fn logger_stdout_info() {
    let _res = LoggerBuilder::default(LevelFilter::Info)
        .build_default()
        .log_to_stdout()
        ._start();
}
/// 根据feature来确定日志输出
///     dev：控制台输出
///     prod：在目录/var/local/log/{app}输出日志；
///         每天或大小达到10m更换日志文件；
///         维持10个日志文件；
///         生成/var/local/etc/{app}/logspecification.toml的动态配置文件
pub fn logger_feature(
    app: &str,
    debug_level: LevelFilter,
    prod_level: LevelFilter,
) -> LoggerFeatureBuilder {
    let log_etc_path: PathBuf = "/var/local/etc".into();
    let log_path: PathBuf = "/var/local/log".into();
    logger_feature_with_path(app, debug_level, prod_level, log_etc_path, true, log_path)
}

/// log_etc_reset 配置文件每次重启都重置
pub fn logger_feature_with_path(
    app: &str,
    debug_level: LevelFilter,
    prod_level: LevelFilter,
    log_etc_path: PathBuf,
    log_etc_reset: bool,
    log_path: PathBuf,
) -> LoggerFeatureBuilder {
    if log_etc_reset && log_etc_path.exists() && log_etc_path.is_file() {
        fs::remove_file(log_etc_path.clone()).unwrap();
    }
    LoggerFeatureBuilder::default(app, debug_level, prod_level, log_etc_path, log_path)
}
