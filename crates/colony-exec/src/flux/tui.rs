//! flux TUI mode - Interactive ratatui-based process viewer/killer
//!
//! Ported from the flux standalone binary's `run_live_mode`.

use super::{
    kill_processes, refresh_processes, refresh_processes_with_ports, truncate, ProcessInfo, SortBy,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use nix::sys::signal::Signal;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, TableState},
};
use std::collections::HashSet;
use std::io::stdout;
use std::time::{Duration, Instant};
use sysinfo::System;

// ---------------------------------------------------------------------------
// Resource history for sparkline graphs
// ---------------------------------------------------------------------------

struct ResourceHistory {
    cpu_data: Vec<u64>,
    mem_data: Vec<u64>,
    max_samples: usize,
}

impl ResourceHistory {
    fn new(max_samples: usize) -> Self {
        Self {
            cpu_data: Vec::new(),
            mem_data: Vec::new(),
            max_samples,
        }
    }

    fn push(&mut self, cpu: u64, mem: u64) {
        if self.cpu_data.len() >= self.max_samples {
            self.cpu_data.remove(0);
            self.mem_data.remove(0);
        }
        self.cpu_data.push(cpu);
        self.mem_data.push(mem);
    }
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

const ORANGE: Color = Color::Rgb(255, 165, 0);

fn draw_resource_graphs(frame: &mut Frame, history: &ResourceHistory) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    let cpu_title = format!(" CPU USAGE ({}%) ", history.cpu_data.last().unwrap_or(&0));
    let cpu_line = Sparkline::default()
        .block(
            Block::default()
                .title(Line::from(cpu_title).style(Style::default().fg(ORANGE)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ORANGE)),
        )
        .data(&history.cpu_data)
        .style(Style::default().fg(Color::Cyan));

    let mem_title = format!(
        " MEMORY USAGE ({} MB) ",
        history.mem_data.last().unwrap_or(&0)
    );
    let mem_line = Sparkline::default()
        .block(
            Block::default()
                .title(Line::from(mem_title).style(Style::default().fg(ORANGE)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ORANGE))
                .title_bottom("q - quit"),
        )
        .data(&history.mem_data)
        .style(Style::default().fg(Color::Magenta));

    frame.render_widget(cpu_line, chunks[0]);
    frame.render_widget(mem_line, chunks[1]);
}

// ---------------------------------------------------------------------------
// Process table rendering
// ---------------------------------------------------------------------------

fn draw_process_table(
    frame: &mut Frame,
    processes: &[ProcessInfo],
    selected_pids: &HashSet<u32>,
    table_state: &mut TableState,
    ports_mode: bool,
    show_confirm: bool,
) {
    let area = frame.area();

    let rows: Vec<Row> = processes
        .iter()
        .map(|p| {
            let is_selected = selected_pids.contains(&p.pid);
            let marker = if is_selected { "●" } else { " " };

            let cpu_style = if p.cpu > 50.0 {
                Style::default().fg(Color::Red).bold()
            } else if p.cpu > 10.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let memory_style = if p.memory_mb > 500 {
                Style::default().fg(Color::Red).bold()
            } else if p.memory_mb > 100 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let mut cells = vec![Cell::from(marker).style(if is_selected {
                Style::default().fg(Color::Green).bold()
            } else {
                Style::default()
            })];

            if ports_mode {
                let port_str = p
                    .port
                    .map(|port| format!("{:<5}", port))
                    .unwrap_or_default();
                let proto_str = p.protocol.as_deref().unwrap_or("");
                cells.push(
                    Cell::from(format!("{} {:>3}", port_str, proto_str))
                        .style(Style::default().fg(Color::Green)),
                );
            }

            cells.extend([
                Cell::from(format!("{:<7}", p.pid)).style(Style::default().fg(Color::DarkGray)),
                Cell::from(truncate(&p.name, 40)).style(Style::default().fg(Color::White)),
                Cell::from(format!("{:>6.1}%", p.cpu)).style(cpu_style),
                Cell::from(format!("{:>6} MB", p.memory_mb)).style(memory_style),
            ]);

            Row::new(cells)
        })
        .collect();

    let (header, widths): (Row, Vec<Constraint>) = if ports_mode {
        (
            Row::new(vec![
                Cell::from(" "),
                Cell::from(format!("{:<9}", "PORT")).style(Style::default().fg(ORANGE)),
                Cell::from(format!("{:<7}", "PID")).style(Style::default().fg(ORANGE)),
                Cell::from("NAME").style(Style::default().fg(ORANGE)),
                Cell::from(format!("{:>7}", "CPU %")).style(Style::default().fg(ORANGE)),
                Cell::from(format!("{:>9}", "MEMORY")).style(Style::default().fg(ORANGE)),
            ])
            .style(Style::default().bold()),
            vec![
                Constraint::Length(2),
                Constraint::Length(9),
                Constraint::Length(7),
                Constraint::Min(20),
                Constraint::Length(7),
                Constraint::Length(9),
            ],
        )
    } else {
        (
            Row::new(vec![
                Cell::from(" "),
                Cell::from(format!("{:<7}", "PID")).style(Style::default().fg(ORANGE)),
                Cell::from("NAME").style(Style::default().fg(ORANGE)),
                Cell::from(format!("{:>7}", "CPU %")).style(Style::default().fg(ORANGE)),
                Cell::from(format!("{:>9}", "MEMORY")).style(Style::default().fg(ORANGE)),
            ])
            .style(Style::default().bold()),
            vec![
                Constraint::Length(2),
                Constraint::Length(7),
                Constraint::Min(20),
                Constraint::Length(7),
                Constraint::Length(9),
            ],
        )
    };

    let selected_count = selected_pids.len();
    let title = if selected_count > 0 {
        format!(" flux - {} selected ", selected_count)
    } else {
        " flux ".to_string()
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(Line::from(title).style(Style::default().fg(ORANGE)))
                .border_style(Style::default().fg(ORANGE))
                .title_bottom(Line::from(
                    " ↑↓ - navigate • Space - select • Enter - kill • q - quit • a - select all ",
                ).style(Style::default().fg(ORANGE))),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(128, 85, 0))
                .fg(Color::White),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, area, table_state);

    if show_confirm {
        let popup_area = centered_rect(50, 20, area);
        frame.render_widget(Clear, popup_area);

        let count = selected_pids.len();
        let pids: Vec<u32> = selected_pids.iter().copied().collect();
        let text = format!(
            "Kill {} process{}?\nPIDs: {:?}\n\n[Enter] Confirm  [Esc] Cancel",
            count,
            if count == 1 { "" } else { "es" },
            pids
        );
        let popup = Paragraph::new(text).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Confirm ")
                .border_style(Style::default().fg(ORANGE)),
        );
        frame.render_widget(popup, popup_area);
    }
}

// ---------------------------------------------------------------------------
// Main TUI event loop
// ---------------------------------------------------------------------------

pub fn run_tui_mode(
    filter: Option<&str>,
    sort_by: SortBy,
    signal: Signal,
    ports_mode: bool,
    port_filter: Option<u16>,
    show_resources: bool,
) -> std::io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut table_state = TableState::default();
    table_state.select(Some(0));
    let mut selected_pids: HashSet<u32> = HashSet::new();
    let mut last_refresh = Instant::now();
    let refresh_interval = Duration::from_secs(2);
    let mut sys = System::new_all();

    let mut history = ResourceHistory::new(100);

    let mut processes = if ports_mode {
        refresh_processes_with_ports(&mut sys, filter, port_filter, sort_by)
    } else {
        refresh_processes(&mut sys, filter, sort_by)
    };
    let mut show_confirm = false;

    loop {
        // Refresh data periodically
        if last_refresh.elapsed() >= refresh_interval && !show_confirm {
            sys.refresh_cpu_all();
            sys.refresh_memory();

            processes = if ports_mode {
                refresh_processes_with_ports(&mut sys, filter, port_filter, sort_by)
            } else {
                refresh_processes(&mut sys, filter, sort_by)
            };

            let global_cpu = sys.global_cpu_usage() as u64;
            let total_mem = sys.used_memory() / 1024 / 1024;
            history.push(global_cpu, total_mem);

            last_refresh = Instant::now();

            if let Some(selected) = table_state.selected() {
                if selected >= processes.len() && !processes.is_empty() {
                    table_state.select(Some(processes.len() - 1));
                }
            }
        }

        // Draw
        terminal.draw(|frame| {
            if show_resources {
                draw_resource_graphs(frame, &history);
            } else {
                draw_process_table(
                    frame,
                    &processes,
                    &selected_pids,
                    &mut table_state,
                    ports_mode,
                    show_confirm,
                );
            }
        })?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if show_confirm {
                        match key.code {
                            KeyCode::Enter => break,
                            KeyCode::Esc => show_confirm = false,
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                selected_pids.clear();
                                break;
                            }
                            KeyCode::Char('a') => {
                                let all_selected =
                                    processes.iter().all(|p| selected_pids.contains(&p.pid));
                                if all_selected {
                                    selected_pids.clear();
                                } else {
                                    for proc_info in &processes {
                                        selected_pids.insert(proc_info.pid);
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if let Some(selected) = table_state.selected() {
                                    if selected > 0 {
                                        table_state.select(Some(selected - 1));
                                    }
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if let Some(selected) = table_state.selected() {
                                    if selected < processes.len().saturating_sub(1) {
                                        table_state.select(Some(selected + 1));
                                    }
                                }
                            }
                            KeyCode::Char(' ') => {
                                if let Some(selected) = table_state.selected() {
                                    if let Some(proc_info) = processes.get(selected) {
                                        if selected_pids.contains(&proc_info.pid) {
                                            selected_pids.remove(&proc_info.pid);
                                        } else {
                                            selected_pids.insert(proc_info.pid);
                                        }
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                if !selected_pids.is_empty() {
                                    show_confirm = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    if !selected_pids.is_empty() {
        let to_kill: Vec<ProcessInfo> = processes
            .into_iter()
            .filter(|p| selected_pids.contains(&p.pid))
            .collect();
        kill_processes(&to_kill, signal);
    }

    Ok(())
}
