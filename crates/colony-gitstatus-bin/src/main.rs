//! gitstatus: a panel binary that displays git repository status.
//!
//! Shows modified, staged, untracked, and deleted files grouped by status.
//! Subscribes to `cmd_complete` and `git_change` tuples to refresh after
//! commands complete or git state changes.

use colony_panel::protocol::{FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME};
use std::io::{self, BufRead, Write};
use std::process::Command;

fn main() {
    let mut state = GitStatusState::new();
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config with subscriptions
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["cmd_complete".to_string(), "git_change".to_string()],
            help: Vec::new(),
        };
        writeln!(out, "{}", serde_json::to_string(&config).expect("config must serialize")).ok();
        out.flush().ok();
    }

    // Initial render
    state.refresh();
    emit_frame(&stdout, &state);

    // Event loop: read stdin for events
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else {
            continue;
        };

        match event {
            InputEvent::Key {
                ref key,
                ref modifiers,
            } => {
                let changed = handle_key(&mut state, key, modifiers);
                if changed {
                    emit_frame(&stdout, &state);
                }
            }
            InputEvent::Tuple { .. } => {
                // Any subscribed tuple means we should refresh git status
                state.refresh();
                emit_frame(&stdout, &state);
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

fn handle_key(state: &mut GitStatusState, key: &str, _modifiers: &[String]) -> bool {
    match key {
        "j" | "Down" => state.cursor_down(),
        "k" | "Up" => state.cursor_up(),
        "r" => {
            state.refresh();
            true
        }
        _ => false,
    }
}

fn emit_frame(stdout: &io::Stdout, state: &GitStatusState) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in state.render_lines() {
        writeln!(out, "{}", serde_json::to_string(&line).expect("frame line must serialize")).ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

// ---------------------------------------------------------------------------
// Git file entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct GitEntry {
    path: String,
    status: FileStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FileStatus {
    Staged,
    Modified,
    Deleted,
    Untracked,
}

impl FileStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Staged => "Staged",
            Self::Modified => "Modified",
            Self::Deleted => "Deleted",
            Self::Untracked => "Untracked",
        }
    }

    fn style(self) -> &'static str {
        match self {
            Self::Staged => "green",
            Self::Modified => "yellow",
            Self::Deleted => "red",
            Self::Untracked => "dim",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Staged => "\u{f00c}",    // check mark
            Self::Modified => "\u{f040}",   // pencil
            Self::Deleted => "\u{f00d}",    // x mark
            Self::Untracked => "\u{f128}",  // question mark
        }
    }
}

// ---------------------------------------------------------------------------
// Branch info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct BranchInfo {
    name: String,
    ahead: u32,
    behind: u32,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct GitStatusState {
    branch: BranchInfo,
    entries: Vec<GitEntry>,
    cursor: usize,
    _width: u16,
    _height: u16,
}

impl GitStatusState {
    fn new() -> Self {
        Self {
            branch: BranchInfo::default(),
            entries: Vec::new(),
            cursor: 0,
            _width: 30,
            _height: 40,
        }
    }

    fn set_size(&mut self, width: u16, height: u16) {
        self._width = width;
        self._height = height;
    }

    fn refresh(&mut self) {
        self.branch = parse_branch_info();
        self.entries = parse_git_status();
        // Clamp cursor to valid range
        let max = self.entry_count().saturating_sub(1);
        if self.cursor > max {
            self.cursor = max;
        }
    }

    /// Total number of selectable lines (section headers + entries).
    fn entry_count(&self) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        let section_count = self.section_count();
        section_count + self.entries.len()
    }

    fn section_count(&self) -> usize {
        let mut sections = 0usize;
        let mut last_status: Option<FileStatus> = None;
        for entry in &self.entries {
            if last_status != Some(entry.status) {
                sections += 1;
                last_status = Some(entry.status);
            }
        }
        sections
    }

    fn cursor_down(&mut self) -> bool {
        let max = self.entry_count().saturating_sub(1);
        if self.cursor < max {
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

    fn render_lines(&self) -> Vec<FrameLine> {
        let mut lines = Vec::new();

        // Branch header
        let mut branch_text = format!("\u{e0a0} {}", self.branch.name);
        if self.branch.ahead > 0 || self.branch.behind > 0 {
            branch_text.push_str(&format!(
                " [+{}/-{}]",
                self.branch.ahead, self.branch.behind
            ));
        }
        lines.push(FrameLine {
            text: branch_text,
            icon: None,
            style: Some("bold,cyan".to_string()),
            indent: None,
            action: None,
            expandable: None,
            expanded: None,
            selected: None,
        });

        if self.entries.is_empty() {
            lines.push(FrameLine {
                text: "Clean working tree".to_string(),
                icon: Some("\u{f00c}".to_string()),
                style: Some("dim".to_string()),
                indent: Some(1),
                action: None,
                expandable: None,
                expanded: None,
                selected: None,
            });
            return lines;
        }

        // Group entries by status, emitting section headers
        let mut last_status: Option<FileStatus> = None;
        let mut line_idx = 0usize;

        for entry in &self.entries {
            if last_status != Some(entry.status) {
                // Section header
                let header_selected = line_idx == self.cursor;
                let header_style = if header_selected {
                    "bold,underline"
                } else {
                    "bold"
                };
                lines.push(FrameLine {
                    text: format!("{} ({})", entry.status.label(), self.count_status(entry.status)),
                    icon: None,
                    style: Some(header_style.to_string()),
                    indent: None,
                    action: None,
                    expandable: None,
                    expanded: None,
                    selected: None,
                });
                last_status = Some(entry.status);
                line_idx += 1;
            }

            let selected = line_idx == self.cursor;
            let style = if selected {
                format!("bold,{}", entry.status.style())
            } else {
                entry.status.style().to_string()
            };

            lines.push(FrameLine {
                text: entry.path.clone(),
                icon: Some(entry.status.icon().to_string()),
                style: Some(style),
                indent: Some(1),
                action: Some(format!("open:{}", entry.path)),
                expandable: None,
                expanded: None,
                selected: None,
            });
            line_idx += 1;
        }

        lines
    }

    fn count_status(&self, status: FileStatus) -> usize {
        self.entries.iter().filter(|e| e.status == status).count()
    }
}

// ---------------------------------------------------------------------------
// Git command helpers
// ---------------------------------------------------------------------------

fn parse_branch_info() -> BranchInfo {
    let output = Command::new("git")
        .args(["status", "-b", "--porcelain"])
        .output();

    let Ok(output) = output else {
        return BranchInfo {
            name: "unknown".to_string(),
            ..BranchInfo::default()
        };
    };

    if !output.status.success() {
        return BranchInfo {
            name: "not a repo".to_string(),
            ..BranchInfo::default()
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(header) = stdout.lines().next() else {
        return BranchInfo::default();
    };

    // Header format: "## branch...tracking [ahead N, behind M]"
    let header = header.strip_prefix("## ").unwrap_or(header);

    let mut info = BranchInfo::default();

    // Extract branch name (everything before "..." or end of string)
    let branch_part = header.split("...").next().unwrap_or(header);
    // Also strip any bracket info from branch name
    info.name = branch_part.split(' ').next().unwrap_or(branch_part).to_string();

    // Parse ahead/behind from bracket portion
    if let Some(bracket_start) = header.find('[') {
        let Some(bracket_end) = header.find(']') else {
            return info;
        };
        let bracket = &header[bracket_start + 1..bracket_end];
        for part in bracket.split(", ") {
            let part = part.trim();
            if let Some(n) = part.strip_prefix("ahead ") {
                info.ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                info.behind = n.parse().unwrap_or(0);
            }
        }
    }

    info
}

fn parse_git_status() -> Vec<GitEntry> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries: Vec<GitEntry> = stdout
        .lines()
        .filter(|line| line.len() >= 3)
        .filter_map(|line| {
            let index = line.as_bytes()[0];
            let worktree = line.as_bytes()[1];
            let path = line[3..].to_string();

            let status = classify_status(index, worktree)?;
            Some(GitEntry { path, status })
        })
        .collect();

    // Sort by status group, then by path within each group
    entries.sort_by(|a, b| a.status.cmp(&b.status).then(a.path.cmp(&b.path)));
    entries
}

/// Classify a porcelain status pair into a single `FileStatus`.
///
/// The index column indicates staged changes, the worktree column indicates
/// unstaged changes. We prioritise staged status when both columns are set.
fn classify_status(index: u8, worktree: u8) -> Option<FileStatus> {
    // Staged changes (index column has a meaningful letter)
    match index {
        b'A' | b'M' | b'R' | b'C' => return Some(FileStatus::Staged),
        b'D' => return Some(FileStatus::Deleted),
        _ => {}
    }

    // Worktree changes
    match worktree {
        b'M' => Some(FileStatus::Modified),
        b'D' => Some(FileStatus::Deleted),
        b'?' => Some(FileStatus::Untracked),
        _ => None,
    }
}
