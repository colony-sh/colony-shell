//! A run's container: where a named agent's commands execute, through Colony's shell.
//!
//! Each run gets three podman objects, named after the run and the event log it belongs
//! to (`<log>` is a hash of the log's path, so two logs on one machine never share a
//! name), and removes them when it ends:
//!
//! | Object | What it is |
//! |---|---|
//! | `colony-<log>-<agent>-<run>` | the run's container, from the agent's image |
//! | `colony-<log>-<agent>-<run>-net` | an internal network: no route out |
//! | `colony-<log>-<agent>-<run>-proxy` | `colony-events proxy` on that network and the outside one, letting through only the agent's granted hosts |
//!
//! The agent's workdir, less every secret file ([`SECRET_NAMES`], [`SECRET_SUFFIXES`],
//! [`SECRET_DIRS`], and `.env.*` but `.env.example`), is copied to a staging directory
//! (`~/.colony/runs/<container>`), mounted read-only at `/base`, and copied into `/work` in
//! the container's own layer, its scratch worktree: everything a run writes stays in the
//! container, its diff is `/base` against `/work`, and no secret file ever enters it. The
//! skipped paths (never their contents) are recorded with the container's start.
//!
//! Both containers run with a user namespace (their root is uid 200000 in the podman VM),
//! every capability dropped, `no-new-privileges`, and pid and memory limits.
// ponytail: copy, not an overlay mount: a rootful podman's overlay volumes are root-owned and
// unwritable from a user namespace; use `:O` if the machine goes rootless.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// The image `agent new --image run` means.
pub const DEFAULT_IMAGE: &str = "localhost/colony-run:latest";

/// The port the proxy listens on in the VM.
pub const PROXY_PORT: u16 = 3128;

/// The flags every container a run makes is started with.
const HARDENING: [&str; 11] = [
    // A run never pulls: the daemon reaches no registry without a grant.
    "--pull=never",
    "--uidmap=0:200000:65536",
    "--gidmap=0:200000:65536",
    "--cap-drop=ALL",
    "--security-opt=no-new-privileges",
    "--pids-limit=512",
    "--memory=1g",
    "--label=colony.run=1",
    "--log-driver=k8s-file",
    "--stop-timeout=1",
    "--init",
];

/// The podman command: `COLONY_PODMAN`, else `podman` on PATH.
pub fn podman() -> Command {
    Command::new(std::env::var("COLONY_PODMAN").unwrap_or_else(|_| "podman".to_string()))
}

fn run(cmd: &mut Command) -> Result<Output, String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("podman: {e}"))?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(format!(
            "podman: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// A run's container, its network and its proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    pub container: String,
    pub network: String,
    pub proxy: String,
    /// Workdir paths left out as secrets, relative to it.
    pub skipped: Vec<String>,
}

/// One request the proxy saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub host: String,
    pub port: u16,
    pub allowed: bool,
}

impl Sandbox {
    /// The names a run's objects take, for this machine's event log.
    pub fn named(agent: &str, run: u64) -> Self {
        Self::named_in(&colony_tuple::EventLog::default_path(), agent, run)
    }

    /// The names a run's objects take, for the log at `log_path`.
    pub fn named_in(log_path: &Path, agent: &str, run: u64) -> Self {
        let hash = Sha256::digest(log_path.display().to_string().as_bytes());
        let log = hex::encode(&hash[..4]);
        let container = format!("colony-{log}-{agent}-{run}");
        Self {
            network: format!("{container}-net"),
            proxy: format!("{container}-proxy"),
            container,
            skipped: Vec::new(),
        }
    }

    /// Start a run's container from `image`, with `workdir` copied into `/work` and only
    /// `grants` reachable. Anything left from an earlier attempt at the same run is
    /// removed first. On failure nothing is left behind.
    pub fn start(
        agent: &str,
        run: u64,
        image: &str,
        workdir: &Path,
        grants: &[String],
    ) -> Result<Self, String> {
        let mut sandbox = Self::named(agent, run);
        sandbox.remove();
        sandbox.skipped = sandbox.create(image, workdir, grants).map_err(|e| {
            sandbox.remove();
            e
        })?;
        Ok(sandbox)
    }

    /// Where the run's filtered copy of its workdir is staged.
    fn staging(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".colony/runs")
            .join(&self.container)
    }

    /// Returns the paths left out as secrets.
    fn create(
        &self,
        image: &str,
        workdir: &Path,
        grants: &[String],
    ) -> Result<Vec<String>, String> {
        let base = self.staging();
        let mut skipped = Vec::new();
        copy_without_secrets(workdir, &base, Path::new(""), &mut skipped)
            .map_err(|e| format!("staging {}: {e}", workdir.display()))?;
        run(podman().args(["network", "create", "--internal", &self.network]))?;
        run(podman()
            .args(["run", "-d", "--name", &self.proxy, "--network", "podman"])
            .args(HARDENING)
            .args([image, "colony-events", "proxy", "--listen"])
            .arg(format!("0.0.0.0:{PROXY_PORT}"))
            .args(["--allow", &grants.join(",")]))?;
        run(podman().args(["network", "connect", &self.network, &self.proxy]))?;
        let proxy_url = format!("http://{}:{PROXY_PORT}", self.proxy);
        let mut cmd = podman();
        cmd.args([
            "run",
            "-d",
            "--name",
            &self.container,
            "--network",
            &self.network,
        ])
        .args(HARDENING)
        .arg("-v")
        .arg(format!("{}:/base:ro", base.display()));
        for var in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy"] {
            cmd.arg("-e").arg(format!("{var}={proxy_url}"));
        }
        run(cmd.args(["-e", "COLONY_SANDBOX=1", image, "sleep", "infinity"]))?;
        run(podman().args(["exec", &self.container, "cp", "-R", "/base/.", "/work/"]))?;
        Ok(skipped)
    }

    /// What the run changed in its scratch worktree, as a unified diff. Empty if nothing.
    pub fn diff(&self) -> Result<String, String> {
        let out = podman()
            .args([
                "exec",
                "-w",
                "/",
                &self.container,
                "diff",
                "-ruN",
                "base",
                "work",
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("podman: {e}"))?;
        // diff exits 1 when the trees differ.
        match out.status.code() {
            Some(0 | 1) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            _ => Err(format!(
                "diff: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        }
    }

    /// Every request the proxy saw, in order.
    pub fn verdicts(&self) -> Vec<Verdict> {
        let Ok(out) = run(podman().args(["logs", &self.proxy])) else {
            return Vec::new();
        };
        parse_verdicts(&String::from_utf8_lossy(&out.stdout))
    }

    /// Whether the run's container is still running. A podman that cannot be asked counts
    /// as yes: only a container podman says is gone or stopped has died.
    pub fn is_running(&self) -> bool {
        let Ok(out) = podman()
            .args(["inspect", "-f", "{{.State.Running}}", &self.container])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return true;
        };
        out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true"
    }

    /// Watch the container from another thread: if it stops, `cancel` is set, so the run
    /// stops too. [`Watch::stop`] ends the watch.
    pub fn watch(&self, cancel: Arc<AtomicBool>) -> Watch {
        let (tx, rx) = mpsc::channel::<()>();
        let sandbox = self.clone();
        let handle = std::thread::spawn(move || loop {
            match rx.recv_timeout(WATCH_EVERY) {
                Err(mpsc::RecvTimeoutError::Timeout) if !sandbox.is_running() => {
                    cancel.store(true, Ordering::SeqCst);
                    return true;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                _ => return false,
            }
        });
        Watch { tx, handle }
    }

    /// Remove the container, the proxy, the network and the staged workdir. Missing ones
    /// are fine.
    pub fn remove(&self) {
        let _ = std::fs::remove_dir_all(self.staging());
        let _ = podman()
            .args(["rm", "-f", "-t", "0", &self.container, &self.proxy])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status();
        let _ = podman()
            .args(["network", "rm", "-f", &self.network])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status();
    }
}

/// How often [`Sandbox::watch`] asks podman.
const WATCH_EVERY: Duration = Duration::from_secs(1);

/// A watch on a run's container.
pub struct Watch {
    tx: mpsc::Sender<()>,
    handle: std::thread::JoinHandle<bool>,
}

impl Watch {
    /// End the watch. `true` if the container died while it was watched.
    pub fn stop(self) -> bool {
        drop(self.tx);
        self.handle.join().unwrap_or(false)
    }
}

/// File names a run never sees, at any depth.
pub const SECRET_NAMES: &[&str] = &[
    ".env",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "id_ecdsa_sk",
    "id_ed25519_sk",
    "credentials.json",
    ".npmrc",
    ".netrc",
    ".pypirc",
];
/// File name endings a run never sees.
pub const SECRET_SUFFIXES: &[&str] = &[".pem", ".key", ".p12", ".pfx"];
/// Directories a run never sees, with all they hold.
pub const SECRET_DIRS: &[&str] = &[".aws", ".ssh", ".gnupg"];

/// Whether an entry is a secret a run must never see.
fn is_secret(name: &std::ffi::OsStr, is_dir: bool) -> bool {
    let name = name.to_string_lossy();
    if is_dir {
        return SECRET_DIRS.contains(&name.as_ref());
    }
    SECRET_NAMES.contains(&name.as_ref())
        || (name.starts_with(".env.") && name != ".env.example")
        || SECRET_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Copy `from` to a fresh `to`, leaving out secrets at any depth; each one's path below the
/// workdir (`rel`) goes in `skipped`. Symlinks are copied as links, not followed.
// ponytail: copies the whole tree each run (node_modules, target too); add an exclude list or a
// cache when that is slow.
fn copy_without_secrets(
    from: &Path,
    to: &Path,
    rel: &Path,
    skipped: &mut Vec<String>,
) -> std::io::Result<()> {
    if rel.as_os_str().is_empty() {
        let _ = std::fs::remove_dir_all(to);
    }
    std::fs::create_dir_all(to)?;
    let mut entries: Vec<_> = std::fs::read_dir(from)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let kind = entry.file_type()?;
        let here = rel.join(entry.file_name());
        if is_secret(&entry.file_name(), kind.is_dir()) {
            skipped.push(here.display().to_string());
            continue;
        }
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if kind.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
        } else if kind.is_dir() {
            copy_without_secrets(&src, &dst, &here, skipped)?;
        } else if kind.is_file() {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn parse_verdicts(text: &str) -> Vec<Verdict> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| {
            Some(Verdict {
                host: v["host"].as_str()?.to_string(),
                port: u16::try_from(v["port"].as_u64()?).ok()?,
                allowed: v["allowed"].as_bool()?,
            })
        })
        .collect()
}

/// Check a host grant: a host name, or `*.` and a domain.
pub fn check_grant(grant: &str) -> Result<String, String> {
    let g = grant.trim().to_ascii_lowercase();
    let name = g.strip_prefix("*.").unwrap_or(&g);
    let ok = !name.is_empty()
        && name.len() <= 253
        && name.contains('.')
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    if ok {
        Ok(g)
    } else {
        Err(format!(
            "bad host '{grant}' (use example.com or *.example.com)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runs_objects_are_named_after_it() {
        let s = Sandbox::named_in(Path::new("/h/a.db"), "lead-miner", 3);
        let log = &s.container["colony-".len().."colony-".len() + 8];
        assert_eq!(s.container, format!("colony-{log}-lead-miner-3"));
        assert_eq!(s.network, format!("{}-net", s.container));
        assert_eq!(s.proxy, format!("{}-proxy", s.container));
        let other = Sandbox::named_in(Path::new("/h/b.db"), "lead-miner", 3);
        assert_ne!(s.container, other.container, "two logs never share a name");
    }

    #[test]
    fn the_staged_copy_has_no_secret_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        for d in ["app", ".ssh", ".aws", "certs"] {
            std::fs::create_dir_all(src.join(d)).unwrap();
        }
        let secrets = [
            ".env",
            ".env.local",
            "app/.env",
            "certs/server.pem",
            "certs/tls.key",
            "id_ed25519",
            "app/credentials.json",
            ".npmrc",
            ".ssh/config",
            ".aws/credentials",
        ];
        let kept = [
            "keep.txt",
            "app/main.rs",
            ".envrc",
            ".env.example",
            "id_ed25519.pub",
            "keyboard.rs",
        ];
        for f in secrets.iter().chain(&kept) {
            std::fs::write(src.join(f), "x").unwrap();
        }
        std::os::unix::fs::symlink("/etc/hosts", src.join("link")).unwrap();
        let to = dir.path().join("to");
        let mut skipped = Vec::new();
        copy_without_secrets(&src, &to, Path::new(""), &mut skipped).unwrap();
        for f in kept {
            assert!(to.join(f).is_file(), "{f} was left out");
        }
        for f in secrets {
            assert!(!to.join(f).exists(), "{f} was copied");
        }
        assert!(to
            .join("link")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        skipped.sort();
        assert_eq!(
            skipped,
            [
                ".aws",
                ".env",
                ".env.local",
                ".npmrc",
                ".ssh",
                "app/.env",
                "app/credentials.json",
                "certs/server.pem",
                "certs/tls.key",
                "id_ed25519"
            ]
        );
    }

    #[test]
    fn verdicts_are_read_from_the_proxy_log() {
        let log = "{\"host\":\"example.com\",\"port\":443,\"allowed\":false}\nnoise\n{\"host\":\"a.b\",\"port\":80,\"allowed\":true}\n";
        assert_eq!(
            parse_verdicts(log),
            [
                Verdict {
                    host: "example.com".into(),
                    port: 443,
                    allowed: false
                },
                Verdict {
                    host: "a.b".into(),
                    port: 80,
                    allowed: true
                },
            ]
        );
    }

    #[test]
    fn grants_are_host_names() {
        assert_eq!(
            check_grant("News.YCombinator.com").unwrap(),
            "news.ycombinator.com"
        );
        assert_eq!(check_grant("*.reddit.com").unwrap(), "*.reddit.com");
        for bad in [
            "",
            "*",
            "*.",
            "localhost",
            "a b.com",
            "http://x.com",
            "x.com:443",
            "*.*.x.com",
        ] {
            assert!(check_grant(bad).is_err(), "{bad}");
        }
    }
}
