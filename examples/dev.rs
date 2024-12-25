use custom_utils::logger;
use custom_utils::logger::logger_stdout_debug;
use log::LevelFilter::{Debug, Info};
use log::{debug, error, info, warn};

#[tokio::main]
async fn main() {
    let _ = logger::logger_feature("dev", "error,custom_utils=warn", Info, false)
        .module("custom_utils", Info)
        .build();

    // logger::logger_stdout(Debug).log_to_stdout();
    debug!("abc");
    info!("abc");
    warn!("warn");
    error!("error");
}
