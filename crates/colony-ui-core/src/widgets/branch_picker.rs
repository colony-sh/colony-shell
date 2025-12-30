use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBranch {
    pub name: String,
    pub current: bool,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBranch {
    pub name: String,
    pub tracking: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchPickerData {
    pub current: String,
    pub local: Vec<LocalBranch>,
    pub remote: Vec<RemoteBranch>,
    pub allow_create: bool,
}

pub fn gather_branch_data(cwd: &Path) -> Option<BranchPickerData> {
    let current = get_current_branch(cwd)?;
    let local = get_local_branches(cwd, &current);
    let remote = get_remote_branches(cwd, &local);

    Some(BranchPickerData {
        current,
        local,
        remote,
        allow_create: true,
    })
}

fn get_current_branch(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() {
            None
        } else {
            Some(branch)
        }
    } else {
        None
    }
}

fn get_local_branches(cwd: &Path, current: &str) -> Vec<LocalBranch> {
    let output = Command::new("git")
        .args(["branch", "-vv"])
        .current_dir(cwd)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut branches = Vec::new();

    for line in stdout.lines() {
        let is_current = line.starts_with('*');
        let line = line.trim_start_matches(['*', ' '].as_ref());

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let name = parts[0].to_string();
        let (ahead, behind) = parse_ahead_behind(line);

        branches.push(LocalBranch {
            current: is_current || name == current,
            name,
            ahead,
            behind,
        });
    }

    branches
}

fn parse_ahead_behind(line: &str) -> (usize, usize) {
    let mut ahead = 0;
    let mut behind = 0;

    if let Some(start) = line.find('[') {
        if let Some(end) = line.find(']') {
            let tracking_info = &line[start + 1..end];

            for part in tracking_info.split(", ") {
                if part.starts_with("ahead ") {
                    ahead = part
                        .strip_prefix("ahead ")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                } else if part.starts_with("behind ") {
                    behind = part
                        .strip_prefix("behind ")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                }
            }
        }
    }

    (ahead, behind)
}

fn get_remote_branches(cwd: &Path, local: &[LocalBranch]) -> Vec<RemoteBranch> {
    let output = Command::new("git")
        .args(["branch", "-r"])
        .current_dir(cwd)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let local_names: Vec<&str> = local.iter().map(|b| b.name.as_str()).collect();

    stdout
        .lines()
        .filter_map(|line| {
            let name = line.trim().to_string();

            if name.contains("HEAD") {
                return None;
            }

            let short_name = name
                .strip_prefix("origin/")
                .or_else(|| name.split('/').nth(1))
                .unwrap_or(&name);

            let tracking = if local_names.contains(&short_name) {
                Some(short_name.to_string())
            } else {
                None
            };

            Some(RemoteBranch { name, tracking })
        })
        .collect()
}

pub fn switch_branch(cwd: &Path, branch: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["checkout", branch])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        Ok(format!("Switched to branch '{}'", branch))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn create_branch(cwd: &Path, name: &str, from: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["checkout", "-b", name, from])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        Ok(format!("Created and switched to branch '{}'", name))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn checkout_remote(cwd: &Path, remote: &str, local_name: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["checkout", "-b", local_name, remote])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        Ok(format!(
            "Created branch '{}' tracking '{}'",
            local_name, remote
        ))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ahead_behind() {
        assert_eq!(parse_ahead_behind("main abc123 [origin/main]"), (0, 0));
        assert_eq!(
            parse_ahead_behind("main abc123 [ahead 2]"),
            (2, 0)
        );
        assert_eq!(
            parse_ahead_behind("main abc123 [behind 3]"),
            (0, 3)
        );
        assert_eq!(
            parse_ahead_behind("main abc123 [ahead 2, behind 3]"),
            (2, 3)
        );
    }

    #[test]
    fn test_branch_picker_data_serialization() {
        let data = BranchPickerData {
            current: "main".to_string(),
            local: vec![LocalBranch {
                name: "main".to_string(),
                current: true,
                ahead: 0,
                behind: 0,
            }],
            remote: vec![RemoteBranch {
                name: "origin/main".to_string(),
                tracking: Some("main".to_string()),
            }],
            allow_create: true,
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"current\":\"main\""));
        assert!(json.contains("\"allow_create\":true"));
    }
}
