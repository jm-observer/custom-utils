//! systemd service installation.
//!
//! [`ServiceConfig::generate_unit`] renders a unit file on any platform (handy
//! for `--dry-run`). [`ServiceConfig::install`] performs the privileged install
//! and is only available on Linux with the `updater-systemd` feature.

use anyhow::Result;

/// Describes a systemd service to install for a CLI deployment.
///
/// ```
/// let unit = custom_utils::updater::ServiceConfig::new("alarm-server")
///     .description("Alarm Server")
///     .exec_args("-w {workspace}")
///     .binaries(["alarm-server", "alarm-cli"])
///     .workspace("/etc/alarm-server")
///     .generate_unit();
/// assert!(unit.contains("WorkingDirectory=/etc/alarm-server"));
/// ```
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    name: String,
    description: String,
    /// `ExecStart` argument string; `{workspace}` is substituted at render time.
    exec_args: String,
    /// Binaries copied into `/usr/local/bin` during install.
    binaries: Vec<String>,
    user: String,
    workspace: String,
    restart_sec: u32,
}

impl ServiceConfig {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            description: format!("{name} service"),
            exec_args: String::new(),
            binaries: vec![name.clone()],
            user: name.clone(),
            workspace: format!("/etc/{name}"),
            restart_sec: 5,
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

    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = user.into();
        self
    }

    pub fn workspace(mut self, path: impl Into<String>) -> Self {
        self.workspace = path.into();
        self
    }

    pub fn restart_sec(mut self, secs: u32) -> Self {
        self.restart_sec = secs;
        self
    }

    /// Render the systemd unit file contents.
    pub fn generate_unit(&self) -> String {
        let exec_args = self.exec_args.replace("{workspace}", &self.workspace);
        let exec_start = if exec_args.is_empty() {
            format!("/usr/local/bin/{}", self.name)
        } else {
            format!("/usr/local/bin/{} {}", self.name, exec_args)
        };
        format!(
            "[Unit]\n\
             Description={description}\n\
             After=network.target\n\
             \n\
             [Service]\n\
             Type=simple\n\
             User={user}\n\
             Group={user}\n\
             ExecStart={exec_start}\n\
             Restart=on-failure\n\
             RestartSec={restart_sec}\n\
             WorkingDirectory={workspace}\n\
             \n\
             [Install]\n\
             WantedBy=multi-user.target\n",
            description = self.description,
            user = self.user,
            exec_start = exec_start,
            restart_sec = self.restart_sec,
            workspace = self.workspace,
        )
    }

    /// Install binaries, create the service user/workspace, write the unit file
    /// and `systemctl enable` it. Requires root.
    #[cfg(all(target_os = "linux", feature = "updater-systemd"))]
    pub fn install(&self) -> Result<()> {
        use anyhow::{anyhow, bail, Context};
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;
        use std::process::Command;

        let uid = Command::new("id")
            .arg("-u")
            .output()
            .context("Failed to run `id -u` for root check")?;
        if String::from_utf8_lossy(&uid.stdout).trim() != "0" {
            bail!("install requires root; re-run with sudo");
        }

        let src_dir = std::env::current_exe()
            .context("Failed to resolve current executable")?
            .parent()
            .ok_or_else(|| anyhow!("Current executable has no parent directory"))?
            .to_path_buf();

        for bin in &self.binaries {
            let src = src_dir.join(bin);
            let dest = format!("/usr/local/bin/{bin}");
            std::fs::copy(&src, &dest).with_context(|| format!("Failed to copy {} -> {dest}", src.display()))?;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("Failed to chmod {dest}"))?;
        }

        if !user_exists(&self.user) {
            run(Command::new("useradd").args([
                "--system",
                "--no-create-home",
                "--shell",
                "/usr/sbin/nologin",
                &self.user,
            ]))
            .context("Failed to create system user")?;
        }

        std::fs::create_dir_all(&self.workspace)
            .with_context(|| format!("Failed to create workspace {}", self.workspace))?;
        run(Command::new("chown").args(["-R", &format!("{}:{}", self.user, self.user), &self.workspace]))
            .context("Failed to chown workspace")?;

        let unit_path = format!("/etc/systemd/system/{}.service", self.name);
        std::fs::write(&unit_path, self.generate_unit()).with_context(|| format!("Failed to write {unit_path}"))?;

        run(Command::new("systemctl").arg("daemon-reload")).context("systemctl daemon-reload failed")?;
        run(Command::new("systemctl").args(["enable", &self.name])).context("systemctl enable failed")?;

        log::info!("Installed systemd service '{}'", self.name);
        return Ok(());

        fn user_exists(user: &str) -> bool {
            Path::new("/etc/passwd")
                .canonicalize()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|s| s.lines().any(|l| l.split(':').next() == Some(user)))
                .unwrap_or(false)
        }

        fn run(cmd: &mut Command) -> Result<()> {
            let status = cmd.status().with_context(|| format!("Failed to spawn {cmd:?}"))?;
            if !status.success() {
                bail!("command {cmd:?} exited with {status}");
            }
            Ok(())
        }
    }

    /// Stub used when systemd install is unavailable (non-Linux or feature off).
    #[cfg(not(all(target_os = "linux", feature = "updater-systemd")))]
    pub fn install(&self) -> Result<()> {
        anyhow::bail!(
            "systemd install requires Linux and the `updater-systemd` feature; \
             use generate_unit() for a dry-run preview"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_substitutes_workspace_placeholder() {
        let unit = ServiceConfig::new("alarm-server")
            .description("Alarm Server")
            .exec_args("-w {workspace}")
            .workspace("/var/lib/alarm")
            .user("alarm")
            .restart_sec(10)
            .generate_unit();

        assert!(unit.contains("Description=Alarm Server"));
        assert!(unit.contains("ExecStart=/usr/local/bin/alarm-server -w /var/lib/alarm"));
        assert!(unit.contains("WorkingDirectory=/var/lib/alarm"));
        assert!(unit.contains("User=alarm"));
        assert!(unit.contains("RestartSec=10"));
        assert!(unit.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn unit_without_exec_args_omits_trailing_space() {
        let unit = ServiceConfig::new("svc").generate_unit();
        assert!(unit.contains("ExecStart=/usr/local/bin/svc\n"));
    }

    #[test]
    fn defaults_derive_from_name() {
        let cfg = ServiceConfig::new("foo");
        assert_eq!(cfg.user, "foo");
        assert_eq!(cfg.workspace, "/etc/foo");
        assert_eq!(cfg.binaries, vec!["foo".to_string()]);
    }
}
