use custom_utils::logger::*;
use log::warn;
use log::LevelFilter::Debug;

fn main() {
    let _ = logger_feature("a", Debug, Debug).build();
    warn!("warn");
}
