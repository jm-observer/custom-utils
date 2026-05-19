//! Unified Linux deployment description.
//!
//! [`LinuxService`] is the single source of truth a CLI crate configures once;
//! it then derives a self-consistent [`UpdateConfig`], [`ServiceConfig`] and
//! workspace path so self-update, systemd install, the watchdog heartbeat and
//! config-path resolution all agree on one layout:
//!
//! - binaries under `~/.local/bin` (user-writable, so self-update needs no root)
//! - workspace / config under `~/.config/<app>` (what [`crate::args::workspace`]
//!   resolves at runtime)
//! - a rootless `systemctl --user` service owned by the current login user
//!
//! Install, run and self-update never need root.

use anyhow::Result;
use std::path::PathBuf;

use super::{ServiceConfig, UpdateConfig, UpdateOutcome};

/// A deploy subcommand the host CLI can embed as a pass-through variant
/// (`MyCmd::Deploy(DeployCommand)`) and forward to [`LinuxService::dispatch`],
/// or obtain from argv via [`LinuxService::parse_deploy`].
#[derive(Debug, Clone)]
pub enum DeployCommand {
    /// Install the binaries + user unit (or render it with `dry_run`).
    Install {
        dry_run: bool,
        /// `-w/--workspace` override, applied to both install and the unit.
        workspace: Option<String>,
    },
    /// Self-update from the latest GitHub release.
    Update { force: bool },
    /// Report the configured version.
    Version,
    /// Report the deploy-subcommand usage.
    Help,
}

/// Outcome of [`LinuxService::dispatch`] / [`LinuxService::handle_cli`].
///
/// The library performs no stdout/exit side effects: text outcomes are handed
/// back for the caller to print (and compose with its own usage).
#[derive(Debug)]
pub enum CliAction {
    /// A deploy subcommand ran to completion (logged via `log`); the caller
    /// should exit.
    Handled,
    /// `install --dry-run`: the rendered unit file for the caller to print.
    DryRun(String),
    /// `--version`: the configured version string for the caller to print.
    Version(String),
    /// `--help`: the deploy-subcommand usage for the caller to print
    /// (typically alongside its own help).
    Help(String),
    /// No deploy subcommand matched: the caller should run the service. The
    /// resolved workspace (honoring any `-w/--workspace`) is provided so the
    /// caller doesn't recompute it.
    Run { workspace: std::path::PathBuf },
}

/// One description, four consistent capabilities (update / install / workspace /
/// watchdog) for a Linux CLI deployment.
///
/// ```
/// let svc = custom_utils::updater::LinuxService::new(
///         "alarm-server", "jm-observer", "alarm", "0.1.0")
///     .extra_bins(["alarm-cli"])
///     .user("alarm")
///     .user_home("/home/alarm")
///     .watchdog_sec(30);
/// let unit = svc.service_config().generate_unit().unwrap();
/// assert!(unit.contains("ExecStart=/home/alarm/.local/bin/alarm-server -w /home/alarm/.config/alarm-server"));
/// assert!(unit.contains("Type=notify"));
/// ```
#[derive(Debug, Clone)]
pub struct LinuxService {
    /// Application name; the systemd unit name and the `~/.config/<app>` segment.
    app: String,
    repo_owner: String,
    repo_name: String,
    version: String,
    bin_name: Option<String>,
    extra_bins: Vec<String>,
    user: Option<String>,
    user_home: Option<PathBuf>,
    /// Explicit workspace path (e.g. from a `--workspace` arg); overrides the
    /// `~/.config/<app>` default. Tilde / `./` are expanded.
    arg_workspace: Option<String>,
    description: Option<String>,
    exec_args: String,
    restart_sec: u32,
    watchdog_sec: Option<u32>,
}

impl LinuxService {
    pub fn new(
        app: impl Into<String>,
        repo_owner: impl Into<String>,
        repo_name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            app: app.into(),
            repo_owner: repo_owner.into(),
            repo_name: repo_name.into(),
            version: version.into(),
            bin_name: None,
            extra_bins: Vec::new(),
            user: None,
            user_home: None,
            arg_workspace: None,
            description: None,
            // The whole point of the unified workspace is to hand it to the
            // service; default to passing it via `-w`.
            exec_args: "-w {workspace}".to_string(),
            restart_sec: 5,
            watchdog_sec: None,
        }
    }

    /// Primary binary name (defaults to `app`).
    pub fn bin_name(mut self, name: impl Into<String>) -> Self {
        self.bin_name = Some(name.into());
        self
    }

    /// Sibling binaries shipped in the same release / installed alongside.
    pub fn extra_bins<I, S>(mut self, bins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_bins = bins.into_iter().map(Into::into).collect();
        self
    }

    /// Login user that owns the service (defaults to the current user).
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Home directory of the service user (defaults to the resolved value).
    pub fn user_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.user_home = Some(home.into());
        self
    }

    /// Explicit workspace path (overrides `~/.config/<app>`); `~` / `./` expand.
    pub fn workspace_arg(mut self, path: impl Into<String>) -> Self {
        self.arg_workspace = Some(path.into());
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// `ExecStart` argument string; `{workspace}` is substituted at render time.
    /// Defaults to `-w {workspace}`.
    pub fn exec_args(mut self, args: impl Into<String>) -> Self {
        self.exec_args = args.into();
        self
    }

    pub fn restart_sec(mut self, secs: u32) -> Self {
        self.restart_sec = secs;
        self
    }

    /// Enable the systemd watchdog (`Type=notify` + `WatchdogSec=<secs>`), so a
    /// spawned [`Self::spawn_watchdog`] heartbeat actually keeps the service up.
    pub fn watchdog_sec(mut self, secs: u32) -> Self {
        self.watchdog_sec = Some(secs);
        self
    }

    /// The systemd service description derived from this deployment.
    pub fn service_config(&self) -> ServiceConfig {
        let mut sc = ServiceConfig::new(&self.app)
            .binaries(self.all_bins())
            .exec_args(&self.exec_args)
            .restart_sec(self.restart_sec);
        if let Some(d) = &self.description {
            sc = sc.description(d);
        }
        if let Some(u) = &self.user {
            sc = sc.user(u);
        }
        if let Some(h) = &self.user_home {
            sc = sc.user_home(h.clone());
        }
        if let Some(secs) = self.watchdog_sec {
            sc = sc.watchdog_sec(secs);
        }
        if let Some(ws) = &self.arg_workspace {
            // Best-effort expansion; fall back to the raw value if it fails so
            // `service_config()` stays infallible.
            let expanded = crate::util_args::expand_path(ws).unwrap_or_else(|_| PathBuf::from(ws));
            sc = sc.workspace(expanded);
        }
        sc
    }

    /// The self-update description, targeting `~/.local/bin` so updates land
    /// where the installed service runs (no root needed).
    pub fn update_config(&self) -> Result<UpdateConfig> {
        let bin_dir = self.service_config().bin_dir_path()?;
        let cfg = UpdateConfig::new(&self.repo_owner, &self.repo_name, &self.version)
            .bin_name(self.primary_bin())
            .extra_bins(self.extra_bins.clone())
            .install_dir(bin_dir);
        Ok(cfg)
    }

    /// The resolved workspace / config directory — what [`crate::args::workspace`]
    /// resolves to at runtime for this service.
    pub fn workspace(&self) -> Result<PathBuf> {
        self.service_config().workspace_path()
    }

    /// The resolved binary install directory (`~/.local/bin` by default).
    pub fn bin_dir(&self) -> Result<PathBuf> {
        self.service_config().bin_dir_path()
    }

    /// Fetch the latest release and update in place if newer (or `force`).
    pub async fn self_update(&self, force: bool) -> Result<UpdateOutcome> {
        self.update_config()?.force(force).execute().await
    }

    /// Install binaries + the user-level systemd unit (rootless).
    pub fn install(&self) -> Result<()> {
        self.service_config().install()
    }

    /// Parse argv into a [`DeployCommand`] using the lightweight `util_args`
    /// parser (no clap). `None` => not a deploy command; the host owns it.
    ///
    /// Precedence: `--version`/`-V` > `--help`/`-h` > `install` > `update`.
    /// `install` captures `--dry-run`/`-n` and `-w`/`--workspace <path>`;
    /// `update` captures `--force`/`-f`.
    pub fn parse_deploy(&self) -> Option<DeployCommand> {
        if crate::util_args::exist_arg("--version", "-V") {
            return Some(DeployCommand::Version);
        }
        if crate::util_args::exist_arg("--help", "-h") {
            return Some(DeployCommand::Help);
        }
        match crate::util_args::command().as_deref() {
            Some("install") => Some(DeployCommand::Install {
                dry_run: crate::util_args::exist_arg("--dry-run", "-n"),
                workspace: crate::util_args::arg_value("--workspace", "-w"),
            }),
            Some("update") => Some(DeployCommand::Update {
                force: crate::util_args::exist_arg("--force", "-f"),
            }),
            _ => None,
        }
    }

    /// Execute a [`DeployCommand`] (e.g. forwarded from a host CLI's
    /// pass-through variant). No stdout/exit side effects: `install`/`update`
    /// log via `log`; `--dry-run`/`--version`/`--help` return text to print.
    pub async fn dispatch(&self, cmd: DeployCommand) -> Result<CliAction> {
        match cmd {
            DeployCommand::Install { dry_run, workspace } => {
                let mut svc = self.clone();
                if let Some(w) = workspace {
                    svc.arg_workspace = Some(w);
                }
                if dry_run {
                    Ok(CliAction::DryRun(svc.service_config().generate_unit()?))
                } else {
                    svc.install()?;
                    Ok(CliAction::Handled)
                }
            }
            DeployCommand::Update { force } => {
                let outcome = self.self_update(force).await?;
                log::info!("update: {outcome:?}");
                Ok(CliAction::Handled)
            }
            DeployCommand::Version => Ok(CliAction::Version(self.version.clone())),
            DeployCommand::Help => Ok(CliAction::Help(self.deploy_usage())),
        }
    }

    /// The deploy-subcommand usage block (the host typically prints this next
    /// to its own help).
    pub fn deploy_usage(&self) -> String {
        let bin = self.primary_bin();
        format!(
            "Deploy subcommands:\n  \
             {bin} install [--dry-run|-n] [--workspace|-w <path>]   install user systemd service (rootless)\n  \
             {bin} update  [--force|-f]                             self-update from GitHub release\n  \
             {bin} --version | -V                                   print version\n  \
             {bin} --help    | -h                                   print this help"
        )
    }

    /// Optional zero-config sugar: [`Self::parse_deploy`] + [`Self::dispatch`],
    /// falling back to [`CliAction::Run`] (with `-w/--workspace` honored) when
    /// argv carries no deploy command.
    ///
    /// ```no_run
    /// # async fn run(_: std::path::PathBuf) -> anyhow::Result<()> { Ok(()) }
    /// # async fn m() -> anyhow::Result<()> {
    /// use custom_utils::updater::{LinuxService, CliAction};
    /// let svc = LinuxService::new("alarm-server", "jm-observer", "alarm", "0.1.0");
    /// match svc.handle_cli().await? {
    ///     CliAction::Run { workspace } => run(workspace).await?,
    ///     CliAction::DryRun(t) | CliAction::Version(t) | CliAction::Help(t) => println!("{t}"),
    ///     CliAction::Handled => {}
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn handle_cli(&self) -> Result<CliAction> {
        match self.parse_deploy() {
            Some(cmd) => self.dispatch(cmd).await,
            None => {
                let mut svc = self.clone();
                if let Some(w) = crate::util_args::arg_value("--workspace", "-w") {
                    svc.arg_workspace = Some(w);
                }
                Ok(CliAction::Run {
                    workspace: svc.workspace()?,
                })
            }
        }
    }

    /// Spawn the systemd watchdog heartbeat. No-op unless built on Linux with
    /// the `prod` feature; pair with [`Self::watchdog_sec`] so the unit declares
    /// `WatchdogSec=`.
    pub fn spawn_watchdog(&self) -> tokio::task::JoinHandle<()> {
        crate::util_daemon::daemon()
    }

    fn primary_bin(&self) -> String {
        self.bin_name.clone().unwrap_or_else(|| self.app.clone())
    }

    fn all_bins(&self) -> Vec<String> {
        let mut bins = Vec::with_capacity(1 + self.extra_bins.len());
        bins.push(self.primary_bin());
        for b in &self.extra_bins {
            if !bins.contains(b) {
                bins.push(b.clone());
            }
        }
        bins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> LinuxService {
        LinuxService::new("alarm-server", "jm-observer", "alarm", "0.1.0")
            .extra_bins(["alarm-cli"])
            .user("alarm")
            .user_home("/home/alarm")
    }

    #[test]
    fn service_config_uses_local_bin_and_config_workspace() {
        let unit = svc().service_config().generate_unit().unwrap();
        assert!(unit.contains("ExecStart=/home/alarm/.local/bin/alarm-server -w /home/alarm/.config/alarm-server"));
        assert!(unit.contains("WorkingDirectory=/home/alarm/.config/alarm-server"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(!unit.contains("User="));
    }

    #[test]
    fn watchdog_propagates_to_unit() {
        let unit = svc().watchdog_sec(30).service_config().generate_unit().unwrap();
        assert!(unit.contains("Type=notify"));
        assert!(unit.contains("WatchdogSec=30"));
    }

    #[test]
    fn update_targets_local_bin() {
        let bin_dir = svc().bin_dir().unwrap();
        assert_eq!(bin_dir, PathBuf::from("/home/alarm/.local/bin"));
        // primary + extra, primary derived from app name.
        assert_eq!(
            svc().all_bins(),
            vec!["alarm-server".to_string(), "alarm-cli".to_string()]
        );
    }

    #[test]
    fn explicit_workspace_arg_overrides_default() {
        let ws = svc().workspace_arg("/var/lib/alarm").workspace().unwrap();
        assert_eq!(ws, PathBuf::from("/var/lib/alarm"));
    }
}
