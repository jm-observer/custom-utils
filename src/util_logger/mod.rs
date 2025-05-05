use crate::util_logger::builder::{DebugLevel, LoggerBuilder, LoggerFeatureBuilder};
use builder::{simple_colored_with_thread_target, LoggerBuilder2};
use flexi_logger::{LogSpecBuilder, LogSpecification, Logger, LoggerHandle, WriteMode};
use log::LevelFilter;
use std::fs;
use std::path::PathBuf;

mod builder;

/// 简单，纯粹想输出日志而已。适用于临时
/// 控制台输出日志
/// logger_stdout("info,custom_utils=warn")
pub fn logger_stdout(level: impl Into<DebugLevel>) {
    let specification = match level.into() {
        DebugLevel::Filter(debug_level) => {
            let mut log_spec_builder = LogSpecBuilder::new();
            log_spec_builder.default(debug_level);
            log_spec_builder.build()
        }
        DebugLevel::Env(default) => LogSpecification::env_or_parse(default).unwrap(),
    };
    Box::leak(Box::new(
        Logger::with(specification)
            .format(simple_colored_with_thread_target)
            .write_mode(WriteMode::Direct)
            .log_to_stdout()
            .start()
            .unwrap(),
    ));
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

///
/// let _ = custom_utils::logger::logger_feature("lapce", "warn,wgpu_core=error,lapce_app::keypress::loader=info", log::LevelFilter::Info, true)
///         .build();
/// let _ = custom_utils::logger::logger_feature("lapce", log::LevelFilter::Debug, log::LevelFilter::Info, true)
///         .build();
///
/// 根据feature来确定日志输出
///     log_etc_reset 配置文件每次重启都重置
///     dev：控制台输出
///     prod：在目录{user_home}/log/{app}输出日志；
///         每天或大小达到10m更换日志文件；
///         维持10个日志文件；
///         生成{user_home}/etc/{app}/logspecification.toml的动态配置文件
pub fn logger_feature(
    app: &str,
    debug_level: impl Into<DebugLevel>,
    prod_level: LevelFilter,
    log_etc_reset: bool,
) -> LoggerFeatureBuilder {
    let home = home::home_dir().unwrap();
    let log_etc_path: PathBuf = home.join("etc");
    if !log_etc_path.exists() {
        std::fs::create_dir_all(&log_etc_path);
    }
    let log_path: PathBuf = home.join("log");
    if !log_path.exists() {
        std::fs::create_dir_all(&log_path);
    }
    logger_feature_with_path(
        app,
        debug_level,
        prod_level,
        log_etc_path,
        log_etc_reset,
        log_path,
    )
}

/// log_etc_reset 配置文件每次重启都重置
pub fn logger_feature_with_path(
    app: &str,
    debug_level: impl Into<DebugLevel>,
    prod_level: LevelFilter,
    log_etc_path: PathBuf,
    log_etc_reset: bool,
    log_path: PathBuf,
) -> LoggerFeatureBuilder {
    if log_etc_reset && log_etc_path.exists() && log_etc_path.is_file() {
        fs::remove_file(log_etc_path.clone()).unwrap();
    }
    LoggerFeatureBuilder::default(app, debug_level.into(), prod_level, log_etc_path, log_path)
}
