//! Tilde expansion
//!
//! POSIX rules:
//! - ~ expands to $HOME
//! - ~user expands to user's home directory
//! - Tilde only expands at the start of a word
//! - Tilde does not expand if quoted

use crate::{ExpansionContext, ExpansionError, Result};
/// Expand tilde to home directory
///
/// - `~` → $HOME
/// - `~user` → user's home directory (via getpwnam on Unix)
pub fn expand_tilde(user: Option<&str>, context: &ExpansionContext) -> Result<String> {
    match user {
        None => context
            .home()
            .map(|p| p.display().to_string())
            .ok_or_else(|| ExpansionError::UndefinedVariable("HOME".to_string())),
        Some("+") => context
            .get_var_or_env("PWD")
            .ok_or_else(|| ExpansionError::UndefinedVariable("PWD".to_string())),
        Some("-") => context
            .get_var_or_env("OLDPWD")
            .ok_or_else(|| ExpansionError::UndefinedVariable("OLDPWD".to_string())),
        Some(username) => expand_user_home(username),
    }
}

/// Get a user's home directory
///
fn expand_user_home(username: &str) -> Result<String> {
    use nix::unistd::User;

    let user = User::from_name(username)
        .map_err(|e| ExpansionError::InvalidParameter(format!("~{}: {}", username, e)))?
        .ok_or_else(|| ExpansionError::InvalidParameter(format!("~{}: unknown user", username)))?;

    user.dir.to_str().map(String::from).ok_or_else(|| {
        ExpansionError::InvalidParameter(format!(
            "~{}: home directory is not valid UTF-8",
            username
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_tilde_expands_to_home() {
        let mut context = ExpansionContext::new();
        // Set a known HOME
        let home = PathBuf::from("/home/testuser");
        context.set_env("HOME", &home.display().to_string());

        let result = expand_tilde(None, &context).unwrap();
        assert!(result.contains("home") || result.contains("Users"));
    }

    #[test]
    fn test_tilde_user_expands_to_their_home() {
        let context = ExpansionContext::new();
        let result = expand_tilde(Some("root"), &context).unwrap();
        assert!(result.starts_with('/'));
    }

    #[test]
    fn test_tilde_unknown_user_errors() {
        let context = ExpansionContext::new();
        let result = expand_tilde(Some("no-such-user-colony-test"), &context);
        assert!(result.is_err());
    }
}
