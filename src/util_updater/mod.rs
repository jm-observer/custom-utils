//! Deployment helpers for CLI crates: GitHub self-update and systemd install.
//!
//! [`LinuxService`] ties self-update, systemd install, the watchdog heartbeat
//! and workspace-path resolution to one consistent `~/.local/bin` +
//! `~/.config/<app>` layout.

mod linux_service;
mod systemd;
mod update;

pub use linux_service::{CliAction, DeployCommand, LinuxService};
pub use systemd::ServiceConfig;
pub use update::{UpdateConfig, UpdateOutcome};
