//! flux - Process manager builtin for colony-shell
//!
//! Ported from https://github.com/VG-dev1/flux (MIT licensed).
//!
//! Default mode: process listing with structured output on fd 5 (JSON),
//! tuples on fd 4, and formatted text on stdout.
//!
//! TUI mode (--tui): Interactive ratatui-based process viewer/killer.

pub mod tui;

use crate::builtins::BuiltinResult;
use crate::redirect::{STDLLM, STDTOOL};
use crate::{ExecutionError, Result};
use colony_tuple::{Tuple, TupleSpace, Value};
use nix::sys::signal::Signal;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use sysinfo::System;
use terminal_size::{terminal_size, Width};

// ---------------------------------------------------------------------------
// Argument parsing (no clap -- builtins parse args manually)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortBy {
    Cpu,
    Mem,
    Pid,
    Name,
    Port,
}

#[derive(Debug)]
struct FluxArgs {
    filter: Option<String>,
    signal: String,
    sort: SortBy,
    ports: bool,
    port: Option<u16>,
    nuke: bool,
    resources: bool,
    tui: bool,
}

fn parse_flux_args(args: &[String]) -> std::result::Result<FluxArgs, String> {
    let mut fa = FluxArgs {
        filter: None,
        signal: "KILL".to_string(),
        sort: SortBy::Cpu,
        ports: false,
        port: None,
        nuke: false,
        resources: false,
        tui: false,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-f" | "--filter" => {
                i += 1;
                let val = args.get(i).ok_or("--filter requires a value")?;
                fa.filter = Some(val.clone());
            }
            "-s" | "--signal" => {
                i += 1;
                let val = args.get(i).ok_or("--signal requires a value")?;
                fa.signal = val.clone();
            }
            "--sort" => {
                i += 1;
                let val = args.get(i).ok_or("--sort requires a value")?;
                fa.sort = match val.to_lowercase().as_str() {
                    "cpu" => SortBy::Cpu,
                    "mem" => SortBy::Mem,
                    "pid" => SortBy::Pid,
                    "name" => SortBy::Name,
                    "port" => SortBy::Port,
                    other => return Err(format!("unknown sort field: {other}")),
                };
            }
            "--ports" => fa.ports = true,
            "--port" => {
                i += 1;
                let val = args.get(i).ok_or("--port requires a value")?;
                fa.port = Some(
                    val.parse::<u16>()
                        .map_err(|_| format!("invalid port number: {val}"))?,
                );
            }
            "--nuke" => fa.nuke = true,
            "--resources" => fa.resources = true,
            "--tui" => fa.tui = true,
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }

    Ok(fa)
}

// ---------------------------------------------------------------------------
// Process data types (shared between default and TUI modes)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory_mb: u64,
    pub port: Option<u16>,
    pub protocol: Option<String>,
}

#[derive(Serialize)]
struct JsonProcessEntry {
    pid: u32,
    name: String,
    cpu: f32,
    memory_mb: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
}

#[derive(Serialize)]
struct JsonSystemInfo {
    cpu_percent: f32,
    memory_used_mb: u64,
    memory_total_mb: u64,
}

#[derive(Serialize)]
struct JsonOutput {
    processes: Vec<JsonProcessEntry>,
    system: JsonSystemInfo,
}

// ---------------------------------------------------------------------------
// Port mapping (via `listeners` crate)
// ---------------------------------------------------------------------------

pub fn get_port_mappings() -> HashMap<u32, Vec<(u16, String)>> {
    let mut map: HashMap<u32, Vec<(u16, String)>> = HashMap::new();

    if let Ok(listeners) = listeners::get_all() {
        for listener in listeners {
            let port = listener.socket.port();
            let protocol = format!("{:?}", listener.protocol).to_uppercase();
            let entry = map.entry(listener.process.pid).or_default();
            if !entry
                .iter()
                .any(|(p, proto)| *p == port && proto == &protocol)
            {
                entry.push((port, protocol));
            }
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Process enumeration
// ---------------------------------------------------------------------------

pub fn refresh_processes(
    sys: &mut System,
    filter: Option<&str>,
    sort_by: SortBy,
) -> Vec<ProcessInfo> {
    sys.refresh_all();

    let cpu_count = sys.cpus().len() as f32;

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .filter_map(|(pid, proc_info)| {
            let name = proc_info.name().to_string_lossy().to_string();

            if let Some(f) = filter {
                if !name.to_lowercase().contains(&f.to_lowercase()) {
                    return None;
                }
            }

            Some(ProcessInfo {
                pid: pid.as_u32(),
                name,
                cpu: proc_info.cpu_usage() / cpu_count,
                memory_mb: proc_info.memory() / 1024 / 1024,
                port: None,
                protocol: None,
            })
        })
        .collect();

    sort_processes(&mut processes, sort_by);
    processes
}

pub fn refresh_processes_with_ports(
    sys: &mut System,
    filter: Option<&str>,
    port_filter: Option<u16>,
    sort_by: SortBy,
) -> Vec<ProcessInfo> {
    sys.refresh_all();

    let port_map = get_port_mappings();
    let cpu_count = sys.cpus().len() as f32;

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .flat_map(|(pid, proc_info)| {
            let pid_u32 = pid.as_u32();
            let Some(ports) = port_map.get(&pid_u32) else {
                return vec![];
            };

            let name = proc_info.name().to_string_lossy().to_string();

            if let Some(f) = filter {
                if !name.to_lowercase().contains(&f.to_lowercase()) {
                    return vec![];
                }
            }

            let cpu = proc_info.cpu_usage() / cpu_count;
            let memory_mb = proc_info.memory() / 1024 / 1024;

            ports
                .iter()
                .filter_map(|(port, protocol)| {
                    if let Some(target_port) = port_filter {
                        if *port != target_port {
                            return None;
                        }
                    }

                    Some(ProcessInfo {
                        pid: pid_u32,
                        name: name.clone(),
                        cpu,
                        memory_mb,
                        port: Some(*port),
                        protocol: Some(protocol.clone()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();

    sort_processes(&mut processes, sort_by);
    processes
}

pub fn sort_processes(processes: &mut [ProcessInfo], sort_by: SortBy) {
    match sort_by {
        SortBy::Cpu => processes.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortBy::Mem => processes.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb)),
        SortBy::Pid => processes.sort_by(|a, b| a.pid.cmp(&b.pid)),
        SortBy::Name => {
            processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        SortBy::Port => processes.sort_by(|a, b| a.port.cmp(&b.port)),
    }
}

// ---------------------------------------------------------------------------
// Signal parsing
// ---------------------------------------------------------------------------

pub fn parse_signal(signal_str: &str) -> std::result::Result<Signal, String> {
    let signal_str = signal_str.to_uppercase();
    let signal_str = signal_str.strip_prefix("SIG").unwrap_or(&signal_str);

    match signal_str {
        "KILL" | "9" => Ok(Signal::SIGKILL),
        "TERM" | "15" => Ok(Signal::SIGTERM),
        "INT" | "2" => Ok(Signal::SIGINT),
        "HUP" | "1" => Ok(Signal::SIGHUP),
        "QUIT" | "3" => Ok(Signal::SIGQUIT),
        "USR1" | "10" => Ok(Signal::SIGUSR1),
        "USR2" | "12" => Ok(Signal::SIGUSR2),
        "STOP" | "19" => Ok(Signal::SIGSTOP),
        "CONT" | "18" => Ok(Signal::SIGCONT),
        _ => Err(format!("unknown signal: {signal_str}")),
    }
}

// ---------------------------------------------------------------------------
// Kill processes
// ---------------------------------------------------------------------------

pub fn kill_processes(selected: &[ProcessInfo], signal: Signal) {
    use nix::sys::signal::kill as nix_kill;
    use nix::unistd::Pid as NixPid;

    for proc_info in selected {
        let result = nix_kill(NixPid::from_raw(proc_info.pid as i32), signal);
        if result.is_ok() {
            println!("Killed {} (PID: {})", proc_info.name, proc_info.pid);
        } else {
            eprintln!("Failed to kill {} (PID: {})", proc_info.name, proc_info.pid);
        }
    }
}

// ---------------------------------------------------------------------------
// Formatted text output helpers
// ---------------------------------------------------------------------------

fn calculate_name_width(ports_mode: bool) -> usize {
    let term_width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    let mut fixed = 6 + 7 + 7 + 9 + 4; // markers + spacing
    if ports_mode {
        fixed += 10;
    }

    term_width.saturating_sub(fixed).clamp(15, 80)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn print_header(ports_mode: bool, name_width: usize) {
    if ports_mode {
        println!(
            "{:<9} {:<7} {:<width$} {:>7} {:>9}",
            "PORT",
            "PID",
            "NAME",
            "CPU %",
            "MEMORY",
            width = name_width,
        );
    } else {
        println!(
            "{:<7} {:<width$} {:>7} {:>9}",
            "PID",
            "NAME",
            "CPU %",
            "MEMORY",
            width = name_width,
        );
    }
}

fn print_process(p: &ProcessInfo, ports_mode: bool, name_width: usize) {
    let display_name = truncate(&p.name, name_width);
    let cpu_str = format!("{:>6.1}%", p.cpu);
    let mem_str = format!("{:>6} MB", p.memory_mb);

    if ports_mode {
        let port_str = p
            .port
            .map(|port| {
                let proto = p.protocol.as_deref().unwrap_or("TCP");
                format!("{:<5} {:>3}", port, proto)
            })
            .unwrap_or_else(|| "          ".to_string());
        println!(
            "{:<9} {:<7} {:<width$} {} {}",
            port_str,
            p.pid,
            display_name,
            cpu_str,
            mem_str,
            width = name_width,
        );
    } else {
        println!(
            "{:<7} {:<width$} {} {}",
            p.pid,
            display_name,
            cpu_str,
            mem_str,
            width = name_width,
        );
    }
}

// ---------------------------------------------------------------------------
// Structured output (fd 5 JSON, fd 4 tuples)
// ---------------------------------------------------------------------------

fn emit_structured_output(sys: &System, processes: &[ProcessInfo]) {
    let json_output = JsonOutput {
        processes: processes
            .iter()
            .map(|p| JsonProcessEntry {
                pid: p.pid,
                name: p.name.clone(),
                cpu: p.cpu,
                memory_mb: p.memory_mb,
                port: p.port,
                protocol: p.protocol.clone(),
            })
            .collect(),
        system: JsonSystemInfo {
            cpu_percent: sys.global_cpu_usage(),
            memory_used_mb: sys.used_memory() / 1024 / 1024,
            memory_total_mb: sys.total_memory() / 1024 / 1024,
        },
    };

    // Write JSON to fd 5 (STDTOOL) -- best-effort, ignore EBADF if fd not open
    if let Ok(json_str) = serde_json::to_string(&json_output) {
        // SAFETY: We are writing to a known fd that may or may not be open.
        // If STDTOOL (fd 5) is not connected, the write will fail harmlessly.
        let mut fd5 = unsafe { std::fs::File::from_raw_fd(STDTOOL) };
        let _ = writeln!(fd5, "{json_str}");
        // Prevent File::drop from closing the fd -- we don't own it
        std::mem::forget(fd5);
    }
}

fn emit_tuple_output(sys: &System, processes: &[ProcessInfo]) {
    // Write tuple text to fd 4 (STDLLM) -- best-effort
    // SAFETY: Same as fd 5 above.
    let mut fd4 = unsafe { std::fs::File::from_raw_fd(STDLLM) };

    for p in processes {
        let port_str = p
            .port
            .map(|port| port.to_string())
            .unwrap_or_else(|| "NONE".to_string());
        let _ = writeln!(
            fd4,
            "(flux:process, {}, {}, {:.1}, {}, {})",
            p.pid, p.name, p.cpu, p.memory_mb, port_str
        );
    }

    let _ = writeln!(
        fd4,
        "(flux:system, {:.1}, {}, {}, {})",
        sys.global_cpu_usage(),
        sys.used_memory() / 1024 / 1024,
        sys.total_memory() / 1024 / 1024,
        processes.len()
    );

    std::mem::forget(fd4);
}

fn emit_tuples_to_space(sys: &System, processes: &[ProcessInfo], tuple_space: &TupleSpace) {
    for p in processes {
        let port_val = match p.port {
            Some(port) => Value::Int(port as i64),
            None => Value::Null,
        };

        tuple_space.out(Tuple::from(vec![
            Value::Str("flux:process".to_string()),
            Value::Int(p.pid as i64),
            Value::Str(p.name.clone()),
            Value::Float(p.cpu as f64),
            Value::Int(p.memory_mb as i64),
            port_val,
        ]));
    }

    tuple_space.out(Tuple::from(vec![
        Value::Str("flux:system".to_string()),
        Value::Float(sys.global_cpu_usage() as f64),
        Value::Int((sys.used_memory() / 1024 / 1024) as i64),
        Value::Int((sys.total_memory() / 1024 / 1024) as i64),
        Value::Int(processes.len() as i64),
    ]));
}

// ---------------------------------------------------------------------------
// Nuke mode
// ---------------------------------------------------------------------------

fn run_nuke_mode(processes: &[ProcessInfo], signal: Signal) -> Result<BuiltinResult> {
    if processes.is_empty() {
        eprintln!("flux: no matching processes found");
        return Ok(BuiltinResult::Failure(1));
    }

    println!("Targeting {} processes:", processes.len());
    for p in processes {
        println!("  - [{}] {}", p.pid, p.name);
    }

    println!("\nAre you sure you want to nuke these? (y/N)");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(ExecutionError::IoError)?;

    if input.trim().to_lowercase() == "y" {
        kill_processes(processes, signal);
    }

    Ok(BuiltinResult::Success(0))
}

// ---------------------------------------------------------------------------
// Entry point: builtin_flux
// ---------------------------------------------------------------------------

/// flux - Process manager builtin
///
/// Usage: flux [flags]
///   --filter/-f NAME   Filter processes by name
///   --signal/-s SIG    Signal to send (default KILL)
///   --sort FIELD        Sort by cpu/mem/pid/name/port
///   --ports            Show only processes with open ports
///   --port PORT        Filter by specific port
///   --nuke             Kill all matching processes (requires --filter or --port)
///   --resources        Show resource sparkline graphs (TUI mode)
///   --tui              Launch interactive TUI mode
pub fn builtin_flux(args: &[String], tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    let fa = match parse_flux_args(args) {
        Ok(fa) => fa,
        Err(e) => {
            eprintln!("flux: {e}");
            return Ok(BuiltinResult::Failure(2));
        }
    };

    let signal = match parse_signal(&fa.signal) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("flux: {e}");
            return Ok(BuiltinResult::Failure(2));
        }
    };

    // Safety: --nuke requires --filter or --port
    if fa.nuke && fa.filter.is_none() && fa.port.is_none() {
        eprintln!("flux: --nuke requires --filter or --port to prevent accidental mass deletion");
        return Ok(BuiltinResult::Failure(2));
    }

    let ports_mode = fa.ports || fa.port.is_some();

    // --- TUI mode ---
    if fa.tui {
        if let Err(e) = tui::run_tui_mode(
            fa.filter.as_deref(),
            fa.sort,
            signal,
            ports_mode,
            fa.port,
            fa.resources,
        ) {
            eprintln!("flux: TUI error: {e}");
            return Ok(BuiltinResult::Failure(1));
        }
        return Ok(BuiltinResult::Success(0));
    }

    // --- Default mode: enumerate, emit, print ---
    let mut sys = System::new_all();

    let processes = if ports_mode {
        refresh_processes_with_ports(&mut sys, fa.filter.as_deref(), fa.port, fa.sort)
    } else {
        refresh_processes(&mut sys, fa.filter.as_deref(), fa.sort)
    };

    // Nuke mode shortcuts to kill flow
    if fa.nuke {
        return run_nuke_mode(&processes, signal);
    }

    // Emit structured JSON on fd 5
    emit_structured_output(&sys, &processes);

    // Emit tuple text on fd 4
    emit_tuple_output(&sys, &processes);

    // Emit tuples into the tuple space for reactive watchers
    emit_tuples_to_space(&sys, &processes, tuple_space);

    // Print formatted text to stdout
    let name_width = calculate_name_width(ports_mode);
    print_header(ports_mode, name_width);
    for p in &processes {
        print_process(p, ports_mode, name_width);
    }

    Ok(BuiltinResult::Success(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flux_args_empty() {
        let args: Vec<String> = vec![];
        let fa = parse_flux_args(&args).unwrap();
        assert!(fa.filter.is_none());
        assert_eq!(fa.sort, SortBy::Cpu);
        assert!(!fa.tui);
        assert!(!fa.nuke);
    }

    #[test]
    fn test_parse_flux_args_filter() {
        let args = vec!["-f".to_string(), "node".to_string()];
        let fa = parse_flux_args(&args).unwrap();
        assert_eq!(fa.filter.as_deref(), Some("node"));
    }

    #[test]
    fn test_parse_flux_args_sort() {
        let args = vec!["--sort".to_string(), "mem".to_string()];
        let fa = parse_flux_args(&args).unwrap();
        assert_eq!(fa.sort, SortBy::Mem);
    }

    #[test]
    fn test_parse_flux_args_port() {
        let args = vec!["--port".to_string(), "3000".to_string()];
        let fa = parse_flux_args(&args).unwrap();
        assert_eq!(fa.port, Some(3000));
    }

    #[test]
    fn test_parse_flux_args_tui() {
        let args = vec!["--tui".to_string()];
        let fa = parse_flux_args(&args).unwrap();
        assert!(fa.tui);
    }

    #[test]
    fn test_parse_flux_args_nuke_requires_filter() {
        let args = vec!["--nuke".to_string()];
        let fa = parse_flux_args(&args).unwrap();
        assert!(fa.nuke);
        // Validation happens in builtin_flux, not in parse
    }

    #[test]
    fn test_parse_flux_args_invalid_flag() {
        let args = vec!["--bogus".to_string()];
        assert!(parse_flux_args(&args).is_err());
    }

    #[test]
    fn test_parse_signal_names() {
        assert_eq!(parse_signal("KILL").unwrap(), Signal::SIGKILL);
        assert_eq!(parse_signal("TERM").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("INT").unwrap(), Signal::SIGINT);
        assert_eq!(parse_signal("SIGKILL").unwrap(), Signal::SIGKILL);
        assert_eq!(parse_signal("9").unwrap(), Signal::SIGKILL);
        assert_eq!(parse_signal("15").unwrap(), Signal::SIGTERM);
    }

    #[test]
    fn test_parse_signal_invalid() {
        assert!(parse_signal("BOGUS").is_err());
    }

    #[test]
    fn test_sort_processes_by_cpu() {
        let mut procs = vec![
            ProcessInfo {
                pid: 1,
                name: "a".into(),
                cpu: 10.0,
                memory_mb: 100,
                port: None,
                protocol: None,
            },
            ProcessInfo {
                pid: 2,
                name: "b".into(),
                cpu: 50.0,
                memory_mb: 200,
                port: None,
                protocol: None,
            },
        ];
        sort_processes(&mut procs, SortBy::Cpu);
        assert_eq!(procs[0].pid, 2); // higher CPU first
    }

    #[test]
    fn test_sort_processes_by_name() {
        let mut procs = vec![
            ProcessInfo {
                pid: 1,
                name: "Zebra".into(),
                cpu: 0.0,
                memory_mb: 0,
                port: None,
                protocol: None,
            },
            ProcessInfo {
                pid: 2,
                name: "alpha".into(),
                cpu: 0.0,
                memory_mb: 0,
                port: None,
                protocol: None,
            },
        ];
        sort_processes(&mut procs, SortBy::Name);
        assert_eq!(procs[0].name, "alpha");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world foo", 8), "hello...");
    }
}
