//! Auto-gather project context from current working directory.

use std::path::Path;
use std::process::Command;

/// Detected project information.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: String,
    pub git_branch: Option<String>,
    pub git_status_summary: Option<String>,
    pub languages: Vec<String>,
    pub build_file: Option<String>,
}

impl ProjectContext {
    /// Gather project context from the given directory.
    pub fn gather(cwd: &Path) -> Self {
        Self {
            root: cwd.to_string_lossy().to_string(),
            git_branch: Self::git_branch(cwd),
            git_status_summary: Self::git_status(cwd),
            languages: Self::detect_languages(cwd),
            build_file: Self::detect_build_file(cwd),
        }
    }

    /// Render as text for inclusion in context.
    pub fn render(&self) -> String {
        let mut lines = vec![format!("Project: {}", self.root)];

        if let Some(ref branch) = self.git_branch {
            lines.push(format!("Git branch: {branch}"));
        }
        if let Some(ref status) = self.git_status_summary {
            lines.push(format!("Git status: {status}"));
        }
        if !self.languages.is_empty() {
            lines.push(format!("Languages: {}", self.languages.join(", ")));
        }
        if let Some(ref build) = self.build_file {
            lines.push(format!("Build: {build}"));
        }

        lines.join("\n")
    }

    fn git_branch(cwd: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn git_status(cwd: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["status", "--short"])
            .current_dir(cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let count = text.lines().filter(|l| !l.is_empty()).count();
        if count == 0 {
            return Some("clean".to_string());
        }
        Some(format!("{count} changed files"))
    }

    fn detect_languages(cwd: &Path) -> Vec<String> {
        let indicators: &[(&str, &str)] = &[
            ("Cargo.toml", "Rust"),
            ("package.json", "JavaScript/TypeScript"),
            ("go.mod", "Go"),
            ("pyproject.toml", "Python"),
            ("requirements.txt", "Python"),
            ("Gemfile", "Ruby"),
            ("mix.exs", "Elixir"),
            ("build.gradle", "Java/Kotlin"),
            ("pom.xml", "Java"),
            ("Package.swift", "Swift"),
        ];

        indicators
            .iter()
            .filter(|(file, _)| cwd.join(file).exists())
            .map(|(_, lang)| (*lang).to_string())
            .collect()
    }

    fn detect_build_file(cwd: &Path) -> Option<String> {
        let files = [
            "Cargo.toml",
            "package.json",
            "Makefile",
            "CMakeLists.txt",
            "build.gradle",
            "pom.xml",
            "mix.exs",
        ];
        files
            .iter()
            .find(|f| cwd.join(f).exists())
            .map(|f| (*f).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn gather_detects_rust_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let ctx = ProjectContext::gather(tmp.path());
        assert!(ctx.languages.contains(&"Rust".to_string()));
        assert_eq!(ctx.build_file.as_deref(), Some("Cargo.toml"));
    }

    #[test]
    fn gather_detects_multiple_languages() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();

        let ctx = ProjectContext::gather(tmp.path());
        assert_eq!(ctx.languages.len(), 2);
        assert!(ctx.languages.contains(&"Rust".to_string()));
        assert!(ctx.languages.contains(&"JavaScript/TypeScript".to_string()));
    }

    #[test]
    fn render_includes_project_root() {
        let ctx = ProjectContext {
            root: "/test/project".to_string(),
            git_branch: Some("main".to_string()),
            git_status_summary: Some("clean".to_string()),
            languages: vec!["Rust".to_string()],
            build_file: Some("Cargo.toml".to_string()),
        };

        let rendered = ctx.render();
        assert!(rendered.contains("Project: /test/project"));
        assert!(rendered.contains("Git branch: main"));
        assert!(rendered.contains("Git status: clean"));
        assert!(rendered.contains("Languages: Rust"));
        assert!(rendered.contains("Build: Cargo.toml"));
    }

    #[test]
    fn render_omits_missing_fields() {
        let ctx = ProjectContext {
            root: "/tmp".to_string(),
            git_branch: None,
            git_status_summary: None,
            languages: Vec::new(),
            build_file: None,
        };

        let rendered = ctx.render();
        assert!(rendered.contains("Project: /tmp"));
        assert!(!rendered.contains("Git"));
        assert!(!rendered.contains("Languages"));
        assert!(!rendered.contains("Build"));
    }
}
