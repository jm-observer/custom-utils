use std::thread;
use std::time::Duration;
use custom_utils::logger::*;
use log::LevelFilter::Warn;
use log::warn;

fn main() {
    let _logger = logger_feature("abc", Warn, Warn, false).build();

    loop {
        debug!("debug");
        info!("info");
        warn!("warn");
        error!("error");
        thread::sleep(Duration::from_secs(5));
    }
}
