use custom_utils::logger::*;
use log::warn;
use log::LevelFilter::{Debug, Warn};

fn main() {
    let _ = logger_feature("a", Warn, Warn).build();
    debug!("warn");
}
