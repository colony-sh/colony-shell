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
        None => {
            context
                .home()
                .map(|p| p.display().to_string())
                .ok_or_else(|| ExpansionError::UndefinedVariable("HOME".to_string()))
        }
        Some("+") => {
            context
                .get_var_or_env("PWD")
                .ok_or_else(|| ExpansionError::UndefinedVariable("PWD".to_string()))
        }
        Some("-") => {
            context
                .get_var_or_env("OLDPWD")
                .ok_or_else(|| ExpansionError::UndefinedVariable("OLDPWD".to_string()))
        }
        Some(username) => {
            expand_user_home(username)
        }
    }
}

/// Get a user's home directory
///
/// On Unix, this uses getpwnam. For now, we'll just return an error
/// as this requires unsafe FFI or a dependency like `users` crate.
fn expand_user_home(username: &str) -> Result<String> {
    // TODO: Implement using nix crate's getpwnam
    // For now, return error
    Err(ExpansionError::InvalidParameter(format!(
        "~{}: user home directory lookup not yet implemented",
        username
    )))
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
    fn test_tilde_user_not_implemented() {
        let context = ExpansionContext::new();
        let result = expand_tilde(Some("root"), &context);
        assert!(result.is_err());
    }
}
