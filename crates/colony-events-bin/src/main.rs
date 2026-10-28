//! events-monitor: a panel binary that displays live tuple events.
//!
//! Subscribes to ALL tuples (wildcard) via the panel protocol and shows a
//! live event stream with timestamps and values. Tracks events per minute
//! and listener count from the registry.
//!
//! Key bindings: j/k scroll, p pause/resume, f filter by pattern.

use colony_exec::events::EventRegistry;
use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::collections::VecDeque;
use std::io::{self, BufRead, Write};
use std::time::Instant;

/// Maximum events to keep in the display buffer.
const MAX_EVENTS: usize = 200;

fn main() {
    let mut state = EventsState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config: subscribe to ALL tuple keys (wildcard)
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["*".to_string()],
            help: Vec::new(),
        };
        writeln!(
            out,
            "{}",
            serde_json::to_string(&config).expect("config must serialize")
        )
        .ok();
        out.flush().ok();
    }

    // Initial render
    emit_frame(&stdout, &state);

    // Event loop
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else {
            continue;
        };

        match event {
            InputEvent::Key {
                ref key,
                modifiers: _,
            } => {
                let changed = handle_key(&mut state, key);
                if changed {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Tuple { ref data } => {
                if !state.paused {
                    let key = data
                        .get("key")
                        .and_then(|k| k.as_str())
                        .unwrap_or("?")
                        .to_string();

                    let values: Vec<String> = data
                        .get("value")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|v| match v {
                                    serde_json::Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    state.push_event(key, values);
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Resize { width, height } => {
                state.set_size(width, height);
                emit_frame(&stdout, &state);
            }
            InputEvent::Shutdown => break,
            InputEvent::Focus { .. } => {}
        }
    }
}

fn handle_key(state: &mut EventsState, key: &str) -> bool {
    match key {
        "j" | "Down" => state.cursor_down(),
        "k" | "Up" => state.cursor_up(),
        "p" => {
            state.paused = !state.paused;
            true
        }
        "f" => {
            // Toggle filter mode (simple: cycle through known patterns)
            state.cycle_filter();
            true
        }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &EventsState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(
            out,
            "{}",
            serde_json::to_string(&line).expect("frame line must serialize")
        )
        .ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct EventEntry {
    timestamp: Instant,
    key: String,
    values: Vec<String>,
}

struct EventsState {
    events: VecDeque<EventEntry>,
    cursor: usize,
    paused: bool,
    filter: Option<String>,
    known_patterns: Vec<String>,
    filter_index: usize,
    event_timestamps: VecDeque<Instant>,
    _width: u16,
    _height: u16,
}

impl EventsState {
    fn new() -> Self {
        Self {
            events: VecDeque::new(),
            cursor: 0,
            paused: false,
            filter: None,
            known_patterns: Vec::new(),
            filter_index: 0,
            event_timestamps: VecDeque::new(),
            _width: 60,
            _height: 40,
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self._height = height;
    }

    fn push_event(&mut self, key: String, values: Vec<String>) {
        let now = Instant::now();

        // Track known patterns
        if !self.known_patterns.contains(&key) {
            self.known_patterns.push(key.clone());
        }

        // Track event rate
        self.event_timestamps.push_back(now);
        // Remove timestamps older than 60 seconds
        while self
            .event_timestamps
            .front()
            .is_some_and(|t| now.duration_since(*t).as_secs() > 60)
        {
            self.event_timestamps.pop_front();
        }

        self.events.push_front(EventEntry {
            timestamp: now,
            key,
            values,
        });

        while self.events.len() > MAX_EVENTS {
            self.events.pop_back();
        }
    }

    fn events_per_minute(&self) -> usize {
        self.event_timestamps.len()
    }

    fn listener_count(&self) -> usize {
        EventRegistry::open(&EventRegistry::default_path())
            .ok()
            .and_then(|r| r.list_listeners().ok())
            .map(|l| l.len())
            .unwrap_or(0)
    }

    fn cursor_down(&mut self) -> bool {
        let visible = self.visible_events();
        if self.cursor + 1 < visible.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn cursor_up(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn cycle_filter(&mut self) {
        if self.known_patterns.is_empty() {
            self.filter = None;
            return;
        }

        self.filter_index += 1;
        // Cycle: no filter -> each pattern -> no filter
        if self.filter_index > self.known_patterns.len() {
            self.filter_index = 0;
            self.filter = None;
        } else {
            self.filter = Some(self.known_patterns[self.filter_index - 1].clone());
        }
        self.cursor = 0;
    }

    fn visible_events(&self) -> Vec<&EventEntry> {
        self.events
            .iter()
            .filter(|e| self.filter.as_ref().is_none_or(|f| e.key == *f))
            .collect()
    }

    fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();
        let now = Instant::now();

        // Header
        let paused = if self.paused { " [PAUSED]" } else { "" };
        let filter_str = self
            .filter
            .as_ref()
            .map_or(String::new(), |f| format!(" [filter: {f}]"));

        lines.push(FrameLine {
            text: format!(
                "Events  {}/min  {} listeners{paused}{filter_str}",
                self.events_per_minute(),
                self.listener_count(),
            ),
            icon: None,
            style: Some("bold,cyan".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        // Event list
        let visible = self.visible_events();
        for (i, event) in visible.iter().enumerate() {
            let elapsed = now.duration_since(event.timestamp);
            let ago = format_ago(elapsed);
            let is_selected = i == self.cursor;

            let values_str = event.values.join(" ");
            let display = format!(
                "  {:<18} {}    {}",
                event.key,
                truncate_str(&values_str, 40),
                ago
            );

            let style = if is_selected { "bold,white" } else { "white" };

            lines.push(FrameLine {
                text: display,
                icon: if is_selected {
                    Some("\u{f0da}".to_string())
                } else {
                    None
                },
                style: Some(style.to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        if visible.is_empty() {
            lines.push(FrameLine {
                text: "  No events yet".to_string(),
                icon: None,
                style: Some("dim".to_string()),
                indent: None,
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
        }

        lines
    }
}

fn format_ago(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs == 0 {
        "now".to_string()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{kept}...")
    }
}
