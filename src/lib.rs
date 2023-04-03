mod util_args;
#[cfg(any(feature = "daemon-sync", feature = "daemon-async"))]
mod util_daemon;
#[cfg(feature = "logger")]
mod util_logger;
#[cfg(feature = "tls")]
mod util_tls;
#[cfg(feature = "tls-util")]
mod util_tls_util;
mod util_txrx;

pub use util_txrx::*;

pub mod args {
    pub use crate::util_args::*;
}

#[cfg(feature = "logger")]
pub mod logger {
    pub use crate::util_logger::{
        custom_build, logger_feature, logger_feature_with_path, logger_stdout, logger_stdout_debug,
    };
    pub use flexi_logger::*;
    pub use log::{debug, error, info, trace};
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
