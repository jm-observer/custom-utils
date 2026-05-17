//! GitHub Release self-update.
//!
//! Lets a CLI crate replace its own binary (and sibling binaries shipped in the
//! same release) with the latest published assets. Fully async; streams each
//! asset to disk and swaps it into place with a rollback-safe rename.

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use reqwest::Client;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const CLIENT_USER_AGENT: &str = concat!("custom-utils-updater/", env!("CARGO_PKG_VERSION"));

/// Self-update configuration, built fluently and run via [`UpdateConfig::execute`].
///
/// ```no_run
/// # async fn run() -> anyhow::Result<()> {
/// custom_utils::updater::UpdateConfig::new("jm-observer", "timer-util", env!("CARGO_PKG_VERSION"))
///     .bin_name("alarm-cli")
///     .extra_bins(["alarm-server"])
///     .force(false)
///     .execute()
///     .await?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    repo_owner: String,
    repo_name: String,
    /// Primary binary name. `None` => derived from the current executable.
    bin_name: Option<String>,
    /// Sibling binaries in the same directory to update from the same release.
    extra_bins: Vec<String>,
    current_version: String,
    force: bool,
    /// Override the auto-detected Rust target triple used for asset matching.
    target_triple: Option<String>,
}

/// Result of an [`UpdateConfig::execute`] run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// The latest release is not newer than `current`; nothing was changed.
    UpToDate { current: String, latest: String },
    /// Binaries were replaced on disk.
    Updated {
        from: String,
        to: String,
        bins: Vec<String>,
    },
}

impl UpdateConfig {
    pub fn new(
        repo_owner: impl Into<String>,
        repo_name: impl Into<String>,
        current_version: impl Into<String>,
    ) -> Self {
        Self {
            repo_owner: repo_owner.into(),
            repo_name: repo_name.into(),
            bin_name: None,
            extra_bins: Vec::new(),
            current_version: current_version.into(),
            force: false,
            target_triple: None,
        }
    }

    /// Override the primary binary name (defaults to the running executable).
    pub fn bin_name(mut self, name: impl Into<String>) -> Self {
        self.bin_name = Some(name.into());
        self
    }

    /// Additional binaries shipped in the same release, updated alongside the primary one.
    pub fn extra_bins<I, S>(mut self, bins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_bins = bins.into_iter().map(Into::into).collect();
        self
    }

    /// Update even when the latest release is not newer than the current version.
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Override the target triple used to match release assets.
    pub fn target_triple(mut self, target: impl Into<String>) -> Self {
        self.target_triple = Some(target.into());
        self
    }

    /// Fetch the latest release, and if it is newer (or `force`), download and
    /// swap the primary binary plus every `extra_bins` entry in place.
    pub async fn execute(&self) -> Result<UpdateOutcome> {
        let client = build_client()?;
        let target = match &self.target_triple {
            Some(t) => t.clone(),
            None => detect_target_triple()?.to_string(),
        };

        let exe = std::env::current_exe().context("Failed to resolve current executable path")?;
        let dir = exe
            .parent()
            .ok_or_else(|| anyhow!("Current executable has no parent directory"))?
            .to_path_buf();
        let primary = match &self.bin_name {
            Some(name) => name.clone(),
            None => exe
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("Current executable has no valid file name"))?
                .to_string(),
        };

        let api_url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.repo_owner, self.repo_name
        );
        let (tag, assets) = fetch_latest_release(&client, &api_url)
            .await
            .context("Failed to fetch latest release metadata")?;

        if !self.force && !is_newer(&tag, &self.current_version) {
            return Ok(UpdateOutcome::UpToDate {
                current: self.current_version.clone(),
                latest: tag,
            });
        }

        let mut bins = Vec::with_capacity(1 + self.extra_bins.len());
        bins.push(primary);
        for extra in &self.extra_bins {
            if !bins.contains(extra) {
                bins.push(extra.clone());
            }
        }

        for bin in &bins {
            let url = find_asset_url(&assets, bin, &target)
                .with_context(|| format!("No release asset for binary '{bin}' (target '{target}')"))?;
            let tmp = dir.join(format!("{bin}.update.tmp"));
            download_to(&client, &url, &tmp)
                .await
                .with_context(|| format!("Failed to download asset for '{bin}'"))?;
            let dest = dir.join(format!("{bin}{}", exe_suffix()));
            swap_in_place(&dest, &tmp).with_context(|| format!("Failed to install new binary for '{bin}'"))?;
            log::info!("Updated {} -> {tag}", dest.display());
        }

        Ok(UpdateOutcome::Updated {
            from: self.current_version.clone(),
            to: tag,
            bins,
        })
    }
}

fn build_client() -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
    // Authenticated requests get a higher GitHub rate limit and can read private repos.
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if let Ok(mut value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
    }
    Client::builder()
        .user_agent(CLIENT_USER_AGENT)
        .default_headers(headers)
        .build()
        .context("Failed to build reqwest client")
}

fn detect_target_triple() -> Result<&'static str> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => Ok("x86_64-pc-windows-msvc"),
        ("aarch64", "linux") => Ok("aarch64-unknown-linux-gnu"),
        ("x86_64", "linux") => Ok("x86_64-unknown-linux-gnu"),
        (arch, os) => Err(anyhow!("Unsupported platform: {arch}-{os}")),
    }
}

fn exe_suffix() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// Returns `(tag_name, assets_array)` from a GitHub release JSON document.
async fn fetch_latest_release(client: &Client, api_url: &str) -> Result<(String, Value)> {
    let json: Value = client
        .get(api_url)
        .send()
        .await
        .context("Failed to request latest release")?
        .error_for_status()
        .context("GitHub returned an error status for the release request")?
        .json()
        .await
        .context("Failed to parse release JSON")?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing tag_name in release JSON"))?
        .to_string();
    let assets = json
        .get("assets")
        .filter(|a| a.is_array())
        .cloned()
        .ok_or_else(|| anyhow!("Missing assets array in release JSON"))?;
    Ok((tag, assets))
}

/// Find a `browser_download_url` for an asset whose name contains both the
/// binary name and the target triple.
fn find_asset_url(assets: &Value, bin: &str, target: &str) -> Result<String> {
    let list = assets
        .as_array()
        .ok_or_else(|| anyhow!("Release assets is not an array"))?;
    let asset = list
        .iter()
        .find(|a| {
            let name = a["name"].as_str().unwrap_or_default();
            name.contains(bin) && name.contains(target)
        })
        .ok_or_else(|| anyhow!("No matching asset (bin '{bin}', target '{target}')"))?;
    asset["browser_download_url"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Asset is missing browser_download_url"))
}

/// Stream the response body at `url` into `path`.
async fn download_to(client: &Client, url: &str, path: &Path) -> Result<()> {
    use futures_util::StreamExt;

    let resp = client
        .get(url)
        .send()
        .await
        .context("Failed to request binary download")?
        .error_for_status()
        .context("Download request returned an error status")?;
    let mut file = tokio::fs::File::create(path)
        .await
        .with_context(|| format!("Failed to create temp file {}", path.display()))?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("Failed while streaming download")?;
        file.write_all(&bytes).await.context("Failed to write download chunk")?;
    }
    file.flush().await.context("Failed to flush download")?;
    Ok(())
}

/// Move `new_binary` to `dest`. If `dest` exists it is renamed to `<dest>.bak`
/// first, and restored if the swap fails. The installed file is made executable
/// on Unix.
fn swap_in_place(dest: &Path, new_binary: &Path) -> Result<()> {
    let backup = dest.exists().then(|| {
        let mut name = dest.as_os_str().to_os_string();
        name.push(".bak");
        PathBuf::from(name)
    });

    if let Some(backup) = &backup {
        std::fs::rename(dest, backup)
            .with_context(|| format!("Failed to back up {} to {}", dest.display(), backup.display()))?;
    }

    if let Err(e) = std::fs::rename(new_binary, dest).with_context(|| {
        format!(
            "Failed to move {} into place at {}",
            new_binary.display(),
            dest.display()
        )
    }) {
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, dest);
        }
        return Err(e);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dest)
            .with_context(|| format!("Failed to read metadata for {}", dest.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dest, perms).with_context(|| format!("Failed to chmod {}", dest.display()))?;
    }
    Ok(())
}

/// Compare dotted numeric versions, ignoring a leading `v` and any
/// pre-release/build suffix. Returns `true` when `latest` > `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn parse_version(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn version_comparison() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "v0.9.9"));
        assert!(is_newer("0.11.8", "0.11.7"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        // Pre-release / build suffixes are ignored for the numeric compare.
        assert!(!is_newer("1.2.3-rc1", "1.2.3"));
    }

    #[test]
    fn asset_matching_picks_bin_and_target() {
        let assets = json!([
            { "name": "alarm-server-x86_64-pc-windows-msvc.exe",
              "browser_download_url": "https://example.com/server-win" },
            { "name": "alarm-cli-x86_64-pc-windows-msvc.exe",
              "browser_download_url": "https://example.com/cli-win" },
            { "name": "alarm-cli-aarch64-unknown-linux-gnu",
              "browser_download_url": "https://example.com/cli-linux" },
        ]);
        let url = find_asset_url(&assets, "alarm-cli", "x86_64-pc-windows-msvc").unwrap();
        assert_eq!(url, "https://example.com/cli-win");

        assert!(find_asset_url(&assets, "alarm-cli", "riscv64-unknown-linux-gnu").is_err());
    }

    #[test]
    fn swap_replaces_existing_and_creates_backup() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("tool");
        let staged = dir.path().join("tool.update.tmp");
        fs::write(&dest, "old").unwrap();
        fs::write(&staged, "new").unwrap();

        swap_in_place(&dest, &staged).unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "new");
        let mut backup = dest.clone().into_os_string();
        backup.push(".bak");
        assert_eq!(fs::read_to_string(PathBuf::from(backup)).unwrap(), "old");
        assert!(!staged.exists());
    }

    #[test]
    fn swap_into_fresh_path_needs_no_backup() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("fresh");
        let staged = dir.path().join("fresh.update.tmp");
        fs::write(&staged, "new").unwrap();

        swap_in_place(&dest, &staged).unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "new");
        let mut backup = dest.into_os_string();
        backup.push(".bak");
        assert!(!PathBuf::from(backup).exists());
    }

    #[tokio::test]
    #[ignore = "network: hits the real GitHub API"]
    async fn fetch_latest_release_live() {
        let client = build_client().unwrap();
        let (tag, assets) = fetch_latest_release(
            &client,
            "https://api.github.com/repos/jm-observer/mcp-server/releases/latest",
        )
        .await
        .unwrap();
        assert!(tag.starts_with('v'));
        assert!(assets.as_array().is_some_and(|a| !a.is_empty()));
    }
}
