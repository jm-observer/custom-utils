use crate::daemon::daemon;
use custom_utils::*;
use log::LevelFilter::{Debug, Info};
use log::{debug, error, info, warn};

#[tokio::main]
async fn main() {
    let handle = daemon();
    if let Err(_e) = handle.await {}
    custom_utils::logger::logger_feature_with_path(
        "dev",
        Debug,
        Info,
        "./log".into(),
        "./log".into(),
    )
    .module("custom_utils", Debug)
    .build();
    debug!("abc");
    info!("abc");
    warn!("warn");
    error!("error");
    custom_utils::logger::custom_build(Debug)
        .module("custom_utils", Debug)
        .build_default()
        .log_to_stdout()
        ._start();

    debug!("abc");
    info!("abc");
    warn!("warn");
    error!("error");
}
