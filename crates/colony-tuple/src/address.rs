//! `colony://` addresses: a name for everything a person or an agent can point at.
//!
//! An address is a name, not a permission. The first slice knows these scopes:
//!
//! ```text
//! colony://local/agent/lead-miner                   a named agent
//! colony://local/agent/lead-miner/run/3             one of its runs
//! colony://local/agent/lead-miner/run/3/output/1    one output of that run
//! colony://local/agent/lead-miner/run/3/diff        the run's diff
//! colony://local/agent/lead-miner/run/3/ask/1       a question that run asked you
//! colony://local/view/reports                       a viewport
//! colony://local/event/42                           one event in the log
//! ```
//!
//! `local` is this machine. Other hosts parse, but nothing resolves them yet.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// The scheme every address starts with.
pub const SCHEME: &str = "colony://";

/// The host name for this machine.
pub const LOCAL: &str = "local";

/// Why a string is not an address.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddressError {
    #[error("not a colony address (must start with {SCHEME}): {0}")]
    Scheme(String),

    #[error("bad name '{0}': use a-z, 0-9 and '-', starting with a letter, at most 63 characters")]
    Name(String),

    #[error("bad number '{0}'")]
    Number(String),

    #[error("unknown address path '{0}'")]
    Path(String),
}

/// A parsed `colony://` address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    /// The machine whose daemon holds it. [`LOCAL`] is this one.
    pub host: String,
    /// What it names on that machine.
    pub scope: Scope,
}

/// What an address names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// A named agent.
    Agent(String),
    /// One run of a named agent. Runs count up from 1 per agent.
    Run { agent: String, run: u64 },
    /// One output of a run. Outputs count up from 1 per run.
    Output { agent: String, run: u64, n: u64 },
    /// A run's diff against its scratch worktree.
    Diff { agent: String, run: u64 },
    /// One question a run asked. Asks count up from 1 per run.
    Ask { agent: String, run: u64, n: u64 },
    /// A viewport.
    View(String),
    /// One event in the log, by sequence number.
    Event(i64),
}

impl Address {
    /// An address on this machine.
    pub fn local(scope: Scope) -> Self {
        Self {
            host: LOCAL.to_string(),
            scope,
        }
    }

    /// Whether this address is on this machine.
    pub fn is_local(&self) -> bool {
        self.host == LOCAL
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}/", self.host)?;
        match &self.scope {
            Scope::Agent(name) => write!(f, "agent/{name}"),
            Scope::Run { agent, run } => write!(f, "agent/{agent}/run/{run}"),
            Scope::Output { agent, run, n } => write!(f, "agent/{agent}/run/{run}/output/{n}"),
            Scope::Diff { agent, run } => write!(f, "agent/{agent}/run/{run}/diff"),
            Scope::Ask { agent, run, n } => write!(f, "agent/{agent}/run/{run}/ask/{n}"),
            Scope::View(name) => write!(f, "view/{name}"),
            Scope::Event(seq) => write!(f, "event/{seq}"),
        }
    }
}

impl FromStr for Address {
    type Err = AddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s
            .strip_prefix(SCHEME)
            .ok_or_else(|| AddressError::Scheme(s.to_string()))?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let mut parts = rest.split('/');
        let host = name(parts.next().unwrap_or_default())?;
        let path: Vec<&str> = parts.collect();

        let scope = match path.as_slice() {
            ["agent", a] => Scope::Agent(name(a)?),
            ["agent", a, "run", r] => Scope::Run {
                agent: name(a)?,
                run: number(r)?,
            },
            ["agent", a, "run", r, "output", n] => Scope::Output {
                agent: name(a)?,
                run: number(r)?,
                n: number(n)?,
            },
            ["agent", a, "run", r, "diff"] => Scope::Diff {
                agent: name(a)?,
                run: number(r)?,
            },
            ["agent", a, "run", r, "ask", n] => Scope::Ask {
                agent: name(a)?,
                run: number(r)?,
                n: number(n)?,
            },
            ["view", v] => Scope::View(name(v)?),
            ["event", e] => Scope::Event(
                e.parse()
                    .ok()
                    .filter(|seq: &i64| *seq > 0)
                    .ok_or_else(|| AddressError::Number((*e).to_string()))?,
            ),
            _ => return Err(AddressError::Path(path.join("/"))),
        };
        Ok(Self { host, scope })
    }
}

/// Check a host, agent or view name.
fn name(s: &str) -> Result<String, AddressError> {
    let ok = s.len() <= 63
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(s.to_string())
    } else {
        Err(AddressError::Name(s.to_string()))
    }
}

/// Check a run or output number: 1 or more, no sign, no leading zeros.
fn number(s: &str) -> Result<u64, AddressError> {
    let bad = || AddressError::Number(s.to_string());
    if s.starts_with('0') || s.starts_with('+') {
        return Err(bad());
    }
    s.parse().map_err(|_| bad())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scope_round_trips() {
        let cases = [
            "colony://local/agent/lead-miner",
            "colony://local/agent/lead-miner/run/3",
            "colony://local/agent/lead-miner/run/3/output/1",
            "colony://local/agent/lead-miner/run/12/diff",
            "colony://local/agent/lead-miner/run/3/ask/2",
            "colony://local/view/reports",
            "colony://buildbox/event/42",
        ];
        for s in cases {
            let addr: Address = s.parse().unwrap();
            assert_eq!(addr.to_string(), s);
        }
    }

    #[test]
    fn trailing_slash_is_allowed() {
        let addr: Address = "colony://local/view/reports/".parse().unwrap();
        assert_eq!(addr, Address::local(Scope::View("reports".into())));
        assert!(addr.is_local());
    }

    #[test]
    fn bad_addresses_are_refused() {
        let cases = [
            ("http://local/view/reports", "not a colony address"),
            ("colony://Local/view/reports", "bad name"),
            ("colony://local/agent/Lead", "bad name"),
            ("colony://local/agent/a b", "bad name"),
            ("colony://local/agent/x/run/0", "bad number"),
            ("colony://local/agent/x/run/01", "bad number"),
            ("colony://local/agent/x/run/-1", "bad number"),
            ("colony://local/event/0", "bad number"),
            (
                "colony://local/agent/x/run/1/output",
                "unknown address path",
            ),
            ("colony://local", "unknown address path"),
            ("colony://local/agent/x//", "unknown address path"),
        ];
        for (s, want) in cases {
            let err = s.parse::<Address>().unwrap_err().to_string();
            assert!(err.contains(want), "{s}: {err}");
        }
    }
}
