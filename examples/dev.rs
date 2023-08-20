use custom_utils::logger;
use log::LevelFilter::{Debug, Info};
use log::{debug, error, info, warn};

#[tokio::main]
async fn main() {
    logger::logger_feature_with_path("dev", Debug, Info, "./log".into(), "./log".into())
        .module("custom_utils", Debug)
        .build();
    println!("{}", format_args!("[{:8}]", "DEBUG"));
    debug!("abc");
    info!("abc");
    warn!("warn");
    error!("error");
    // logger::custom_build(Debug)
    //     .module("custom_utils", Debug)
    //     .build_default()
    //     .log_to_stdout()
    //     ._start()
    //     .unwrap();
    //
    // debug!("abc");
    // info!("abc");
    // warn!("warn");
    // error!("error");
}
