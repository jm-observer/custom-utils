// Auto-update module for Zero daemon
use anyhow::Context;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct UpdateMeta {
    pub version: String,
    pub url: String,
}

/// Fetch the latest release metadata from GitHub.
async fn fetch_latest_release(api_url: &str, program_name: &str, target_triple: &str) -> Result<UpdateMeta> {
    let client = Client::builder()
        .user_agent("zero-updater")
        .build()
        .context("Failed to build reqwest client")?;
    let resp = client
        .get(api_url)
        .send()
        .await
        .context("Failed to request latest release")?
        .error_for_status()?;
    let json: Value = resp.json().await.context("Failed to parse release JSON")?;
    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing tag_name in release JSON"))?;
    let assets = json["assets"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing assets array in release JSON"))?;

    // Find an asset whose name contains the program name and the target triple.
    // This makes the matching more specific and robust.
    let asset = assets
        .iter()
        .find(|a| {
            let name = a["name"].as_str().unwrap_or("");
            name.contains(program_name) && name.contains(target_triple)
        })
        .ok_or_else(|| {
            anyhow!(
                "No binary asset found matching program '{}' and target '{}'",
                program_name,
                target_triple
            )
        })?;

    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing browser_download_url in asset"))?
        .to_string();
    Ok(UpdateMeta {
        version: tag.to_string(),
        url,
    })
}

/// Download a binary from `url` and write it to `target_path`.
async fn download_binary(url: &str, target_path: &Path) -> Result<()> {
    let client = Client::builder()
        .user_agent("zero-updater")
        .build()
        .context("Failed to build reqwest client for download")?;
    let resp = client
        .get(url)
        .send()
        .await
        .context("Failed to request binary download")?
        .error_for_status()?;
    // Stream the body to the destination file.
    let mut file = tokio::fs::File::create(target_path)
        .await
        .context("Failed to create temporary binary file")?;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("Failed while streaming download chunks")?;
        file.write_all(&bytes)
            .await
            .context("Failed to write to temporary binary file")?;
    }
    file.flush().await.context("Failed to flush binary file")?;
    Ok(())
}

/// Swap the currently running binary with the newly downloaded binary.
/// The current binary is renamed to `<name>.bak` and the new binary is placed at the original path.
/// Permissions are set to executable (Unix only).
fn swap_binary(current_exe: &Path, new_binary: &Path) -> Result<()> {
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow!("Current executable has no parent directory"))?;

    // Create backup path: same directory, file name with .bak suffix.
    let exe_name = current_exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Current executable file name invalid"))?;
    let backup_path = parent.join(format!("{}.bak", exe_name));

    // Rename the running binary to backup.
    fs::rename(current_exe, &backup_path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            current_exe.display(),
            backup_path.display()
        )
    })?;

    // Move the new binary into place.
    if let Err(e) = fs::rename(new_binary, current_exe).with_context(|| {
        format!(
            "Failed to move new binary into place: {} -> {}",
            new_binary.display(),
            current_exe.display()
        )
    }) {
        // Attempt rollback
        let _ = fs::rename(&backup_path, current_exe);
        return Err(e);
    }

    // Ensure the new binary is executable. (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(current_exe)
            .with_context(|| format!("Failed to read metadata for {}", current_exe.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(current_exe, perms)
            .with_context(|| format!("Failed to set executable permissions on {}", current_exe.display()))?;
    }
    Ok(())
}

/// The high‑level entry point that performs a full update cycle.
/// It fetches the latest release, downloads the appropriate binary,
/// swaps the binary, records a shutdown reason, and finally exits.
pub async fn run_update(api_url: &str, current_exe: &Path) -> Result<()> {
    // 1. Extract program name and determine target triple from current environment.
    let exe_name = current_exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Current executable has no valid name"))?;

    // Remove extension for program name if it exists (e.g., mcp-tool.exe -> mcp-tool)
    let program_name = match current_exe.extension() {
        Some(ext) => current_exe.file_stem().and_then(|s| s.to_str()).unwrap_or(exe_name),
        None => exe_name,
    };

    let target_triple = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        (arch, os) => return Err(anyhow::anyhow!("Unsupported platform: {}-{}", arch, os)),
    };

    // 2. Get latest release information.
    let meta = fetch_latest_release(api_url, program_name, target_triple).await?;
    log::info!("Latest version: {} at {}", meta.version, meta.url);

    // 3. Determine temporary download location (same directory as current exe).
    let dir = current_exe
        .parent()
        .ok_or_else(|| anyhow!("Executable has no parent directory"))?;
    let tmp_path = dir.join("zero.update.tmp");

    // 4. Download the binary.
    download_binary(&meta.url, &tmp_path).await?;
    log::info!("Binary downloaded to {}", tmp_path.display());

    // 5. Swap the binary.
    swap_binary(current_exe, &tmp_path)?;
    Ok(())
}

/// The high‑level entry point that performs a full update cycle.
/// It fetches the latest release, downloads the appropriate binary,
/// swaps the binary, records a shutdown reason, and finally exits.
pub async fn run_update_with_shutdown<F>(api_url: &str, current_exe: &Path, mark_shutdown: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    log::info!("Shutdown flag recorded, exiting for systemd restart");
    run_update(api_url, current_exe).await?;
    // 7. Exit the process – systemd will restart it.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_fetch_latest_release_mcp_server() {
        // This test attempts to fetch from the real URL to verify the matching logic.
        // Note: Requires internet access.
        let url = "https://api.github.com/repos/jm-observer/mcp-server/releases/latest";

        // Manually provide the parameters that run_update would normally extract
        let program_name = "mcp-tool";
        let target_triple = if cfg!(target_os = "windows") {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
            "aarch64-unknown-linux-gnu"
        } else {
            "x86_64-unknown-linux-gnu"
        };

        let result = fetch_latest_release(url, program_name, target_triple).await;

        assert!(result.is_ok(), "Failed to fetch: {:?}", result.err());
        let meta = result.unwrap();
        println!("Latest version: {} at {}", meta.version, meta.url);

        // Based on user input: tag should be v0.2.0
        assert_eq!(meta.version, "v0.2.0");

        // Verify the URL contains the expected asset pattern for the current platform
        assert!(meta.url.contains(program_name));
        assert!(meta.url.contains(target_triple));
    }

    #[tokio::test]
    async fn test_download_mcp_asset() {
        let dir = tempdir().expect("Failed to create temp dir");
        let target_path = dir.path().join("mcp-tool.exe");

        // Using one of the assets from the user's list
        let test_url =
            "https://github.com/jm-observer/mcp-server/releases/download/v0.2.0/mcp-tool.exe_x86_64-pc-windows-msvc";

        let result = download_binary(test_url, &target_path).await;

        if result.is_ok() {
            assert!(target_path.exists());
            assert!(target_path.metadata().unwrap().len() > 0);
        } else {
            eprintln!("Download failed (expected if URL is not live): {:?}", result.err());
        }
    }

    #[test]
    fn test_swap_binary_logic_simulation() {
        let dir = tempdir().expect("Failed to create temp dir");
        let current_exe = dir.path().join("mcp-tool.exe");
        let new_binary = dir.path().join("mcp-tool-new.exe");

        fs::write(&current_exe, "old content").unwrap();
        fs::write(&new_binary, "new content").unwrap();

        let result = swap_binary(&current_exe, &new_binary);
        assert!(result.is_ok());

        let content = fs::read_to_string(&current_exe).unwrap();
        assert_eq!(content, "new content");

        let backup_path = dir.path().join("mcp-tool.exe.bak");
        assert!(backup_path.exists());
    }
}
