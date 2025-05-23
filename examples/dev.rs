use custom_utils::logger;
use log::LevelFilter::Info;
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
