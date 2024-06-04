use flexi_logger::writers::LogWriter;
use flexi_logger::Duplicate::Debug;
use flexi_logger::{DeferredNow, Logger};
use log::{debug, info, Record};

pub struct CustomWriter;

impl LogWriter for CustomWriter {
    fn write(&self, _now: &mut DeferredNow, record: &Record) -> std::io::Result<()> {
        println!("[{}]", record.args().to_string());
        Ok(())
    }
    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn max_log_level(&self) -> log::LevelFilter {
        log::LevelFilter::Info
    }
}

fn main() {
    let logger = Logger::try_with_str("info").unwrap();
    logger
        .log_to_writer(Box::new(CustomWriter))
        .duplicate_to_stdout(Debug)
        .start()
        .unwrap();
    info!("infsssssssssso");
    debug!("debssssssssssug");
}
