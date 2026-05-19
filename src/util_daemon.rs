// `updater` bundles the async watchdog, so the deploy stack is one feature.
// Sync is only used when explicitly requested *without* async/updater, so the
// glob re-exports below never collide.
#[cfg(any(feature = "daemon-async", feature = "updater"))]
mod async_deamon;
#[cfg(all(feature = "daemon-sync", not(any(feature = "daemon-async", feature = "updater"))))]
mod sync_deamon;

#[cfg(any(feature = "daemon-async", feature = "updater"))]
pub use async_deamon::*;
#[cfg(all(feature = "daemon-sync", not(any(feature = "daemon-async", feature = "updater"))))]
pub use sync_deamon::*;
