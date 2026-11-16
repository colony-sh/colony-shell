//! Named secrets (API keys) in the OS keychain, one item per name.
//!
//! An item's service is [`SERVICE`] and its account is the secret's name, such as
//! `ANTHROPIC_API_KEY`. [`lookup`] prefers the environment variable of the same name, so a
//! shell that exports a key keeps working as before; the keychain is the fallback, which
//! is what the `colony-events` daemon started at login uses.
//!
//! Only macOS has a keychain backend so far. Elsewhere [`get`] finds nothing and [`set`]
//! refuses, rather than pretend to store a key that is gone when the process exits.

use crate::{Result, SecureError};

/// The keychain service every secret is stored under.
pub const SERVICE: &str = "colony";

/// Whether this build can store secrets in an OS keychain.
pub const AVAILABLE: bool = cfg!(target_os = "macos");

/// The value for `name`: its environment variable when that is set and not empty, else
/// the keychain item. Keychain errors count as not found.
pub fn lookup(name: &str) -> Option<String> {
    pick(std::env::var(name).ok(), || get(name).ok().flatten())
}

fn pick(env: Option<String>, stored: impl FnOnce() -> Option<String>) -> Option<String> {
    env.filter(|v| !v.is_empty()).or_else(stored)
}

/// A secret's name: upper-case letters, digits and `_`, starting with a letter.
pub fn check_name(name: &str) -> Result<()> {
    let ok = name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(SecureError::Keychain(format!(
            "bad name '{name}': use A-Z, 0-9 and _, starting with a letter"
        )))
    }
}

/// The stored value for `name`, if any.
pub fn get(name: &str) -> Result<Option<String>> {
    check_name(name)?;
    if !AVAILABLE {
        return Ok(None);
    }
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecureError::Keychain(e.to_string())),
    }
}

/// Store `value` under `name`, replacing any older value.
pub fn set(name: &str, value: &str) -> Result<()> {
    check_name(name)?;
    unavailable()?;
    if value.is_empty() {
        return Err(SecureError::Keychain("empty value".to_string()));
    }
    entry(name)?
        .set_password(value)
        .map_err(|e| SecureError::Keychain(e.to_string()))
}

/// Remove `name`. `Ok(false)` when nothing was stored.
pub fn delete(name: &str) -> Result<bool> {
    check_name(name)?;
    unavailable()?;
    match entry(name)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(SecureError::Keychain(e.to_string())),
    }
}

fn unavailable() -> Result<()> {
    if AVAILABLE {
        Ok(())
    } else {
        Err(SecureError::Keychain(
            "no OS keychain backend in this build (macOS only so far); \
             set the variable in the environment instead"
                .to_string(),
        ))
    }
}

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| SecureError::Keychain(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_wins_over_the_keychain() {
        let stored = || Some("from-keychain".to_string());
        assert_eq!(
            pick(Some("from-env".into()), stored).as_deref(),
            Some("from-env")
        );
        assert_eq!(
            pick(Some(String::new()), stored).as_deref(),
            Some("from-keychain")
        );
        assert_eq!(pick(None, stored).as_deref(), Some("from-keychain"));
        assert_eq!(pick(None, || None), None);
    }

    #[test]
    fn names_are_checked_before_the_keychain_is_touched() {
        for bad in ["", "lower", "1ABC", "A-B", "A B", &"A".repeat(65)] {
            assert!(check_name(bad).is_err(), "{bad:?}");
            assert!(get(bad).is_err(), "{bad:?}");
            assert!(set(bad, "v").is_err(), "{bad:?}");
        }
        assert!(check_name("ANTHROPIC_API_KEY").is_ok());
    }

    #[test]
    fn an_empty_value_is_refused() {
        assert!(set("COLONY_TEST_KEY", "").is_err());
    }
}
