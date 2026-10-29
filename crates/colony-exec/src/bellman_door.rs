//! The local Bellman bridge door: a Unix socket the session's Bellman bridge serves.
//!
//! The terminal holds no Bellman token. The bridge does, and answers anyone running as this
//! user on its socket (folder 0700, socket 0600). The door is found as:
//!
//! 1. a live `<bridge pid>.sock` in `$BELLMAN_DOOR_ROOT`. Colony Nerve gives each terminal a
//!    folder of its own, so a bridge started there is this terminal's. The default folder is
//!    shared by every bridge on the machine, so it is never scanned this way.
//! 2. `owner-<Claude Code pid>.sock`, a link the bridge makes for its parent, found by walking
//!    up our own parents, as the Bellman hooks do (`findDoor` in bellman `src/door.ts`).
//!
//! Requests go through `curl --unix-socket`, as the Phoenix calls go through curl.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// How far up the parent pids to look for an owner link (bellman uses 8 as well).
const MAX_DEPTH: usize = 8;

/// Why a door read did not give an answer.
#[derive(Debug, PartialEq, Eq)]
pub enum DoorError {
    /// `BELLMAN_ROOM` is unset or empty.
    NotInRoom,
    /// No socket in the door folder, and no owner link up the parent pids.
    NotFound(PathBuf),
    /// The door answered 404 for the room: its bridge holds no seat there.
    NoSeat(String),
    /// curl could not reach the socket.
    Unreachable(PathBuf),
    /// Any other answer the door gave.
    Http(u16, String),
}

impl fmt::Display for DoorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInRoom => write!(f, "not in a Bellman room (BELLMAN_ROOM not set)"),
            Self::NotFound(root) => write!(
                f,
                "no Bellman bridge door found (looked in {} and up the parent processes for owner-<pid>.sock); is Claude Code running with the Bellman plugin?",
                root.display()
            ),
            Self::NoSeat(room) => write!(f, "the Bellman bridge holds no seat in room {room}"),
            Self::Unreachable(socket) => {
                write!(f, "the Bellman door at {} is not answering", socket.display())
            }
            Self::Http(status, body) => write!(f, "the Bellman door answered {status}: {body}"),
        }
    }
}

/// The room this terminal is in: `BELLMAN_ROOM`.
pub fn room() -> Result<String, DoorError> {
    std::env::var("BELLMAN_ROOM")
        .ok()
        .filter(|r| !r.is_empty())
        .ok_or(DoorError::NotInRoom)
}

/// The folder a bridge uses when `BELLMAN_DOOR_ROOT` is unset.
fn default_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude/bellman/door")
}

/// Find this terminal's door socket.
pub fn find() -> Result<PathBuf, DoorError> {
    let explicit = std::env::var_os("BELLMAN_DOOR_ROOT")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from);
    let root = explicit.clone().unwrap_or_else(default_root);
    let start = nix::unistd::getppid().as_raw().unsigned_abs();
    find_in(explicit.as_deref(), &root, start, parent_pid, is_alive)
        .ok_or(DoorError::NotFound(root))
}

/// The discovery itself, with the process table handed in so tests can fake it.
fn find_in(
    explicit_root: Option<&Path>,
    root: &Path,
    start_pid: u32,
    parent_of: impl Fn(u32) -> Option<u32>,
    alive: impl Fn(u32) -> bool,
) -> Option<PathBuf> {
    // ponytail: first live <pid>.sock wins; with two bridges in one terminal, pick by seat if that shows.
    let by_pid = explicit_root.and_then(|dir| {
        let mut live: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_suffix(".sock"))
                    .and_then(|pid| pid.parse::<u32>().ok())
                    .is_some_and(&alive)
            })
            .collect();
        live.sort();
        live.into_iter().next()
    });
    if by_pid.is_some() {
        return by_pid;
    }

    let mut pid = Some(start_pid);
    for _ in 0..MAX_DEPTH {
        let Some(p) = pid else { break };
        // exists() follows the link, so one left pointing at a closed socket is skipped.
        let link = root.join(format!("owner-{p}.sock"));
        if link.exists() {
            return Some(link);
        }
        pid = parent_of(p);
    }
    None
}

/// The parent of `pid`, from `ps`, or `None` at init or on any failure.
fn parent_pid(pid: u32) -> Option<u32> {
    let out = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let ppid: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    (ppid > 1).then_some(ppid)
}

/// Whether a process with this pid exists (EPERM means it does, as someone else).
fn is_alive(pid: u32) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(raw), None) {
        Ok(()) | Err(nix::errno::Errno::EPERM) => true,
        Err(_) => false,
    }
}

/// `GET <path>` from the door at `socket`, as JSON. A 404 on a room route is `NoSeat(room)`.
pub fn get(socket: &Path, path: &str, room: &str) -> Result<Value, DoorError> {
    let output = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}",
            "--unix-socket",
        ])
        .arg(socket)
        .arg(format!("http://door{path}"))
        .output()
        .map_err(|_| DoorError::Unreachable(socket.to_path_buf()))?;
    if !output.status.success() {
        return Err(DoorError::Unreachable(socket.to_path_buf()));
    }
    let out = String::from_utf8_lossy(&output.stdout);
    let (status, body) =
        split_status(&out).ok_or_else(|| DoorError::Unreachable(socket.to_path_buf()))?;
    match status {
        200 => serde_json::from_str(body).map_err(|_| DoorError::Http(200, clean(body, 200))),
        404 => Err(DoorError::NoSeat(room.to_string())),
        other => Err(DoorError::Http(other, clean(body, 200))),
    }
}

/// curl's `-w "\n%{http_code}"` output split into the status and the body before it.
fn split_status(out: &str) -> Option<(u16, &str)> {
    let (body, code) = out.rsplit_once('\n')?;
    Some((code.trim().parse().ok()?, body))
}

/// A room id as one URL path segment.
pub fn segment(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A peer's string as one line of at most `max` characters, safe to print to a terminal:
/// controls (ESC among them, so no escape sequence survives) and bidi/zero-width marks dropped.
pub fn clean(text: &str, max: usize) -> String {
    let plain: String = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    c,
                    '\u{61c}' | '\u{200b}' | '\u{200e}' | '\u{200f}' | '\u{2060}' | '\u{feff}'
                )
                && !('\u{202a}'..='\u{202e}').contains(c)
                && !('\u{2066}'..='\u{2069}').contains(c)
        })
        .collect();
    let line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let mut cut: String = line.chars().take(max.saturating_sub(1)).collect();
    cut.push('\u{2026}');
    cut
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("colony-door-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_find_live_pid_socket_in_explicit_root() {
        let dir = scratch("pid");
        let _dead = UnixListener::bind(dir.join("111.sock")).unwrap();
        let _live = UnixListener::bind(dir.join("222.sock")).unwrap();
        let found = find_in(Some(&dir), &dir, 1, |_| None, |pid| pid == 222);
        assert_eq!(found, Some(dir.join("222.sock")));
    }

    #[test]
    fn test_find_owner_link_up_the_parents() {
        let dir = scratch("owner");
        let door = dir.join("999.sock");
        let _listener = UnixListener::bind(&door).unwrap();
        std::os::unix::fs::symlink(&door, dir.join("owner-30.sock")).unwrap();
        // 10 -> 20 -> 30 (Claude Code) -> init. The bridge itself (999) is not alive here.
        let parents = |pid| match pid {
            10 => Some(20),
            20 => Some(30),
            _ => None,
        };
        let found = find_in(Some(&dir), &dir, 10, parents, |_| false);
        assert_eq!(found, Some(dir.join("owner-30.sock")));
    }

    #[test]
    fn test_default_root_is_not_scanned_for_pid_sockets() {
        let dir = scratch("shared");
        let _other = UnixListener::bind(dir.join("222.sock")).unwrap();
        assert_eq!(find_in(None, &dir, 10, |_| None, |_| true), None);
    }

    #[test]
    fn test_dangling_owner_link_is_skipped() {
        let dir = scratch("dangling");
        std::os::unix::fs::symlink(dir.join("gone.sock"), dir.join("owner-10.sock")).unwrap();
        assert_eq!(find_in(None, &dir, 10, |_| None, |_| true), None);
    }

    #[test]
    fn test_split_status() {
        assert_eq!(split_status("{\"a\":1}\n200"), Some((200, "{\"a\":1}")));
        assert_eq!(
            split_status("{\"error\":\"not_found\"}\n404"),
            Some((404, "{\"error\":\"not_found\"}"))
        );
        assert_eq!(split_status("garbage"), None);
    }

    #[test]
    fn test_segment_escapes_everything_but_unreserved() {
        assert_eq!(segment("qs_01dc-8a.~"), "qs_01dc-8a.~");
        assert_eq!(segment("a/b?c d"), "a%2Fb%3Fc%20d");
    }

    #[test]
    fn test_clean_strips_escapes_and_bidi() {
        assert_eq!(clean("\u{1b}[31mred\u{1b}[0m\nline", 50), "[31mred[0m line");
        assert_eq!(clean("ab\u{202e}cd\u{200b}", 50), "abcd");
        assert_eq!(clean("abcdef", 4), "abc\u{2026}");
    }
}
