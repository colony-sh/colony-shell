//! A run's container: where a named agent's commands execute, through Colony's shell.
//!
//! Each run gets three podman objects, named after the run, and removes them when it ends:
//!
//! | Object | What it is |
//! |---|---|
//! | `colony-<agent>-<run>` | the run's container, from the agent's image |
//! | `colony-<agent>-<run>-net` | an internal network: no route out |
//! | `colony-<agent>-<run>-proxy` | `colony-events proxy` on that network and the outside one, letting through only the agent's granted hosts |
//!
//! The agent's workdir is mounted read-only at `/base` and copied into `/work` in the
//! container's own layer, its scratch worktree: everything a run writes stays in the
//! container, and its diff is `/base` against `/work`.
//!
//! Both containers run with a user namespace (their root is uid 200000 in the podman VM),
//! every capability dropped, `no-new-privileges`, and pid and memory limits.
// ponytail: copy, not an overlay mount: a rootful podman's overlay volumes are root-owned and
// unwritable from a user namespace; use `:O` if the machine goes rootless.

use std::path::Path;
use std::process::{Command, Output, Stdio};

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
}

/// One request the proxy saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub host: String,
    pub port: u16,
    pub allowed: bool,
}

impl Sandbox {
    /// The names a run's objects take.
    pub fn named(agent: &str, run: u64) -> Self {
        let container = format!("colony-{agent}-{run}");
        Self {
            network: format!("{container}-net"),
            proxy: format!("{container}-proxy"),
            container,
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
        let sandbox = Self::named(agent, run);
        sandbox.remove();
        sandbox.create(image, workdir, grants).map_err(|e| {
            sandbox.remove();
            e
        })?;
        Ok(sandbox)
    }

    fn create(&self, image: &str, workdir: &Path, grants: &[String]) -> Result<(), String> {
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
        .arg(format!("{}:/base:ro", workdir.display()));
        for var in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy"] {
            cmd.arg("-e").arg(format!("{var}={proxy_url}"));
        }
        run(cmd.args(["-e", "COLONY_SANDBOX=1", image, "sleep", "infinity"]))?;
        run(podman().args(["exec", &self.container, "cp", "-R", "/base/.", "/work/"]))?;
        Ok(())
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

    /// Remove the container, the proxy and the network. Missing ones are fine.
    pub fn remove(&self) {
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
        let s = Sandbox::named("lead-miner", 3);
        assert_eq!(s.container, "colony-lead-miner-3");
        assert_eq!(s.network, "colony-lead-miner-3-net");
        assert_eq!(s.proxy, "colony-lead-miner-3-proxy");
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
