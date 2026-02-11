//! Recipe command execution engine
//!
//! Handles execution of recipe-defined commands, dispatching to the appropriate
//! handler based on the action type (shell, agent, skill).

use crate::error::{RecipeError, Result};
use crate::recipe::{CommandAction, CommandDef};
use crate::skill::SkillEngine;

#[derive(Debug, Clone)]
pub struct ShellAction {
    pub command: String,
}

#[derive(Debug, Clone)]
pub struct AgentAction {
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct SkillAction {
    pub skill_name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub enum ResolvedAction {
    Shell(ShellAction),
    Agent(AgentAction),
    Skill(SkillAction),
}

impl ResolvedAction {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ResolvedAction::Shell(_) => "shell",
            ResolvedAction::Agent(_) => "agent",
            ResolvedAction::Skill(_) => "skill",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Success,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub action: ResolvedAction,
    pub output: Option<String>,
}

pub fn resolve_command(
    command_def: &CommandDef,
    user_args: &str,
) -> Result<ResolvedAction> {
    match command_def.action {
        CommandAction::Shell => {
            let base_cmd = command_def
                .command
                .as_deref()
                .ok_or_else(|| {
                    RecipeError::Invalid(format!(
                        "shell command '{}' has no 'command' field",
                        command_def.name
                    ))
                })?;

            let full_command = if user_args.is_empty() {
                base_cmd.to_string()
            } else {
                format!("{base_cmd} {user_args}")
            };

            Ok(ResolvedAction::Shell(ShellAction {
                command: full_command,
            }))
        }
        CommandAction::Agent => {
            let base_prompt = command_def
                .prompt
                .as_deref()
                .ok_or_else(|| {
                    RecipeError::Invalid(format!(
                        "agent command '{}' has no 'prompt' field",
                        command_def.name
                    ))
                })?;

            let full_prompt = if user_args.is_empty() {
                base_prompt.to_string()
            } else {
                format!("{base_prompt}\n\nUser context: {user_args}")
            };

            Ok(ResolvedAction::Agent(AgentAction {
                prompt: full_prompt,
            }))
        }
        CommandAction::Skill => {
            let skill_name = command_def
                .command
                .as_deref()
                .or(command_def.prompt.as_deref())
                .ok_or_else(|| {
                    RecipeError::Invalid(format!(
                        "skill command '{}' has no skill reference in 'command' or 'prompt' field",
                        command_def.name
                    ))
                })?;

            Ok(ResolvedAction::Skill(SkillAction {
                skill_name: skill_name.to_string(),
                arguments: user_args.to_string(),
            }))
        }
    }
}

pub fn find_command<'a>(commands: &'a [CommandDef], name: &str) -> Option<&'a CommandDef> {
    commands.iter().find(|c| c.name == name)
}

pub fn resolve_skill_action(
    skill_action: &SkillAction,
    engine: &SkillEngine,
) -> Result<Option<String>> {
    let synthetic_input = format!("/{} {}", skill_action.skill_name, skill_action.arguments);
    let Some(skill_match) = engine.match_input(&synthetic_input) else {
        let direct_match = engine.match_input(&skill_action.skill_name);
        let Some(m) = direct_match else {
            return Err(RecipeError::NotFound(format!(
                "skill '{}'",
                skill_action.skill_name
            )));
        };
        return Ok(engine.expand_prompt(&m));
    };

    Ok(engine.expand_prompt(&skill_match))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::SkillDef;

    fn shell_command(name: &str, cmd: &str) -> CommandDef {
        CommandDef {
            name: name.to_string(),
            description: None,
            action: CommandAction::Shell,
            command: Some(cmd.to_string()),
            prompt: None,
        }
    }

    fn agent_command(name: &str, prompt: &str) -> CommandDef {
        CommandDef {
            name: name.to_string(),
            description: None,
            action: CommandAction::Agent,
            command: None,
            prompt: Some(prompt.to_string()),
        }
    }

    fn skill_command(name: &str, skill_ref: &str) -> CommandDef {
        CommandDef {
            name: name.to_string(),
            description: None,
            action: CommandAction::Skill,
            command: Some(skill_ref.to_string()),
            prompt: None,
        }
    }

    #[test]
    fn test_resolve_shell_command() {
        let cmd = shell_command("build", "cargo build");
        let resolved = resolve_command(&cmd, "").unwrap();
        match resolved {
            ResolvedAction::Shell(action) => assert_eq!(action.command, "cargo build"),
            _ => panic!("expected shell action"),
        }
    }

    #[test]
    fn test_resolve_shell_command_with_args() {
        let cmd = shell_command("test", "cargo test");
        let resolved = resolve_command(&cmd, "--release").unwrap();
        match resolved {
            ResolvedAction::Shell(action) => assert_eq!(action.command, "cargo test --release"),
            _ => panic!("expected shell action"),
        }
    }

    #[test]
    fn test_resolve_shell_command_missing_command() {
        let cmd = CommandDef {
            name: "broken".to_string(),
            description: None,
            action: CommandAction::Shell,
            command: None,
            prompt: None,
        };
        let result = resolve_command(&cmd, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no 'command' field"));
    }

    #[test]
    fn test_resolve_agent_command() {
        let cmd = agent_command("explain", "Explain the following code.");
        let resolved = resolve_command(&cmd, "").unwrap();
        match resolved {
            ResolvedAction::Agent(action) => {
                assert_eq!(action.prompt, "Explain the following code.");
            }
            _ => panic!("expected agent action"),
        }
    }

    #[test]
    fn test_resolve_agent_command_with_args() {
        let cmd = agent_command("explain", "Explain the following code.");
        let resolved = resolve_command(&cmd, "src/main.rs").unwrap();
        match resolved {
            ResolvedAction::Agent(action) => {
                assert!(action.prompt.contains("Explain the following code."));
                assert!(action.prompt.contains("src/main.rs"));
            }
            _ => panic!("expected agent action"),
        }
    }

    #[test]
    fn test_resolve_agent_command_missing_prompt() {
        let cmd = CommandDef {
            name: "broken".to_string(),
            description: None,
            action: CommandAction::Agent,
            command: None,
            prompt: None,
        };
        let result = resolve_command(&cmd, "");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_command() {
        let cmd = skill_command("review", "code-review");
        let resolved = resolve_command(&cmd, "src/lib.rs").unwrap();
        match resolved {
            ResolvedAction::Skill(action) => {
                assert_eq!(action.skill_name, "code-review");
                assert_eq!(action.arguments, "src/lib.rs");
            }
            _ => panic!("expected skill action"),
        }
    }

    #[test]
    fn test_resolve_skill_command_missing_ref() {
        let cmd = CommandDef {
            name: "broken".to_string(),
            description: None,
            action: CommandAction::Skill,
            command: None,
            prompt: None,
        };
        let result = resolve_command(&cmd, "");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_command_found() {
        let commands = vec![
            shell_command("build", "cargo build"),
            shell_command("test", "cargo test"),
        ];
        let found = find_command(&commands, "test");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test");
    }

    #[test]
    fn test_find_command_not_found() {
        let commands = vec![shell_command("build", "cargo build")];
        assert!(find_command(&commands, "deploy").is_none());
    }

    #[test]
    fn test_resolved_action_kind_name() {
        assert_eq!(
            ResolvedAction::Shell(ShellAction {
                command: "ls".to_string()
            })
            .kind_name(),
            "shell"
        );
        assert_eq!(
            ResolvedAction::Agent(AgentAction {
                prompt: "hi".to_string()
            })
            .kind_name(),
            "agent"
        );
        assert_eq!(
            ResolvedAction::Skill(SkillAction {
                skill_name: "x".to_string(),
                arguments: String::new()
            })
            .kind_name(),
            "skill"
        );
    }

    #[test]
    fn test_resolve_skill_action_with_engine() {
        let mut engine = SkillEngine::new();
        engine.register(
            "code-review",
            &SkillDef {
                trigger: Some("/code-review".to_string()),
                description: None,
                prompt: Some("Review: {{args}}".to_string()),
            },
        );

        let action = SkillAction {
            skill_name: "code-review".to_string(),
            arguments: "main.rs".to_string(),
        };

        let result = resolve_skill_action(&action, &engine).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("main.rs"));
    }
}
