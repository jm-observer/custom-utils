use custom_utils::logger::flexi_logger::LevelFilter::Warn;
use custom_utils::logger::log::*;
use custom_utils::logger::*;
use std::thread;
use std::time::Duration;

fn main() {
    let _logger = logger_feature("abc", "info", Warn, false).build();

    loop {
        debug!("debug");
        info!("info");
        warn!("warn");
        error!("error");
        thread::sleep(Duration::from_secs(5));
        if true {
            break;
        }
    }
}
