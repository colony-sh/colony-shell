//! Named agents: an agent with a name that outlives any shell.
//!
//! An agent is a charter, grants (its security mode and the one directory its tools may
//! read), budgets and a state, all held in the machine's event log as events about
//! `colony://local/agent/<name>`. [`Agents::from_log`] folds them back. Each run is a beat
//! of the agent's heartbeat, run by the `colony-events` daemon from a checklist written to
//! `~/.colony/agents/<name>/HEARTBEAT.md` ([`checklist_path`]), so the existing heartbeat
//! scheduler and runner do the work.
//!
//! ```text
//! agent new <name> --every <dur> --charter <text> [--in <dir>] [--mode notify|active|full]
//!                  [--timeout <dur>] [--daily-tokens <n>]
//! agent list
//! agent pause|resume|retire <name>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use colony_tuple::{Address, Event, EventLog, Scope};
use serde::{Deserialize, Serialize};

use crate::builtins::BuiltinResult;
use crate::heartbeat::{parse_duration, SecurityMode, HEARTBEAT_ANSWERED, HEARTBEAT_FIRED};

/// Event: an agent was created. Subject: the agent. Data: its [`AgentSpec`].
pub const AGENT_CREATED: &str = "AgentCreated";
/// Event: an agent's state changed. Subject: the agent. Data: `{from, to, run?}`.
pub const AGENT_STATE_CHANGED: &str = "AgentStateChanged";
/// Event: a run produced an output. Subject: the output's address. Data: `{content}`, and
/// `{tool, input}` for a tool call's result.
pub const OUTPUT_RECORDED: &str = "OutputRecorded";
/// Event: a run reported to a viewport. Subject: the run. Data: its [`Report`].
pub const REPORT_POSTED: &str = "ReportPosted";

/// The viewport every agent reports to, for now.
pub const REPORTS_VIEW: &str = "reports";

/// How a report wants to be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Fine,
    Attention,
    Blocked,
    /// It has an ask waiting on you. Nothing asks yet.
    NeedsYou,
}

/// What an agent sends you after a run that acted or missed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub headline: String,
    pub status: ReportStatus,
    pub body: String,
    /// `colony://` addresses of the outputs behind it.
    pub evidence: Vec<String>,
    /// Questions for you. Empty until asks land.
    #[serde(default)]
    pub asks: Vec<serde_json::Value>,
    /// The viewport it is posted to.
    pub view: String,
}

/// Longest headline kept, in characters.
const HEADLINE_MAX: usize = 120;

impl Report {
    /// The report for a run's model text. `missed` marks a run that did not finish.
    /// The model may set the headline and status with `HEADLINE:` and `STATUS:` lines;
    /// otherwise the headline is the first WARN or ALERT finding, or the first line.
    pub fn from_output(text: &str, missed: bool, evidence: Vec<String>) -> Self {
        let tagged = |tag: &str| {
            text.lines().find_map(|l| {
                let l = l.trim();
                l.get(..tag.len())
                    .filter(|p| p.eq_ignore_ascii_case(tag))
                    .map(|_| l[tag.len()..].trim().to_string())
            })
        };
        let headline = tagged("HEADLINE:")
            .or_else(|| tagged("ALERT:"))
            .or_else(|| tagged("WARN:"))
            .or_else(|| {
                text.lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(String::from)
            })
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "The run wrote nothing".to_string());
        let status = match tagged("STATUS:").map(|s| s.to_ascii_lowercase()) {
            _ if missed => ReportStatus::Blocked,
            Some(s) if s == "fine" => ReportStatus::Fine,
            Some(s) if s == "blocked" => ReportStatus::Blocked,
            _ => ReportStatus::Attention,
        };
        Self {
            headline: cut(&headline, HEADLINE_MAX),
            status,
            body: text.trim().to_string(),
            evidence,
            asks: Vec::new(),
            view: REPORTS_VIEW.to_string(),
        }
    }
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('\u{2026}');
    out
}

/// What an agent is told to do, and with what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    /// Its standing task.
    pub charter: String,
    /// How often its heartbeat fires.
    pub every_secs: u64,
    /// Which tools it is offered.
    pub mode: SecurityMode,
    /// The only directory its tools may read.
    pub workdir: PathBuf,
    /// Longest one run may take.
    pub timeout_secs: u64,
    /// Most tool calls in one run.
    pub max_tool_calls: u32,
    /// Most tokens in one run.
    pub token_budget: u32,
    /// Most tokens across a day's runs (UTC day). A beat past it is missed.
    pub daily_tokens: u64,
    /// What it carries between runs. Empty until note approval lands.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Where an agent is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Sleeping,
    Running,
    /// Waiting on an answer from you. Nothing parks a run yet.
    Parked,
    Paused,
    Retired,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Sleeping => "sleeping",
            Self::Running => "running",
            Self::Parked => "parked",
            Self::Paused => "paused",
            Self::Retired => "retired",
        })
    }
}

/// One agent, as the log says it is.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRecord {
    pub name: String,
    pub spec: AgentSpec,
    pub state: AgentState,
    /// When it was created, Unix ms.
    pub created_ms: i64,
    /// When its last beat fired, Unix ms.
    pub last_fired_ms: Option<i64>,
    /// How many beats have fired.
    pub runs: u64,
}

impl AgentRecord {
    /// The agent's address.
    pub fn address(&self) -> Address {
        agent_address(&self.name)
    }

    /// When its next beat fires, Unix ms: one interval after the last beat, or after its
    /// creation. `None` when it is paused, parked or retired.
    pub fn next_beat_ms(&self) -> Option<i64> {
        if !matches!(self.state, AgentState::Sleeping | AgentState::Running) {
            return None;
        }
        let every = i64::try_from(self.spec.every_secs.saturating_mul(1000)).unwrap_or(i64::MAX);
        Some(
            self.last_fired_ms
                .unwrap_or(self.created_ms)
                .saturating_add(every),
        )
    }
}

/// Every agent on this machine, folded from the log.
#[derive(Debug, Default)]
pub struct Agents {
    by_name: BTreeMap<String, AgentRecord>,
}

impl Agents {
    /// Replay the whole log.
    pub fn from_log(log: &EventLog) -> Result<Self, String> {
        let mut agents = Self::default();
        let mut last = 0;
        loop {
            let page = log.after(last, 1000).map_err(|e| e.to_string())?;
            let Some(tail) = page.last() else {
                return Ok(agents);
            };
            last = tail.seq;
            page.iter().for_each(|ev| agents.apply(ev));
        }
    }

    /// Fold one event in. Events about no agent, or an unknown one, are ignored.
    /// A state change applies only from the state it names, so a late
    /// running-to-sleeping never undoes a pause.
    pub fn apply(&mut self, ev: &Event) {
        let Some(scope) = ev
            .subject
            .as_deref()
            .and_then(|s| s.parse::<Address>().ok())
            .map(|a| a.scope)
        else {
            return;
        };
        match (ev.kind.as_str(), scope) {
            (AGENT_CREATED, Scope::Agent(name)) => {
                let Ok(spec) = serde_json::from_value(ev.data.clone()) else {
                    return;
                };
                self.by_name.entry(name.clone()).or_insert(AgentRecord {
                    name,
                    spec,
                    state: AgentState::Sleeping,
                    created_ms: ev.at_ms,
                    last_fired_ms: None,
                    runs: 0,
                });
            }
            (AGENT_STATE_CHANGED, Scope::Agent(name)) => {
                let field = |k: &str| serde_json::from_value::<AgentState>(ev.data[k].clone()).ok();
                let (Some(from), Some(to), Some(agent)) =
                    (field("from"), field("to"), self.by_name.get_mut(&name))
                else {
                    return;
                };
                if agent.state == from {
                    agent.state = to;
                }
            }
            (HEARTBEAT_FIRED, Scope::Run { agent, .. }) => {
                if let Some(agent) = self.by_name.get_mut(&agent) {
                    agent.runs += 1;
                    agent.last_fired_ms = Some(ev.at_ms);
                }
            }
            _ => {}
        }
    }

    pub fn get(&self, name: &str) -> Option<&AgentRecord> {
        self.by_name.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AgentRecord> {
        self.by_name.values()
    }
}

/// `colony://local/agent/<name>`.
pub fn agent_address(name: &str) -> Address {
    Address::local(Scope::Agent(name.to_string()))
}

/// `colony://local/agent/<name>/run/<run>`.
pub fn run_address(name: &str, run: u64) -> Address {
    Address::local(Scope::Run {
        agent: name.to_string(),
        run,
    })
}

/// Tokens the agent's answered beats used since the start of today (UTC).
pub fn tokens_today(log: &EventLog, name: &str) -> Result<u64, String> {
    let midnight = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map_or(0, |t| t.and_utc().timestamp_millis());
    let events = log.about(&agent_address(name)).map_err(|e| e.to_string())?;
    Ok(events
        .iter()
        .filter(|ev| ev.kind == HEARTBEAT_ANSWERED && ev.at_ms >= midnight)
        .filter_map(|ev| ev.data["tokens"].as_u64())
        .sum())
}

/// The checklist the daemon's heartbeat runs for this agent.
pub fn checklist_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".colony/agents")
        .join(name)
        .join("HEARTBEAT.md")
}

/// The agent's checklist: its limits as frontmatter, its charter as the body.
pub fn checklist(spec: &AgentSpec) -> String {
    format!(
        "---\ninterval: {}s\nmode: {}\ntimeout: {}s\nmax_tool_calls: {}\ntoken_budget: {}\nworkdir: {}\n---\n\n{}\n",
        spec.every_secs,
        spec.mode,
        spec.timeout_secs,
        spec.max_tool_calls,
        spec.token_budget,
        spec.workdir.display(),
        spec.charter.trim(),
    )
}

/// Write the agent's checklist, replacing any older one.
pub fn write_checklist(name: &str, spec: &AgentSpec) -> std::io::Result<PathBuf> {
    let path = checklist_path(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, checklist(spec))?;
    Ok(path)
}

/// Format a Unix ms time for people, in local time.
pub fn local_time(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "-".to_string())
}

// ---------------------------------------------------------------------------
// The `agent` builtin
// ---------------------------------------------------------------------------

/// The `agent` subcommands this module handles. The REPL's own (`status`, `model`, ...)
/// drive the chat agent and never reach here.
pub const SUBCOMMANDS: [&str; 6] = ["new", "list", "pause", "resume", "retire", "help"];

/// `agent new|list|pause|resume|retire`. `actor` is who the log records.
pub fn builtin_agent(args: &[String], actor: &str, cwd: &Path) -> BuiltinResult {
    let sub = args.first().map_or("list", String::as_str);
    let rest = args.get(1..).unwrap_or_default();
    let outcome = EventLog::open_default()
        .map_err(|e| e.to_string())
        .and_then(|log| match sub {
            "new" => cmd_new(&log, rest, actor, cwd),
            "list" => cmd_list(&log),
            "pause" => change_state(&log, rest, actor, AgentState::Paused),
            "resume" => change_state(&log, rest, actor, AgentState::Sleeping),
            "retire" => change_state(&log, rest, actor, AgentState::Retired),
            "help" | "--help" | "-h" => {
                print_help();
                Ok(())
            }
            other => Err(format!("unknown subcommand '{other}' (try: agent help)")),
        });
    match outcome {
        Ok(()) => BuiltinResult::Success(0),
        Err(e) => {
            eprintln!("agent {sub}: {e}");
            BuiltinResult::Failure(1)
        }
    }
}

fn print_help() {
    println!(
        "agent new <name> --every <dur> --charter <text> [--in <dir>] [--mode notify|active|full]"
    );
    println!("                 [--timeout <dur>] [--daily-tokens <n>]");
    println!("agent list                     every agent, its state and its next beat");
    println!("agent pause|resume <name>      stop and restart its heartbeat");
    println!("agent retire <name>            stop it for good; the name stays taken");
}

fn cmd_new(log: &EventLog, args: &[String], actor: &str, cwd: &Path) -> Result<(), String> {
    let Some((name, flags)) = args.split_first() else {
        return Err("missing name".to_string());
    };
    let address: Address = format!("colony://local/agent/{name}")
        .parse()
        .map_err(|e: colony_tuple::AddressError| e.to_string())?;
    if Agents::from_log(log)?.get(name).is_some() {
        return Err(format!("{address} already exists"));
    }

    let mut spec = AgentSpec {
        charter: String::new(),
        every_secs: 0,
        mode: SecurityMode::Notify,
        workdir: cwd.to_path_buf(),
        timeout_secs: 600,
        max_tool_calls: 20,
        token_budget: 50_000,
        daily_tokens: 200_000,
        notes: Vec::new(),
    };
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let duration = || {
            parse_duration(value)
                .filter(|s| *s > 0)
                .ok_or_else(|| format!("bad duration '{value}' (use 30s, 10m, 1h, 1d)"))
        };
        match flag.as_str() {
            "--every" => spec.every_secs = duration()?,
            "--timeout" => spec.timeout_secs = duration()?,
            "--charter" => spec.charter = value.clone(),
            "--in" => spec.workdir = cwd.join(value),
            "--mode" => spec.mode = SecurityMode::from_str_lossy(value),
            "--daily-tokens" => {
                spec.daily_tokens = value
                    .parse()
                    .map_err(|_| format!("bad token count '{value}'"))?;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if spec.every_secs == 0 {
        return Err("missing --every".to_string());
    }
    if spec.charter.trim().is_empty() {
        return Err("missing --charter".to_string());
    }
    spec.workdir = spec
        .workdir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", spec.workdir.display()))?;
    if spec.workdir.display().to_string().contains('\n') {
        return Err("the working directory's path has a line break".to_string());
    }
    // The checklist is the charter's text; it must not end its own frontmatter.
    spec.charter = spec.charter.replace("\n---", "\n- - -");

    write_checklist(name, &spec).map_err(|e| e.to_string())?;
    let data = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
    log.append(actor, AGENT_CREATED, Some(&address), None, data)
        .map_err(|e| e.to_string())?;
    crate::events::ensure_daemon()?;

    let agent = Agents::from_log(log)?
        .get(name)
        .cloned()
        .ok_or("created, but the log does not show it")?;
    println!(
        "{address}: {}, next beat {}",
        agent.state,
        agent
            .next_beat_ms()
            .map_or_else(|| "-".to_string(), local_time)
    );
    Ok(())
}

fn cmd_list(log: &EventLog) -> Result<(), String> {
    let agents = Agents::from_log(log)?;
    if agents.iter().next().is_none() {
        println!("No agents.");
        return Ok(());
    }
    println!(
        "{:<20} {:<9} {:>7} {:<17} {:>5}",
        "NAME", "STATE", "EVERY", "NEXT BEAT", "RUNS"
    );
    for a in agents.iter() {
        println!(
            "{:<20} {:<9} {:>7} {:<17} {:>5}",
            a.name,
            a.state,
            short_duration(a.spec.every_secs),
            a.next_beat_ms().map_or_else(|| "-".to_string(), local_time),
            a.runs
        );
    }
    Ok(())
}

fn change_state(
    log: &EventLog,
    args: &[String],
    actor: &str,
    to: AgentState,
) -> Result<(), String> {
    let [name] = args else {
        return Err("usage: agent pause|resume|retire <name>".to_string());
    };
    let agents = Agents::from_log(log)?;
    let agent = agents
        .get(name)
        .ok_or_else(|| format!("no agent named {name}"))?;
    let from = agent.state;
    let allowed = match to {
        AgentState::Paused => matches!(
            from,
            AgentState::Sleeping | AgentState::Running | AgentState::Parked
        ),
        AgentState::Sleeping => from == AgentState::Paused,
        AgentState::Retired => from != AgentState::Retired,
        _ => false,
    };
    if !allowed {
        return Err(format!("{name} is {from}"));
    }
    let data = serde_json::json!({ "from": from, "to": to });
    log.append(
        actor,
        AGENT_STATE_CHANGED,
        Some(&agent.address()),
        None,
        data,
    )
    .map_err(|e| e.to_string())?;
    if to == AgentState::Sleeping {
        crate::events::ensure_daemon()?;
    }
    println!("{}: {to}", agent.address());
    Ok(())
}

fn short_duration(secs: u64) -> String {
    match secs {
        s if s % 86_400 == 0 => format!("{}d", s / 86_400),
        s if s % 3600 == 0 => format!("{}h", s / 3600),
        s if s % 60 == 0 => format!("{}m", s / 60),
        s => format!("{s}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec() -> AgentSpec {
        AgentSpec {
            charter: "find threads".into(),
            every_secs: 60,
            mode: SecurityMode::Notify,
            workdir: "/w".into(),
            timeout_secs: 30,
            max_tool_calls: 5,
            token_budget: 100,
            daily_tokens: 1000,
            notes: vec![],
        }
    }

    fn log() -> (EventLog, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (EventLog::open(&dir.path().join("m.db")).unwrap(), dir)
    }

    fn state(log: &EventLog, from: AgentState, to: AgentState) {
        log.append(
            "t",
            AGENT_STATE_CHANGED,
            Some(&agent_address("a")),
            None,
            json!({"from": from, "to": to}),
        )
        .unwrap();
    }

    #[test]
    fn a_new_agent_sleeps_with_its_next_beat_one_interval_away() {
        let (log, _d) = log();
        let seq = log
            .append(
                "t",
                AGENT_CREATED,
                Some(&agent_address("a")),
                None,
                serde_json::to_value(spec()).unwrap(),
            )
            .unwrap();
        let created = log.get(seq).unwrap().unwrap().at_ms;
        let agents = Agents::from_log(&log).unwrap();
        let a = agents.get("a").unwrap();
        assert_eq!(a.state, AgentState::Sleeping);
        assert_eq!(a.next_beat_ms(), Some(created + 60_000));

        log.append(
            "daemon",
            HEARTBEAT_FIRED,
            Some(&run_address("a", 1)),
            None,
            json!({}),
        )
        .unwrap();
        let agents = Agents::from_log(&log).unwrap();
        let a = agents.get("a").unwrap();
        assert_eq!(a.runs, 1);
        assert_eq!(a.next_beat_ms(), Some(a.last_fired_ms.unwrap() + 60_000));
    }

    #[test]
    fn a_state_change_applies_only_from_the_state_it_names() {
        let (log, _d) = log();
        log.append(
            "t",
            AGENT_CREATED,
            Some(&agent_address("a")),
            None,
            serde_json::to_value(spec()).unwrap(),
        )
        .unwrap();
        state(&log, AgentState::Sleeping, AgentState::Running);
        state(&log, AgentState::Running, AgentState::Paused);
        // The run ends after the pause; its running-to-sleeping must not undo it.
        state(&log, AgentState::Running, AgentState::Sleeping);
        let agents = Agents::from_log(&log).unwrap();
        assert_eq!(agents.get("a").unwrap().state, AgentState::Paused);
        assert_eq!(agents.get("a").unwrap().next_beat_ms(), None);
    }

    #[test]
    fn tokens_today_sums_answered_beats() {
        let (log, _d) = log();
        for t in [10, 25] {
            log.append(
                "d",
                HEARTBEAT_ANSWERED,
                Some(&run_address("a", 1)),
                None,
                json!({"tokens": t}),
            )
            .unwrap();
        }
        log.append(
            "d",
            HEARTBEAT_ANSWERED,
            Some(&run_address("b", 1)),
            None,
            json!({"tokens": 99}),
        )
        .unwrap();
        assert_eq!(tokens_today(&log, "a").unwrap(), 35);
    }

    #[test]
    fn a_report_takes_its_headline_from_the_model_or_its_findings() {
        let ev = vec!["colony://local/agent/a/run/1/output/1".to_string()];
        let r = Report::from_output(
            "PASS: one\nWARN: two threads found\nmore",
            false,
            ev.clone(),
        );
        assert_eq!(r.headline, "two threads found");
        assert_eq!(r.status, ReportStatus::Attention);
        assert_eq!(r.evidence, ev);

        let r = Report::from_output(
            "Headline: Drafted 3 replies\nSTATUS: fine\nWARN: x",
            false,
            ev.clone(),
        );
        assert_eq!(
            (r.headline.as_str(), r.status),
            ("Drafted 3 replies", ReportStatus::Fine)
        );

        let r = Report::from_output("STATUS: fine", true, vec![]);
        assert_eq!(
            r.status,
            ReportStatus::Blocked,
            "a missed run is blocked whatever it says"
        );

        let r = Report::from_output("", false, vec![]);
        assert_eq!(r.headline, "The run wrote nothing");
        let long = "x".repeat(300);
        assert_eq!(
            Report::from_output(&long, false, vec![])
                .headline
                .chars()
                .count(),
            HEADLINE_MAX
        );
    }

    #[test]
    fn the_checklist_carries_the_limits_and_the_charter() {
        let text = checklist(&spec());
        let (config, _) = crate::heartbeat::parse_heartbeat(&text);
        assert_eq!(config.interval_secs, 60);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_tool_calls, 5);
        assert_eq!(config.token_budget, 100);
        assert_eq!(config.workdir.as_deref(), Some(Path::new("/w")));
        assert!(text.ends_with("---\n\nfind threads\n"));
    }
}
