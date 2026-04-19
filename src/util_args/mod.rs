use anyhow::Context;
use std::path::PathBuf;

/// arg_value("--check", "-c")
pub fn arg_value(long: &str, short: &str) -> Option<String> {
    assert!(long.starts_with("--"));
    assert!(short.starts_with('-'));
    let mut is_val = false;
    for arg in std::env::args() {
        if is_val {
            return Some(arg);
        }
        is_val = arg == long || arg == short;
    }
    None
}

/// exist_arg("--check", "-c")
pub fn exist_arg(long: &str, short: &str) -> bool {
    assert!(long.starts_with("--"));
    assert!(short.starts_with('-'));
    for arg in std::env::args() {
        if arg == long || arg == short {
            return true;
        }
    }
    false
}

pub fn command() -> Option<String> {
    for (index, arg) in std::env::args().enumerate() {
        if index == 1 {
            return Some(arg);
        }
    }
    None
}

/// Gets the current system user's home directory.
pub fn get_user_home() -> anyhow::Result<PathBuf> {
    home::home_dir().context("Failed to get user home directory")
}

/// Expands path, supporting '~' for the user's home directory.
pub fn expand_path(path: &str) -> anyhow::Result<PathBuf> {
    #[cfg(not(windows))]
    {
        let expanded = shellexpand::tilde(path);
        return Ok(PathBuf::from(expanded.into_owned()));
    }

    #[cfg(windows)]
    {
        if let Some(stripped) = path.strip_prefix('~') {
            let home = get_user_home()?;
            // Remove leading separator if present to avoid join issues
            let suffix = stripped.trim_start_matches(['/', '\\']);
            Ok(home.join(suffix))
        } else if let Some(stripped) = path.strip_prefix("./").or_else(|| path.strip_prefix(".\\")) {
            let current_dir = std::env::current_dir().context("Failed to get current directory")?;
            // On Windows, if the remaining part starts with a separator,
            // PathBuf::join might treat it as an absolute path from the drive root.
            // We strip all leading separators to ensure it's joined as a relative path.
            let suffix = stripped.trim_start_matches(['/', '\\']);
            Ok(current_dir.join(suffix))
        } else if path == "." {
            std::env::current_dir().context("Failed to get current directory")
        } else {
            Ok(PathBuf::from(path))
        }
    }
}

/// $HOME/.config/app
pub fn workspace(workspace: &Option<String>, app: &str) -> anyhow::Result<PathBuf> {
    let workspace = if let Some(workspace) = workspace {
        expand_path(workspace)?
    } else {
        get_user_home()?.join(".config").join(app)
    };
    log::debug!("{}", workspace.display());
    Ok(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_get_user_home() {
        let home = get_user_home();
        assert!(home.is_ok());
    }

    #[test]
    fn test_expand_path_tilde() {
        let home = get_user_home().unwrap();
        let expanded = expand_path("~").unwrap();
        assert_eq!(expanded, home);
    }

    #[test]
    fn test_expand_path_dot() {
        let cwd = std::env::current_dir().unwrap();
        let expanded = expand_path("./test").unwrap();
        assert_eq!(expanded, cwd.join("test"));
    }
}
