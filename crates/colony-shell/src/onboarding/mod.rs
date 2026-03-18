//! Colony Shell onboarding system.
//!
//! Provides first-run detection, a setup wizard, shell config import,
//! and profile conversion for new users migrating from bash/zsh.
//!
//! The wizard runs automatically on first launch when `~/.colony/config.toml`
//! does not exist. It can also be re-run via `colony onboard`.

pub mod converter;
pub mod importer;
pub mod wizard;

/// Check if this is a first run and onboarding should be shown.
///
/// Returns `true` if `~/.colony/config.toml` does not exist,
/// meaning the user has not completed onboarding.
pub fn check_first_run() -> bool {
    wizard::is_first_run()
}
