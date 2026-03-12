//! Skill execution engine
//!
//! Matches user input against skill trigger patterns and dispatches to handlers.
//! Skills are defined in recipe TOML and can be triggered by command prefixes,
//! regex patterns, or keywords.

use std::collections::HashMap;

use crate::recipe::SkillDef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    CommandPrefix(String),
    Regex(String),
    Keyword(String),
}

impl TriggerKind {
    pub fn parse(trigger: &str) -> Self {
        if trigger.starts_with('/') {
            TriggerKind::CommandPrefix(trigger.to_string())
        } else if trigger.starts_with("regex:") {
            TriggerKind::Regex(trigger.strip_prefix("regex:").unwrap_or("").to_string())
        } else {
            TriggerKind::Keyword(trigger.to_string())
        }
    }

    pub fn matches(&self, input: &str) -> bool {
        match self {
            TriggerKind::CommandPrefix(prefix) => {
                input == prefix || input.starts_with(&format!("{prefix} "))
            }
            TriggerKind::Regex(pattern) => {
                regex_matches(pattern, input)
            }
            TriggerKind::Keyword(keyword) => {
                input
                    .split_whitespace()
                    .any(|w| w.eq_ignore_ascii_case(keyword))
            }
        }
    }
}

fn regex_matches(pattern: &str, input: &str) -> bool {
    let pattern = if pattern.starts_with('^') {
        pattern.to_string()
    } else {
        format!("(?i){pattern}")
    };

    let mut chars = pattern.chars().peekable();
    let regex_str: String = std::iter::from_fn(|| chars.next()).collect();

    simple_glob_match(&regex_str, input)
}

fn simple_glob_match(pattern: &str, input: &str) -> bool {
    let clean = pattern
        .trim_start_matches("(?i)")
        .trim_start_matches('^')
        .trim_end_matches('$');

    if clean.contains('*') || clean.contains('?') || clean.contains('[') {
        let parts: Vec<&str> = clean.split('*').collect();
        if parts.len() == 1 {
            return input.contains(clean);
        }

        let lowered = input.to_lowercase();
        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            let part_lower = part.to_lowercase();
            let Some(found) = lowered[pos..].find(&part_lower) else {
                return false;
            };
            if i == 0 && !pattern.trim_start_matches("(?i)").starts_with('*') && found != 0 {
                return false;
            }
            pos += found + part_lower.len();
        }
        true
    } else {
        input.to_lowercase().contains(&clean.to_lowercase())
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub name: String,
    pub trigger_kind: TriggerKind,
    pub description: Option<String>,
    pub prompt_template: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SkillMatch {
    pub skill: ResolvedSkill,
    pub user_input: String,
    pub arguments: String,
}

pub struct SkillEngine {
    skills: Vec<ResolvedSkill>,
}

impl Default for SkillEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillEngine {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
        }
    }

    pub fn from_skill_defs(defs: &HashMap<String, SkillDef>) -> Self {
        let mut engine = Self::new();
        for (name, def) in defs {
            engine.register(name, def);
        }
        engine
    }

    pub fn register(&mut self, name: &str, def: &SkillDef) {
        let trigger_str = def
            .trigger
            .as_deref()
            .unwrap_or(name);
        let trigger_kind = TriggerKind::parse(trigger_str);

        self.skills.push(ResolvedSkill {
            name: name.to_string(),
            trigger_kind,
            description: def.description.clone(),
            prompt_template: def.prompt.clone(),
        });
    }

    pub fn match_input(&self, input: &str) -> Option<SkillMatch> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        for skill in &self.skills {
            if skill.trigger_kind.matches(trimmed) {
                let arguments = extract_arguments(trimmed, &skill.trigger_kind);
                return Some(SkillMatch {
                    skill: skill.clone(),
                    user_input: trimmed.to_string(),
                    arguments,
                });
            }
        }

        None
    }

    pub fn expand_prompt(&self, skill_match: &SkillMatch) -> Option<String> {
        let template = skill_match.skill.prompt_template.as_ref()?;
        let expanded = template
            .replace("{{input}}", &skill_match.user_input)
            .replace("{{args}}", &skill_match.arguments);
        Some(expanded)
    }

    pub fn list_skills(&self) -> &[ResolvedSkill] {
        &self.skills
    }
}

fn extract_arguments(input: &str, trigger: &TriggerKind) -> String {
    match trigger {
        TriggerKind::CommandPrefix(prefix) => {
            input
                .strip_prefix(prefix)
                .unwrap_or("")
                .trim()
                .to_string()
        }
        TriggerKind::Regex(_) | TriggerKind::Keyword(_) => input.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_parse_command_prefix() {
        let trigger = TriggerKind::parse("/review");
        assert_eq!(trigger, TriggerKind::CommandPrefix("/review".to_string()));
    }

    #[test]
    fn test_trigger_parse_regex() {
        let trigger = TriggerKind::parse("regex:test.*pattern");
        assert_eq!(trigger, TriggerKind::Regex("test.*pattern".to_string()));
    }

    #[test]
    fn test_trigger_parse_keyword() {
        let trigger = TriggerKind::parse("deploy");
        assert_eq!(trigger, TriggerKind::Keyword("deploy".to_string()));
    }

    #[test]
    fn test_command_prefix_exact_match() {
        let trigger = TriggerKind::CommandPrefix("/review".to_string());
        assert!(trigger.matches("/review"));
    }

    #[test]
    fn test_command_prefix_with_args() {
        let trigger = TriggerKind::CommandPrefix("/review".to_string());
        assert!(trigger.matches("/review src/main.rs"));
    }

    #[test]
    fn test_command_prefix_no_match() {
        let trigger = TriggerKind::CommandPrefix("/review".to_string());
        assert!(!trigger.matches("/reviews"));
        assert!(!trigger.matches("review"));
    }

    #[test]
    fn test_keyword_match() {
        let trigger = TriggerKind::Keyword("deploy".to_string());
        assert!(trigger.matches("please deploy now"));
        assert!(trigger.matches("deploy"));
        assert!(trigger.matches("DEPLOY the service"));
    }

    #[test]
    fn test_keyword_no_partial() {
        let trigger = TriggerKind::Keyword("test".to_string());
        assert!(!trigger.matches("testing"));
        assert!(trigger.matches("run test now"));
    }

    #[test]
    fn test_regex_match() {
        let trigger = TriggerKind::Regex("fix*bug".to_string());
        assert!(trigger.matches("fix this bug"));
        assert!(trigger.matches("fix bug"));
    }

    #[test]
    fn test_skill_engine_register_and_match() {
        let mut engine = SkillEngine::new();
        engine.register(
            "code-review",
            &SkillDef {
                trigger: Some("/review".to_string()),
                description: Some("Review code".to_string()),
                prompt: Some("Review the following code: {{args}}".to_string()),
            },
        );

        let m = engine.match_input("/review src/lib.rs").unwrap();
        assert_eq!(m.skill.name, "code-review");
        assert_eq!(m.arguments, "src/lib.rs");
    }

    #[test]
    fn test_skill_engine_no_match() {
        let mut engine = SkillEngine::new();
        engine.register(
            "review",
            &SkillDef {
                trigger: Some("/review".to_string()),
                description: None,
                prompt: None,
            },
        );

        assert!(engine.match_input("/deploy").is_none());
        assert!(engine.match_input("").is_none());
    }

    #[test]
    fn test_skill_engine_expand_prompt() {
        let mut engine = SkillEngine::new();
        engine.register(
            "test",
            &SkillDef {
                trigger: Some("/test".to_string()),
                description: None,
                prompt: Some("Run tests for: {{args}}".to_string()),
            },
        );

        let m = engine.match_input("/test utils").unwrap();
        let expanded = engine.expand_prompt(&m).unwrap();
        assert_eq!(expanded, "Run tests for: utils");
    }

    #[test]
    fn test_skill_engine_from_defs() {
        let mut defs = HashMap::new();
        defs.insert(
            "lint".to_string(),
            SkillDef {
                trigger: Some("/lint".to_string()),
                description: None,
                prompt: None,
            },
        );
        defs.insert(
            "deploy".to_string(),
            SkillDef {
                trigger: Some("deploy".to_string()),
                description: None,
                prompt: None,
            },
        );

        let engine = SkillEngine::from_skill_defs(&defs);
        assert_eq!(engine.list_skills().len(), 2);
    }

    #[test]
    fn test_skill_engine_default_trigger_from_name() {
        let mut engine = SkillEngine::new();
        engine.register(
            "deploy",
            &SkillDef {
                trigger: None,
                description: None,
                prompt: None,
            },
        );

        let m = engine.match_input("please deploy now");
        assert!(m.is_some());
        assert_eq!(m.unwrap().skill.name, "deploy");
    }

    #[test]
    fn test_extract_arguments_command_prefix() {
        let trigger = TriggerKind::CommandPrefix("/review".to_string());
        assert_eq!(extract_arguments("/review src/lib.rs", &trigger), "src/lib.rs");
        assert_eq!(extract_arguments("/review", &trigger), "");
    }

    #[test]
    fn test_extract_arguments_keyword() {
        let trigger = TriggerKind::Keyword("deploy".to_string());
        assert_eq!(
            extract_arguments("please deploy now", &trigger),
            "please deploy now"
        );
    }
}
