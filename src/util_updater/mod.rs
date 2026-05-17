//! Deployment helpers for CLI crates: GitHub self-update and systemd install.

mod systemd;
mod update;

pub use systemd::ServiceConfig;
pub use update::{UpdateConfig, UpdateOutcome};
