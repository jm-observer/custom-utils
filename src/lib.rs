#![allow(unused_imports, unused)]

mod util_args;
#[cfg(any(feature = "daemon-sync", feature = "daemon-async"))]
mod util_daemon;
#[cfg(feature = "logger")]
mod util_logger;
#[cfg(feature = "tls")]
mod util_tls;
#[cfg(feature = "tls-util")]
mod util_tls_util;

#[cfg(feature = "derive")]
mod util_derive;
#[cfg(feature = "derive")]
pub use util_derive::*;

pub mod args {
    pub use crate::util_args::*;
}

#[cfg(feature = "logger")]
pub mod logger {
    pub use crate::util_logger::{
        logger_feature, logger_feature_with_path, logger_stdout, logger_stdout_debug,
        logger_stdout_info,
    };
    pub mod log {
        pub use log::{debug, error, info, trace, warn};
    }
    pub mod flexi_logger {
        pub use flexi_logger::*;
    }
}

#[cfg(any(feature = "daemon-sync", feature = "daemon-async"))]
pub mod daemon {
    pub use crate::util_daemon::daemon;
}

#[cfg(feature = "tls")]
pub mod tls {
    pub use crate::util_tls::*;
}

#[cfg(feature = "tls-util")]
pub mod tls_util {
    pub use crate::util_tls_util::print::*;
    pub use crate::util_tls_util::*;
}

#[cfg(feature = "timer")]
pub mod timer {
    pub use timer_util::*;
}
