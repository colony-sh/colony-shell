//! Load and execute `.colony/init` startup scripts.
//!
//! Layered: `~/.colony/init` runs first (global), then `.colony/init`
//! in the current directory runs second (project-specific).

use std::path::{Path, PathBuf};

/// Discover init scripts to run, in order.
pub fn discover_init_scripts() -> Vec<PathBuf> {
    let mut scripts = Vec::new();

    // 1. Global: ~/.colony/init
    if let Some(home) = dirs::home_dir() {
        let global = home.join(".colony").join("init");
        if global.is_file() {
            scripts.push(global);
        }
    }

    // 2. Project: .colony/init in current directory
    if let Ok(cwd) = std::env::current_dir() {
        let project = cwd.join(".colony").join("init");
        if project.is_file() {
            // Don't run the same file twice (if cwd is home)
            if scripts.last().map_or(true, |last| last != &project) {
                scripts.push(project);
            }
        }
    }

    scripts
}

/// Read an init script and return its lines as commands to execute.
pub fn read_init_script(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_read_init_script() {
        let dir = TempDir::new().unwrap();
        let init = dir.path().join("init");
        fs::write(
            &init,
            "# comment\nsidebar left filetree\n\nalias ll=\"ls -la\"\n",
        )
        .unwrap();

        let lines = read_init_script(&init);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "sidebar left filetree");
        assert_eq!(lines[1], "alias ll=\"ls -la\"");
    }

    #[test]
    fn test_read_missing_script() {
        let lines = read_init_script(Path::new("/nonexistent/init"));
        assert!(lines.is_empty());
    }
}
