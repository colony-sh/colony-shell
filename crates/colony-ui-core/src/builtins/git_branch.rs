use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::process::Command;

pub struct GitBranchComponent;

impl GitBranchComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitBranchComponent {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_git_branch(output: &str) -> Option<(String, bool)> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix('*') {
            let branch_part = rest.trim();

            if branch_part.starts_with("(HEAD detached at ") {
                let hash = branch_part
                    .strip_prefix("(HEAD detached at ")
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or(branch_part);
                return Some((hash.to_string(), true));
            }

            if branch_part.starts_with('(') {
                return Some((branch_part.to_string(), true));
            }

            return Some((branch_part.to_string(), false));
        }
    }
    None
}

impl PromptComponent for GitBranchComponent {
    fn id(&self) -> &str {
        "git_branch"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let output = Command::new("git")
            .arg("branch")
            .current_dir(&ctx.cwd)
            .output();

        let (branch, detached) = match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                match parse_git_branch(&stdout) {
                    Some((b, d)) => (b, d),
                    None => return ComponentOutput::hidden(),
                }
            }
            _ => return ComponentOutput::hidden(),
        };

        let icon = "";
        let text = format!("{} {}", icon, branch);

        let color = if detached {
            Color::Yellow
        } else {
            Color::Green
        };

        let spans = vec![StyledSpan::new(text, Style::default().fg(color))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "branch": branch,
                "detached": detached,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_branch_normal() {
        let output = "* main\n  feature-x\n";
        let result = parse_git_branch(output);
        assert_eq!(result, Some(("main".to_string(), false)));
    }

    #[test]
    fn test_parse_git_branch_detached() {
        let output = "* (HEAD detached at abc1234)\n  main\n";
        let result = parse_git_branch(output);
        assert_eq!(result, Some(("abc1234".to_string(), true)));
    }

    #[test]
    fn test_parse_git_branch_empty() {
        let output = "";
        let result = parse_git_branch(output);
        assert_eq!(result, None);
    }
}
