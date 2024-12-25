use ansi_term::{Color, Style};
use anyhow::Result;
use flexi_logger::writers::LogWriter;
use flexi_logger::{Age, Duplicate, LogSpecification};
use flexi_logger::{Cleanup, Criterion, FileSpec, Naming};
use flexi_logger::{
    DeferredNow, FormatFunction, LevelFilter, LogSpecBuilder, Logger, LoggerHandle, Record,
    WriteMode,
};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::thread;
// #[cfg(feature = "prod")]
const TS_DASHES_BLANK_COLONS_DOT_BLANK: &str = "%m-%d %H:%M:%S%.3f";

#[allow(dead_code)]
fn with_thread(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), std::io::Error> {
    let level = record.level();
    write!(
        w,
        "[{}][{}][{:5}][{}:{}] {}",
        now.format(TS_DASHES_BLANK_COLONS_DOT_BLANK),
        thread::current().name().unwrap_or("<unnamed>"),
        level.to_string(),
        record.target(),
        record.line().unwrap_or(0),
        &record.args()
    )
}
#[allow(dead_code)]
pub fn colored_with_thread(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), std::io::Error> {
    let level = record.level();
    write!(
        w,
        "{}",
        format_args!(
            "[{}][{}][{:5}][{}:{}] {}",
            now.format(TS_DASHES_BLANK_COLONS_DOT_BLANK),
            thread::current().name().unwrap_or("<unnamed>"),
            style(level).paint(format_args!("{:6}", level.to_string()).to_string()),
            record.target(),
            record.line().unwrap_or(0),
            &record.args()
        )
    )
}

#[allow(dead_code)]
pub fn colored_with_thread_target(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), std::io::Error> {
    let level = record.level();
    write!(
        w,
        "{}",
        format_args!(
            "[{}][{}][{:5}][{}:{}] {}",
            now.format(TS_DASHES_BLANK_COLONS_DOT_BLANK),
            thread::current().name().unwrap_or("<unnamed>"),
            style(level).paint(format_args!("{:6}", level.to_string()).to_string()),
            record.target(),
            record.line().unwrap_or(0),
            &record.args()
        )
    )
}

pub struct LoggerBuilder {
    display_target: bool,
    log_spec_builder: LogSpecBuilder,
}
impl LoggerBuilder {
    pub fn default(level: LevelFilter) -> Self {
        let mut log_spec_builder = LogSpecBuilder::new();
        log_spec_builder.default(level);
        Self {
            log_spec_builder,
            display_target: false,
        }
    }
    pub fn module<M: AsRef<str>>(mut self, module_name: M, lf: LevelFilter) -> Self {
        self.log_spec_builder.module(module_name, lf);
        self
    }
    pub fn build_default(self) -> LoggerBuilder2 {
        LoggerBuilder2 {
            logger: Logger::with(self.log_spec_builder.build())
                .format(if self.display_target {
                    colored_with_thread_target
                } else {
                    colored_with_thread
                })
                .write_mode(WriteMode::Direct),
        }
    }

    pub fn log_to_stdout(self) {
        Logger::with(self.log_spec_builder.build())
            .format(if self.display_target {
                colored_with_thread_target
            } else {
                colored_with_thread
            })
            .write_mode(WriteMode::Direct)
            .log_to_stdout()
            .start()
            .unwrap();
    }

    pub fn build_with(self, format: FormatFunction, write_mode: WriteMode) -> LoggerBuilder2 {
        LoggerBuilder2 {
            logger: Logger::with(self.log_spec_builder.build())
                .format(format)
                .write_mode(write_mode),
        }
    }
}
pub struct LoggerBuilder2 {
    logger: Logger,
}
pub struct LoggerBuilder3 {
    logger: Logger,
}
impl LoggerBuilder3 {
    #[must_use]
    pub fn start(self) -> LoggerHandle {
        self.logger.start().unwrap()
    }
    pub fn _start(self) -> Result<LoggerHandle> {
        Ok(self.logger.start()?)
    }

    #[must_use]
    pub fn start_with_specfile(self, p: impl AsRef<Path>) -> LoggerHandle {
        self.logger.start_with_specfile(p).unwrap()
    }
    #[must_use]
    pub fn start_with_specfile_default(self, app: &str) -> LoggerHandle {
        let path = PathBuf::from_str("/var/local/etc/")
            .unwrap()
            .join(app)
            .join("logspecification.toml");
        self.logger.start_with_specfile(path).unwrap()
    }
}
impl LoggerBuilder2 {
    pub fn log_to_stdout(self) -> LoggerBuilder3 {
        LoggerBuilder3 {
            logger: self.logger.log_to_stdout(),
        }
    }
    pub fn log_to_file_default(self, app: &str) -> LoggerBuilder3 {
        let fs_path = PathBuf::from_str("/var/local/log").unwrap().join(app);
        let fs = FileSpec::default()
            .directory(fs_path)
            .basename(app)
            .suffix("log");
        // 若为true，则会覆盖rotate中的数字、keep^
        self.log_to_file(
            fs,
            Criterion::AgeOrSize(Age::Day, 10_000_000),
            Naming::Numbers,
            Cleanup::KeepLogFiles(10),
            true,
        )
    }
    pub fn log_to_writer(self, w: Box<dyn LogWriter>) -> LoggerBuilder3 {
        LoggerBuilder3 {
            logger: self.logger.log_to_writer(w),
        }
    }
    pub fn log_to_file(
        self,
        fs: FileSpec,
        criterion: Criterion,
        naming: Naming,
        cleanup: Cleanup,
        append: bool,
    ) -> LoggerBuilder3 {
        LoggerBuilder3 {
            logger: self
                .logger
                .log_to_file(fs)
                .o_append(append)
                .rotate(criterion, naming, cleanup),
        }
    }
}
#[allow(dead_code)]
pub struct LoggerFeatureBuilder {
    _app: String,
    _debug_level: DebugLevel,
    _prod_level: LevelFilter,
    fs: FileSpec,
    criterion: Criterion,
    naming: Naming,
    cleanup: Cleanup,
    append: bool,
    modules: Vec<(String, LevelFilter)>,
    writer: Option<Box<dyn LogWriter>>,
    log_etc_path: PathBuf,
}
impl LoggerFeatureBuilder {
    pub fn default(
        app: &str,
        _debug_level: DebugLevel,
        prod_level: LevelFilter,
        log_etc_path: PathBuf,
        log_path: PathBuf,
    ) -> Self {
        // let fs_path = PathBuf::from_str("/var/local/log").unwrap().join(app);
        let fs = FileSpec::default()
            .directory(log_path)
            .basename(app)
            .suffix("log");
        // 若为true，则会覆盖rotate中的数字、keep^
        let criterion = Criterion::AgeOrSize(Age::Day, 10_000_000);
        let naming = Naming::Numbers;
        let cleanup = Cleanup::KeepLogFiles(10);
        let append = true;
        Self {
            _app: app.to_string(),
            _debug_level,
            _prod_level: prod_level,
            fs,
            criterion,
            naming,
            cleanup,
            append,
            modules: Vec::new(),
            writer: None,
            log_etc_path,
        }
    }
    pub fn module<M: AsRef<str>>(mut self, module_name: M, lf: LevelFilter) -> Self {
        self.modules.push((module_name.as_ref().to_owned(), lf));
        self
    }
    pub fn log_to_write(mut self, w: Box<dyn LogWriter>) -> Self {
        self.writer = Some(w);
        self
    }
    pub fn config(
        mut self,
        fs: FileSpec,
        criterion: Criterion,
        naming: Naming,
        cleanup: Cleanup,
        append: bool,
    ) -> Self {
        self.fs = fs;
        self.criterion = criterion;
        self.naming = naming;
        self.cleanup = cleanup;
        self.append = append;
        self
    }
    #[cfg(feature = "prod")]
    #[must_use]
    pub fn build(self) -> LoggerHandle {
        let mut log_spec_builder = LogSpecBuilder::new();
        log_spec_builder.default(self._prod_level);
        for (module, level) in self.modules {
            log_spec_builder.module(module, level);
        }
        let path = self.log_etc_path.join("logspecification.toml");
        if let Some(w) = self.writer {
            Logger::with(log_spec_builder.build())
                .format(with_thread)
                .write_mode(WriteMode::Direct)
                .log_to_file_and_writer(self.fs, w)
                .o_append(self.append)
                .rotate(self.criterion, self.naming, self.cleanup)
                .start_with_specfile(path)
                .unwrap()
        } else {
            Logger::with(log_spec_builder.build())
                .format(with_thread)
                .write_mode(WriteMode::Direct)
                .log_to_file(self.fs)
                .o_append(self.append)
                .rotate(self.criterion, self.naming, self.cleanup)
                .start_with_specfile(path)
                .unwrap()
        }
    }
    #[cfg(not(feature = "prod"))]
    #[must_use]
    pub fn build(self) -> LoggerHandle {
        let specification = match self._debug_level {
            DebugLevel::Filter(debug_level) => {
                let mut log_spec_builder = LogSpecBuilder::new();
                log_spec_builder.default(debug_level);
                for (module, level) in self.modules {
                    log_spec_builder.module(module, level);
                }
                log_spec_builder.build()
            }
            DebugLevel::Env(default) => {
                LogSpecification::env_or_parse(default).unwrap()
            }
        };
        if let Some(w) = self.writer {
            LoggerBuilder2 {
                logger: Logger::with(specification)
                    .format(colored_with_thread)
                    .write_mode(WriteMode::Direct)
                    .duplicate_to_stdout(Duplicate::All),
            }
            .log_to_writer(w)
            .start()
        } else {
            LoggerBuilder2 {
                logger: Logger::with(specification)
                    .format(colored_with_thread)
                    .write_mode(WriteMode::Direct),
            }
            .log_to_stdout()
            .start()
        }
    }
}

lazy_static::lazy_static! {
    static ref MY_PALETTE: std::sync::RwLock<Palette> = std::sync::RwLock::new(Palette::default());
}
pub fn style(level: log::Level) -> Style {
    let palette = &*(MY_PALETTE.read().unwrap());
    match level {
        log::Level::Error => palette.error,
        log::Level::Warn => palette.warn,
        log::Level::Info => palette.info,
        log::Level::Debug => palette.debug,
        log::Level::Trace => palette.trace,
    }
}

#[derive(Debug)]
struct Palette {
    pub error: Style,
    pub warn: Style,
    pub info: Style,
    pub debug: Style,
    pub trace: Style,
}
impl Palette {
    fn default() -> Palette {
        Palette {
            error: Style::default().fg(Color::Red).bold(),
            warn: Style::default().fg(Color::Yellow).bold(),
            info: Style::default(),
            debug: Style::default().fg(Color::Fixed(28)),
            trace: Style::default().fg(Color::Fixed(8)),
        }
    }
}

pub enum  DebugLevel {
    Filter(LevelFilter),
    Env(String)
}

impl From<LevelFilter> for DebugLevel {
    fn from(value: LevelFilter) -> Self {
        Self::Filter(value)
    }
}
impl From<&str> for DebugLevel {
    fn from(value: &str) -> Self {
        Self::Env(value.to_string())
    }
}
