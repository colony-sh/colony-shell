//! PATH resolution for command lookup

use crate::{ExecutionError, Result};
use std::path::{Path, PathBuf};

/// Resolve a command name to an executable path
///
/// - If the command contains '/', treat as path (absolute or relative)
/// - Otherwise, search PATH directories for executable file
pub fn resolve_command(command: &str, path_env: Option<&str>) -> Result<PathBuf> {
    // If command contains '/', it's a path - don't search PATH
    if command.contains('/') {
        let path = PathBuf::from(command);
        if path.exists() {
            if is_executable(&path)? {
                return Ok(path);
            } else {
                return Err(ExecutionError::PermissionDenied(command.to_string()));
            }
        } else {
            return Err(ExecutionError::CommandNotFound(command.to_string()));
        }
    }

    // Search PATH
    let path_env_owned;
    let path_env = if let Some(p) = path_env {
        p
    } else {
        path_env_owned = std::env::var("PATH").unwrap_or_default();
        &path_env_owned
    };

    for dir in path_env.split(':') {
        if dir.is_empty() {
            continue;
        }

        let candidate = Path::new(dir).join(command);
        if candidate.exists() && is_executable(&candidate)? {
            return Ok(candidate);
        }
    }

    Err(ExecutionError::CommandNotFound(command.to_string()))
}

/// Check if a file is executable
fn is_executable(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)?;
    let permissions = metadata.permissions();

    // Check if user executable bit is set
    Ok(permissions.mode() & 0o100 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_absolute_path() {
        // /bin/sh should exist on all Unix systems
        let result = resolve_command("/bin/sh", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_from_path() {
        // 'ls' should be in PATH on all Unix systems
        let result = resolve_command("ls", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_nonexistent_command() {
        let result = resolve_command("this_command_definitely_does_not_exist_12345", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_path() {
        let result = resolve_command("sh", Some("/bin:/usr/bin"));
        assert!(result.is_ok());
    }
}
