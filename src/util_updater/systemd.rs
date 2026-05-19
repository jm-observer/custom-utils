//! User-level systemd service installation.
//!
//! [`ServiceConfig::generate_unit`] renders a unit file on any platform (handy
//! for `--dry-run`). [`ServiceConfig::install`] performs a **rootless**
//! `systemctl --user` install and is only available on Linux with the
//! `updater` feature.
//!
//! Everything stays under the current login user: binaries in `~/.local/bin`,
//! working directory in `~/.config/<name>` (what [`crate::args::workspace`]
//! resolves at runtime), unit in `~/.config/systemd/user/<name>.service`.
//! Install, run and self-update never need root.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Describes a user-level systemd service to install for a CLI deployment.
///
/// ```
/// let unit = custom_utils::updater::ServiceConfig::new("alarm-server")
///     .description("Alarm Server")
///     .exec_args("-w {workspace}")
///     .binaries(["alarm-server", "alarm-cli"])
///     .user("alarm")
///     .user_home("/home/alarm")
///     .generate_unit()
///     .unwrap();
/// assert!(unit.contains("ExecStart=/home/alarm/.local/bin/alarm-server -w /home/alarm/.config/alarm-server"));
/// assert!(unit.contains("WorkingDirectory=/home/alarm/.config/alarm-server"));
/// assert!(unit.contains("WantedBy=default.target"));
/// // User-level units must not carry User=/Group=.
/// assert!(!unit.contains("User="));
/// ```
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    name: String,
    description: String,
    /// `ExecStart` argument string; `{workspace}` is substituted at render time.
    exec_args: String,
    /// Binaries copied into the binary directory during install.
    binaries: Vec<String>,
    /// Login user that owns the service. `None` => the current user. Used only
    /// to target `loginctl enable-linger` and to resolve the home directory.
    user: Option<String>,
    /// Home directory of `user`. `None` => the current user's home.
    user_home: Option<PathBuf>,
    /// Binary install directory. `None` => `<home>/.local/bin`.
    bin_dir: Option<PathBuf>,
    /// `WorkingDirectory`. `None` => `<home>/.config/<name>`.
    workspace: Option<PathBuf>,
    restart_sec: u32,
    /// Opt-in `WatchdogSec=`. The unit is `Type=notify` regardless; when this
    /// is set, a stalled [`crate::daemon::daemon`] heartbeat additionally gets
    /// the service killed and restarted by systemd.
    watchdog_sec: Option<u32>,
}

/// Concrete paths derived from a [`ServiceConfig`].
#[derive(Debug, Clone)]
struct Resolved {
    user: String,
    home: PathBuf,
    bin_dir: PathBuf,
    workspace: PathBuf,
}

impl ServiceConfig {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            description: format!("{name} service"),
            exec_args: String::new(),
            binaries: vec![name.clone()],
            user: None,
            user_home: None,
            bin_dir: None,
            workspace: None,
            restart_sec: 5,
            watchdog_sec: None,
            name,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn exec_args(mut self, args: impl Into<String>) -> Self {
        self.exec_args = args.into();
        self
    }

    pub fn binaries<I, S>(mut self, bins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.binaries = bins.into_iter().map(Into::into).collect();
        self
    }

    /// Override the login user (defaults to the current user). Only affects
    /// `loginctl enable-linger` targeting and home-directory resolution.
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Override the home directory of the service user (defaults to the
    /// current user's home).
    pub fn user_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.user_home = Some(home.into());
        self
    }

    /// Override the binary install directory (defaults to `<home>/.local/bin`).
    pub fn bin_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.bin_dir = Some(dir.into());
        self
    }

    /// Override the working directory (defaults to `<home>/.config/<name>`).
    pub fn workspace(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace = Some(path.into());
        self
    }

    pub fn restart_sec(mut self, secs: u32) -> Self {
        self.restart_sec = secs;
        self
    }

    /// Opt into watchdog supervision by adding `WatchdogSec=<secs>`. The unit
    /// is `Type=notify` by default; this only adds the liveness timer so a
    /// stalled [`crate::daemon::daemon`] heartbeat triggers a restart.
    pub fn watchdog_sec(mut self, secs: u32) -> Self {
        self.watchdog_sec = Some(secs);
        self
    }

    /// Resolve the login user, home, binary directory and workspace.
    fn resolve(&self) -> Result<Resolved> {
        let user = match &self.user {
            Some(u) => u.clone(),
            None => current_user()?,
        };
        let home = match &self.user_home {
            Some(h) => h.clone(),
            None => home::home_dir().unwrap_or_else(|| PathBuf::from(format!("/home/{user}"))),
        };
        let bin_dir = self.bin_dir.clone().unwrap_or_else(|| home.join(".local").join("bin"));
        let workspace = self
            .workspace
            .clone()
            .unwrap_or_else(|| home.join(".config").join(&self.name));
        Ok(Resolved {
            user,
            home,
            bin_dir,
            workspace,
        })
    }

    /// The resolved binary install directory (`<home>/.local/bin` by default).
    pub fn bin_dir_path(&self) -> Result<PathBuf> {
        Ok(self.resolve()?.bin_dir)
    }

    /// The resolved working directory (`<home>/.config/<name>` by default).
    pub fn workspace_path(&self) -> Result<PathBuf> {
        Ok(self.resolve()?.workspace)
    }

    /// Render the user-level systemd unit file contents.
    pub fn generate_unit(&self) -> Result<String> {
        let r = self.resolve()?;
        Ok(self.render_unit(&r))
    }

    fn render_unit(&self, r: &Resolved) -> String {
        // The unit is a Linux deployment artifact; emit POSIX separators even
        // when `generate_unit` is run on Windows for a dry-run preview.
        let workspace = posix(&r.workspace);
        let bin = posix(&r.bin_dir.join(&self.name));
        let exec_args = self.exec_args.replace("{workspace}", &workspace);
        let exec_start = if exec_args.is_empty() {
            bin
        } else {
            format!("{bin} {exec_args}")
        };
        // `Type=notify` is the default: the deploy stack always spawns the
        // readiness task ([`crate::daemon::daemon`]) which sends `READY=1`, so
        // systemd gets correct start ordering. `WatchdogSec=` is opt-in
        // (`watchdog_sec`): only then does a stalled heartbeat kill the service.
        let svc_type = "notify";
        let watchdog_line = match self.watchdog_sec {
            Some(secs) => format!("WatchdogSec={secs}\n"),
            None => String::new(),
        };
        // No User=/Group=: a `systemctl --user` unit always runs as the
        // owning user; `WantedBy=default.target` is the user-bus analogue of
        // `multi-user.target`.
        format!(
            "[Unit]\n\
             Description={description}\n\
             After=network.target\n\
             \n\
             [Service]\n\
             Type={svc_type}\n\
             ExecStart={exec_start}\n\
             Restart=on-failure\n\
             RestartSec={restart_sec}\n\
             {watchdog_line}\
             WorkingDirectory={workspace}\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            description = self.description,
            svc_type = svc_type,
            exec_start = exec_start,
            restart_sec = self.restart_sec,
            watchdog_line = watchdog_line,
            workspace = workspace,
        )
    }

    /// Install binaries into `~/.local/bin`, create the workspace, write the
    /// user unit and `systemctl --user enable` it. Fully rootless.
    ///
    /// `loginctl enable-linger` is attempted so the service survives logout /
    /// starts at boot; failure is logged, not fatal (it may need a one-time
    /// admin action on locked-down hosts).
    #[cfg(all(target_os = "linux", feature = "updater"))]
    pub fn install(&self) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let r = self.resolve()?;

        self.warn_if_installed_elsewhere(&r);

        let src_dir = std::env::current_exe()
            .context("Failed to resolve current executable")?
            .parent()
            .ok_or_else(|| anyhow!("Current executable has no parent directory"))?
            .to_path_buf();

        std::fs::create_dir_all(&r.bin_dir)
            .with_context(|| format!("Failed to create binary directory {}", r.bin_dir.display()))?;

        for bin in &self.binaries {
            let src = src_dir.join(bin);
            let dest = r.bin_dir.join(bin);
            if src != dest {
                std::fs::copy(&src, &dest)
                    .with_context(|| format!("Failed to copy {} -> {}", src.display(), dest.display()))?;
            }
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("Failed to chmod {}", dest.display()))?;
        }

        std::fs::create_dir_all(&r.workspace)
            .with_context(|| format!("Failed to create workspace {}", r.workspace.display()))?;

        let unit_dir = r.home.join(".config").join("systemd").join("user");
        std::fs::create_dir_all(&unit_dir).with_context(|| format!("Failed to create {}", unit_dir.display()))?;
        let unit_path = unit_dir.join(format!("{}.service", self.name));
        std::fs::write(&unit_path, self.render_unit(&r))
            .with_context(|| format!("Failed to write {}", unit_path.display()))?;

        run(Command::new("systemctl").args(["--user", "daemon-reload"]))
            .context("systemctl --user daemon-reload failed")?;
        run(Command::new("systemctl").args(["--user", "enable", &self.name]))
            .context("systemctl --user enable failed")?;

        // Linger lets the user manager (and thus the service) run without an
        // active session. Targeting one's own user usually works via polkit;
        // on locked-down hosts it may need an admin, so don't fail the install.
        if let Err(e) = run(Command::new("loginctl").args(["enable-linger", &r.user])) {
            log::warn!(
                "Could not enable linger for '{}' ({e}); run `loginctl enable-linger {}` \
                 as an admin so the service starts at boot",
                r.user,
                r.user
            );
        }

        log::info!("Installed user systemd service '{}'", self.name);
        return Ok(());

        fn run(cmd: &mut Command) -> Result<()> {
            let status = cmd.status().with_context(|| format!("Failed to spawn {cmd:?}"))?;
            if !status.success() {
                anyhow::bail!("command {cmd:?} exited with {status}");
            }
            Ok(())
        }
    }

    /// Stub used when systemd install is unavailable (non-Linux or feature off).
    #[cfg(not(all(target_os = "linux", feature = "updater")))]
    pub fn install(&self) -> Result<()> {
        anyhow::bail!(
            "systemd install requires Linux and the `updater` feature; \
             use generate_unit() for a dry-run preview"
        )
    }

    /// Warn (don't block) when the binary is already deployed elsewhere, so a
    /// user doesn't end up with a shadowed or conflicting second copy.
    #[cfg(all(target_os = "linux", feature = "updater"))]
    fn warn_if_installed_elsewhere(&self, r: &Resolved) {
        use std::path::Path;

        if let Some(other) = std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .filter(|dir| *dir != r.bin_dir)
                .map(|dir| dir.join(&self.name))
                .find(|cand| cand.is_file())
        }) {
            log::warn!(
                "'{}' is already installed at {} — after this install \
                 ~/.local/bin must precede it on PATH or the old copy will shadow the new one",
                self.name,
                other.display()
            );
        }

        let sys_unit = format!("/etc/systemd/system/{}.service", self.name);
        if Path::new(&sys_unit).exists() {
            log::warn!(
                "a system-level unit {sys_unit} already exists; this user-level service may \
                 conflict — consider `sudo systemctl disable --now {}` and removing it",
                self.name
            );
        }
    }
}

/// Render a path with POSIX (`/`) separators regardless of host OS.
fn posix(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// The current login user.
fn current_user() -> Result<String> {
    std::env::var("USER")
        .ok()
        .or_else(|| std::env::var("USERNAME").ok())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| anyhow!("Cannot determine the current user; set it explicitly via .user(..)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str) -> ServiceConfig {
        ServiceConfig::new(name).user("alarm").user_home("/home/alarm")
    }

    #[test]
    fn unit_substitutes_workspace_placeholder() {
        let unit = cfg("alarm-server")
            .description("Alarm Server")
            .exec_args("-w {workspace}")
            .restart_sec(10)
            .generate_unit()
            .unwrap();

        assert!(unit.contains("Description=Alarm Server"));
        assert!(unit.contains("ExecStart=/home/alarm/.local/bin/alarm-server -w /home/alarm/.config/alarm-server"));
        assert!(unit.contains("WorkingDirectory=/home/alarm/.config/alarm-server"));
        assert!(unit.contains("RestartSec=10"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn user_unit_has_no_user_or_group() {
        let unit = cfg("svc").generate_unit().unwrap();
        assert!(!unit.contains("User="));
        assert!(!unit.contains("Group="));
        assert!(!unit.contains("multi-user.target"));
    }

    #[test]
    fn unit_without_exec_args_omits_trailing_space() {
        let unit = cfg("svc").generate_unit().unwrap();
        assert!(unit.contains("ExecStart=/home/alarm/.local/bin/svc\n"));
    }

    #[test]
    fn explicit_overrides_win() {
        let unit = cfg("svc")
            .bin_dir("/opt/svc/bin")
            .workspace("/var/lib/svc")
            .exec_args("-w {workspace}")
            .generate_unit()
            .unwrap();
        assert!(unit.contains("ExecStart=/opt/svc/bin/svc -w /var/lib/svc"));
        assert!(unit.contains("WorkingDirectory=/var/lib/svc"));
    }

    #[test]
    fn default_is_type_notify_without_watchdogsec() {
        let unit = cfg("svc").generate_unit().unwrap();
        assert!(unit.contains("Type=notify"));
        assert!(!unit.contains("WatchdogSec="));
    }

    #[test]
    fn watchdog_sec_adds_watchdogsec_keeping_type_notify() {
        let unit = cfg("svc").watchdog_sec(30).generate_unit().unwrap();
        assert!(unit.contains("Type=notify"));
        assert!(unit.contains("WatchdogSec=30\n"));
    }

    #[test]
    fn defaults_derive_from_name_and_home() {
        let r = cfg("foo").resolve().unwrap();
        assert_eq!(r.user, "alarm");
        assert_eq!(r.home, PathBuf::from("/home/alarm"));
        assert_eq!(r.bin_dir, PathBuf::from("/home/alarm/.local/bin"));
        assert_eq!(r.workspace, PathBuf::from("/home/alarm/.config/foo"));
        assert_eq!(ServiceConfig::new("foo").binaries, vec!["foo".to_string()]);
    }
}
