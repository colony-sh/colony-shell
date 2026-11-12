//! `colony log` and `colony open`: read the machine's event log and resolve `colony://` addresses.
//!
//! ```text
//! colony log [--last N | --after SEQ]   print events, oldest first (default: the last 50)
//! colony open <address>                 show what an address names
//! ```
//!
//! `colony open` on an output or diff prints its content exactly when stdout is a pipe or a
//! file, and with terminal controls removed when stdout is a terminal. On an agent, run or
//! viewport it prints every event about it and what is under it. On an event it prints that
//! event and its data.

use std::io::{IsTerminal, Write};

use colony_tuple::{Address, Event, EventLog, Scope};

use crate::bellman_door::{clean, clean_block};
use crate::builtins::BuiltinResult;
use crate::Result;

/// How many events `colony log` prints with no options.
const DEFAULT_LAST: usize = 50;

/// `colony log [--last N | --after SEQ]`
pub fn colony_log(args: &[String]) -> Result<BuiltinResult> {
    let query = match args {
        [] => Query::Last(DEFAULT_LAST),
        [flag, n] if flag == "--last" => match n.parse() {
            Ok(n) => Query::Last(n),
            Err(_) => return usage_error("colony log", &format!("bad count '{n}'")),
        },
        [flag, seq] if flag == "--after" => match seq.parse() {
            Ok(seq) => Query::After(seq),
            Err(_) => return usage_error("colony log", &format!("bad sequence number '{seq}'")),
        },
        _ => return usage_error("colony log", "usage: colony log [--last N | --after SEQ]"),
    };

    let events = EventLog::open_default().and_then(|log| match query {
        Query::Last(n) => log.last(n),
        Query::After(seq) => log.after(seq, usize::MAX),
    });
    match events {
        Ok(events) => {
            events.iter().for_each(|ev| println!("{}", line(ev)));
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => fail("colony log", &e.to_string()),
    }
}

/// `colony open <address>`
pub fn colony_open(args: &[String]) -> Result<BuiltinResult> {
    let [raw] = args else {
        return usage_error("colony open", "usage: colony open <colony://address>");
    };
    let addr: Address = match raw.parse() {
        Ok(addr) => addr,
        Err(e) => return usage_error("colony open", &e.to_string()),
    };
    if !addr.is_local() {
        return fail(
            "colony open",
            &format!("{addr}: only local addresses resolve so far"),
        );
    }
    let log = match EventLog::open_default() {
        Ok(log) => log,
        Err(e) => return fail("colony open", &e.to_string()),
    };

    let shown = match &addr.scope {
        Scope::Event(seq) => log.get(*seq).map(|ev| ev.map(|ev| show_event(&ev))),
        Scope::Output { .. } | Scope::Diff { .. } => log
            .latest(&addr)
            .map(|ev| ev.and_then(|ev| show_content(&ev))),
        Scope::View(name) => log
            .of_kind(crate::agents::REPORT_POSTED, 1000)
            .map(|mut events| {
                events.retain(|ev| ev.data["view"] == name.as_str());
                events.reverse();
                events.iter().for_each(|ev| println!("{}", line(ev)));
                (!events.is_empty()).then_some(())
            }),
        Scope::Agent(_) | Scope::Run { .. } => log.about(&addr).map(|events| {
            events.iter().for_each(|ev| println!("{}", line(ev)));
            (!events.is_empty()).then_some(())
        }),
    };
    match shown {
        Ok(Some(())) => Ok(BuiltinResult::Success(0)),
        Ok(None) => fail("colony open", &format!("nothing at {addr}")),
        Err(e) => fail("colony open", &e.to_string()),
    }
}

enum Query {
    Last(usize),
    After(i64),
}

/// One event on one line: `#seq  time  actor  kind  subject  data`.
fn line(ev: &Event) -> String {
    let time = chrono::DateTime::from_timestamp_millis(ev.at_ms)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    let data = match &ev.data {
        serde_json::Value::Object(map) if map.is_empty() => String::new(),
        // serde_json escapes every control character, so this is safe to print.
        data => format!("  {}", clean(&data.to_string(), 200)),
    };
    format!(
        "#{}  {}  {}  {}  {}{}",
        ev.seq,
        time,
        clean(&ev.actor, 40),
        clean(&ev.kind, 40),
        ev.subject
            .as_deref()
            .map_or_else(|| "-".to_string(), |s| clean(s, 120)),
        data
    )
}

fn show_event(ev: &Event) {
    println!("{}", line(ev));
    if let Some(cause) = ev.cause {
        println!("cause: colony://local/event/{cause}");
    }
    let pretty = serde_json::to_string_pretty(&ev.data).unwrap_or_default();
    println!("{}", clean_block(&pretty));
}

/// Print an output's or diff's `content`. `None` when the event carries none.
fn show_content(ev: &Event) -> Option<()> {
    let content = ev.data.get("content")?.as_str()?;
    let mut out = std::io::stdout();
    if out.is_terminal() {
        println!("{}", clean_block(content));
    } else {
        // Exact bytes for pipes and files; a failed write (closed pipe) is not an error here.
        let _ = out.write_all(content.as_bytes());
    }
    Some(())
}

fn usage_error(cmd: &str, msg: &str) -> Result<BuiltinResult> {
    eprintln!("{cmd}: {msg}");
    Ok(BuiltinResult::Failure(2))
}

fn fail(cmd: &str, msg: &str) -> Result<BuiltinResult> {
    eprintln!("{cmd}: {msg}");
    Ok(BuiltinResult::Failure(1))
}
