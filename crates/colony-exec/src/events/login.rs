//! Start the `colony-events` daemon at login: a launchd agent on macOS, a systemd user
//! unit on Linux.
//!
//! `on --daemon install` writes the file and loads it; `--print` shows it and changes
//! nothing. The file holds no API keys: a key in a plist or unit file would sit on disk in
//! plain text. The daemon reads keys from the OS keychain (`colony secret set`). Provider
//! settings that are not secret (`COLONY_MODEL_PROVIDER`, `COLONY_MODEL_NAME`,
//! `OLLAMA_HOST`) are copied from the installing shell.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The launchd label and systemd unit name.
const LABEL: &str = "sh.colony.events";

/// Environment the daemon keeps from the shell that installs it. No secrets.
const KEPT_ENV: [&str; 5] = [
    "PATH",
    // Where the viewport listens: the phone reaches it on the private network's address.
    "COLONY_VIEW_ADDR",
    "COLONY_MODEL_PROVIDER",
    "COLONY_MODEL_NAME",
    "OLLAMA_HOST",
];

/// A login item: where it goes and what it says.
#[derive(Debug, PartialEq, Eq)]
pub struct LoginItem {
    pub path: PathBuf,
    pub contents: String,
}

/// The login item for this platform, running `binary` with `env`.
pub fn login_item(binary: &Path, home: &Path, env: &[(String, String)]) -> LoginItem {
    if cfg!(target_os = "macos") {
        launchd_plist(binary, home, env)
    } else {
        systemd_unit(binary, home, env)
    }
}

/// The environment to keep, from the current process.
pub fn kept_env() -> Vec<(String, String)> {
    KEPT_ENV
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_string(), v)))
        .collect()
}

fn launchd_plist(binary: &Path, home: &Path, env: &[(String, String)]) -> LoginItem {
    let log = home.join(".colony/events/daemon.log");
    let env: String = env
        .iter()
        .map(|(k, v)| format!("        <key>{}</key><string>{}</string>\n", xml(k), xml(v)))
        .collect();
    let contents = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key><false/>
    </dict>
    <key>EnvironmentVariables</key>
    <dict>
{env}    </dict>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        xml(&binary.display().to_string()),
        log = xml(&log.display().to_string()),
    );
    LoginItem {
        path: home.join(format!("Library/LaunchAgents/{LABEL}.plist")),
        contents,
    }
}

fn systemd_unit(binary: &Path, home: &Path, env: &[(String, String)]) -> LoginItem {
    let env: String = env
        .iter()
        .map(|(k, v)| format!("Environment=\"{k}={}\"\n", systemd_quote(v)))
        .collect();
    let contents = format!(
        "[Unit]\nDescription=Colony event router and heartbeats\n\n\
         [Service]\nExecStart=\"{}\" daemon\nRestart=on-failure\n{env}\n\
         [Install]\nWantedBy=default.target\n",
        systemd_quote(&binary.display().to_string()),
    );
    LoginItem {
        path: home.join(format!(".config/systemd/user/{LABEL}.service")),
        contents,
    }
}

/// Write the login item and load it, so the daemon runs now and at every login.
pub fn install(item: &LoginItem) -> Result<(), String> {
    if let Some(parent) = item.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&item.path, &item.contents)
        .map_err(|e| format!("{}: {e}", item.path.display()))?;
    if cfg!(target_os = "macos") {
        let domain = format!("gui/{}", current_uid());
        // Unload an older copy first; it is fine if there is none.
        let _ = run("launchctl", &["bootout", &format!("{domain}/{LABEL}")]);
        run(
            "launchctl",
            &["bootstrap", &domain, &item.path.display().to_string()],
        )
    } else {
        run("systemctl", &["--user", "daemon-reload"])?;
        run(
            "systemctl",
            &["--user", "enable", "--now", &format!("{LABEL}.service")],
        )
    }
}

/// Unload the login item and remove its file.
pub fn uninstall(item: &LoginItem) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let _ = run(
            "launchctl",
            &["bootout", &format!("gui/{}/{LABEL}", current_uid())],
        );
    } else {
        let _ = run(
            "systemctl",
            &["--user", "disable", "--now", &format!("{LABEL}.service")],
        );
    }
    match std::fs::remove_file(&item.path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "not installed ({} does not exist)",
            item.path.display()
        )),
        Err(e) => Err(format!("{}: {e}", item.path.display())),
    }
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Quote a value for a double-quoted systemd string: backslash, quote and `%` (a specifier).
fn systemd_quote(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_item_keeps_the_viewport_address_and_no_keys() {
        assert!(
            KEPT_ENV.contains(&"COLONY_VIEW_ADDR"),
            "a reboot must not lose phone access"
        );
        assert!(KEPT_ENV
            .iter()
            .all(|k| !k.contains("KEY") && !k.contains("TOKEN")));
    }

    fn env() -> Vec<(String, String)> {
        vec![
            ("PATH".into(), "/usr/bin:/a&b".into()),
            ("OLLAMA_HOST".into(), "127.0.0.1:9 %h \"x\"".into()),
        ]
    }

    #[test]
    fn the_plist_runs_the_daemon_at_load_and_restarts_it_on_failure() {
        let item = launchd_plist(
            Path::new("/opt/c<1>/colony-events"),
            Path::new("/Users/u"),
            &env(),
        );
        assert_eq!(
            item.path,
            Path::new("/Users/u/Library/LaunchAgents/sh.colony.events.plist")
        );
        let c = &item.contents;
        assert!(c.contains(
            "<string>/opt/c&lt;1&gt;/colony-events</string>\n        <string>daemon</string>"
        ));
        assert!(c.contains("<key>RunAtLoad</key><true/>"));
        assert!(c.contains("<key>SuccessfulExit</key><false/>"));
        assert!(c.contains("<key>PATH</key><string>/usr/bin:/a&amp;b</string>"));
        assert!(c.contains("/Users/u/.colony/events/daemon.log"));
    }

    #[test]
    fn the_unit_runs_the_daemon_and_quotes_its_values() {
        let item = systemd_unit(
            Path::new("/opt/c/colony-events"),
            Path::new("/home/u"),
            &env(),
        );
        assert_eq!(
            item.path,
            Path::new("/home/u/.config/systemd/user/sh.colony.events.service")
        );
        let c = &item.contents;
        assert!(c.contains("ExecStart=\"/opt/c/colony-events\" daemon\n"));
        assert!(c.contains("Restart=on-failure"));
        assert!(c.contains(r#"Environment="OLLAMA_HOST=127.0.0.1:9 %%h \"x\"""#));
        assert!(c.contains("WantedBy=default.target"));
    }

    #[test]
    fn no_secret_is_kept() {
        assert!(KEPT_ENV
            .iter()
            .all(|k| !k.contains("KEY") && !k.contains("TOKEN")));
    }
}
