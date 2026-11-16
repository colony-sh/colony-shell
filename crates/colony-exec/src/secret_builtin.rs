//! `colony secret`: store API keys in the OS keychain, where the daemon can read them.
//!
//! ```text
//! colony secret set <NAME>   read the value from the terminal (not echoed) or stdin
//! colony secret has <NAME>   exit 0 if stored, 1 if not; never prints the value
//! colony secret rm <NAME>    remove it
//! ```

use std::io::{BufRead, IsTerminal, Write};

use colony_secure::secrets;

use crate::builtins::BuiltinResult;
use crate::Result;

/// `colony secret ...`
pub fn colony_secret(args: &[String]) -> Result<BuiltinResult> {
    let (sub, name) = match args {
        [sub, name] => (sub.as_str(), name.as_str()),
        _ => return Ok(fail(2, "usage: colony secret set|has|rm <NAME>")),
    };
    if let Err(e) = secrets::check_name(name) {
        return Ok(fail(2, &e.to_string()));
    }
    let outcome = match sub {
        "set" => read_value(name).and_then(|value| {
            secrets::set(name, &value).map_err(|e| e.to_string())?;
            eprintln!(
                "colony secret: stored {name} in the keychain (service '{}')",
                secrets::SERVICE
            );
            Ok(0)
        }),
        "has" => secrets::get(name)
            .map(|v| {
                if v.is_some() {
                    println!("{name}: stored");
                    0
                } else {
                    println!("{name}: not stored");
                    1
                }
            })
            .map_err(|e| e.to_string()),
        "rm" => secrets::delete(name)
            .map(|removed| {
                if removed {
                    eprintln!("colony secret: removed {name}");
                    0
                } else {
                    eprintln!("colony secret: {name} was not stored");
                    1
                }
            })
            .map_err(|e| e.to_string()),
        _ => return Ok(fail(2, "usage: colony secret set|has|rm <NAME>")),
    };
    Ok(match outcome {
        Ok(0) => BuiltinResult::Success(0),
        Ok(code) => BuiltinResult::Failure(code),
        Err(e) => fail(1, &e),
    })
}

/// The secret's value: typed at the terminal with echo off, or the first line of stdin.
fn read_value(name: &str) -> std::result::Result<String, String> {
    let stdin = std::io::stdin();
    let value = if stdin.is_terminal() {
        eprint!("{name}: ");
        let _ = std::io::stderr().flush();
        let _quiet = NoEcho::enable();
        let mut line = String::new();
        stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        eprintln!();
        line
    } else {
        let mut line = String::new();
        stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        line
    };
    let value = value.trim_end_matches(['\n', '\r']).to_string();
    if value.is_empty() {
        return Err("empty value, nothing stored".to_string());
    }
    Ok(value)
}

/// Terminal echo off on stdin until dropped.
struct NoEcho {
    saved: Option<libc::termios>,
}

impl NoEcho {
    fn enable() -> Self {
        // SAFETY: zeroed termios is a valid out-parameter for tcgetattr.
        let mut term: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: tcgetattr/tcsetattr on stdin with a valid termios pointer.
        let saved = unsafe {
            if libc::tcgetattr(libc::STDIN_FILENO, &mut term) != 0 {
                return Self { saved: None };
            }
            let saved = term;
            term.c_lflag &= !libc::ECHO;
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term);
            saved
        };
        Self { saved: Some(saved) }
    }
}

impl Drop for NoEcho {
    fn drop(&mut self) {
        if let Some(saved) = self.saved {
            // SAFETY: restores the attributes read in `enable` on the same descriptor.
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
            }
        }
    }
}

fn fail(code: i32, msg: &str) -> BuiltinResult {
    eprintln!("colony secret: {msg}");
    BuiltinResult::Failure(code)
}
