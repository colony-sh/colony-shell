# Colony PATH Management & Environment Auto-Reloading

## Overview

Traditional shells treat PATH as an opaque colon-separated string and require manual `source ~/.bashrc` to reload configuration. Colony reimagines environment management as a first-class, reactive system with:

- **Typed, layered PATH** with provenance tracking
- **Built-in directory-scoped environments** (like direnv, but native)
- **Hot-reloading** of configuration files
- **Cross-session synchronization** via tuple spaces
- **Observable state** that other systems can react to

---

## PATH as a First-Class Data Structure

### Core Data Model

Instead of PATH being an opaque string, represent it as a typed collection with metadata:

```rust
use std::path::PathBuf;
use std::time::Instant;
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathLayer {
    System,      // /etc/environment, /etc/paths.d, etc.
    User,        // ~/.colony/config
    Session,     // Added this terminal session only
    Directory,   // Auto-loaded from .colony/env in cwd
}

#[derive(Debug, Clone)]
pub enum PathSource {
    System,                     // OS-level configuration
    UserConfig(PathBuf),        // Which config file added this
    Session,                    // Added interactively this session
    Directory(PathBuf),         // Auto-loaded from .colony/env in this dir
    Manual,                     // User typed `path add`
    Broadcast { session: String }, // Synced from another Colony session
}

#[derive(Debug, Clone)]
pub struct PathEntry {
    /// The directory path
    pub dir: PathBuf,
    
    /// Where this entry came from
    pub source: PathSource,
    
    /// When this entry was added
    pub added_at: Instant,
    
    /// Whether the directory currently exists
    pub valid: bool,
    
    /// Optional human-readable description
    pub description: Option<String>,
}

pub struct PathManager {
    /// Layers ordered by priority (first = highest)
    layers: IndexMap<PathLayer, Vec<PathEntry>>,
    
    /// File watchers for auto-reload
    watchers: Vec<FileWatcher>,
    
    /// Subscribers notified on PATH changes
    subscribers: Vec<Box<dyn Fn(&PathDelta) + Send + Sync>>,
    
    /// Connection to tuple space for cross-session sync
    tuple_space: Option<TupleSpaceClient>,
}
```

### Layer Resolution

PATH entries exist in layers that combine at resolution time:

```
┌─────────────────────────────────┐
│  Directory Layer (.colony/env)  │  ← Highest priority, auto-loaded
├─────────────────────────────────┤
│  Session Layer (path add .)     │  ← This terminal session only
├─────────────────────────────────┤
│  User Layer (~/.colony/config)  │  ← Persistent user config
├─────────────────────────────────┤
│  System Layer (/etc/...)        │  ← Lowest priority
└─────────────────────────────────┘
```

Resolution flattens layers top-to-bottom, with earlier entries taking precedence:

```rust
impl PathManager {
    /// Resolve the effective PATH by flattening all layers
    pub fn resolve(&self) -> Vec<PathBuf> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        
        // Process layers in priority order
        for layer in [PathLayer::Directory, PathLayer::Session, PathLayer::User, PathLayer::System] {
            if let Some(entries) = self.layers.get(&layer) {
                for entry in entries {
                    if entry.valid && seen.insert(entry.dir.clone()) {
                        result.push(entry.dir.clone());
                    }
                }
            }
        }
        
        result
    }
    
    /// Export as traditional colon-separated string for child processes
    pub fn to_env_string(&self) -> String {
        self.resolve()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":")
    }
}
```

---

## Built-in PATH Commands

### Adding to PATH

```bash
# Add current directory to PATH (session-scoped, default)
path add .

# Add with explicit path
path add ./bin
path add ~/.cargo/bin

# Add to front of PATH (higher priority)
path prepend ./bin

# Persist to user config file
path add ~/.local/bin --persist

# Add to specific layer
path add ./vendor/bin --layer=directory

# Broadcast to all Colony sessions via tuple space
path add ~/.local/bin --broadcast

# Add with description (shown in `path list`)
path add ./scripts --desc "Project automation scripts"
```

### Inspecting PATH

```bash
# Table view with source, validity, order
path list

# Example output:
# ┌────┬─────────────────────────┬───────────┬───────┬─────────────────────┐
# │  # │ Directory               │ Layer     │ Valid │ Source              │
# ├────┼─────────────────────────┼───────────┼───────┼─────────────────────┤
# │  1 │ ./bin                   │ Directory │ ✓     │ .colony/env         │
# │  2 │ ./node_modules/.bin     │ Directory │ ✓     │ .colony/env         │
# │  3 │ ~/.cargo/bin            │ Session   │ ✓     │ Manual              │
# │  4 │ ~/.local/bin            │ User      │ ✓     │ ~/.colony/config:12 │
# │  5 │ /usr/local/bin          │ System    │ ✓     │ /etc/paths.d/local  │
# │  6 │ /usr/bin                │ System    │ ✓     │ System default      │
# │  7 │ /old/removed/path       │ User      │ ✗     │ ~/.colony/config:8  │
# └────┴─────────────────────────┴───────────┴───────┴─────────────────────┘

# Filter by layer
path list --layer=directory
path list --layer=session

# Show only invalid entries (directories that don't exist)
path list --invalid

# Show only valid entries
path list --valid

# Search for which PATH entry provides a command
path search rustc
# → ~/.cargo/bin (User layer, from ~/.colony/config:15)

# Enhanced `which` with PATH context
path which rustc
# → ~/.cargo/bin/rustc
#   Provided by: ~/.cargo/bin (User layer)
#   Binary size: 12.4 MB
#   Last modified: 2024-01-15

# Check if a directory is in PATH
path has ./bin
# → Yes (Directory layer, position 1)
```

### Managing PATH

```bash
# Remove an entry by directory
path remove /old/thing

# Remove from specific layer
path remove ./bin --layer=directory

# Remove duplicates (keep first occurrence)
path dedup

# Remove entries pointing to non-existent directories
path clean

# Preview what `path clean` would remove
path clean --dry-run

# Interactive reordering (opens TUI)
path reorder

# Move an entry to a different position
path move ~/.cargo/bin --position=1
path move ~/.cargo/bin --before=/usr/local/bin
path move ~/.cargo/bin --after=./bin
```

### PATH Snapshots

Save and restore PATH configurations for different contexts:

```bash
# Save current PATH as named snapshot
path save rust-dev

# List saved snapshots
path snapshots
# → rust-dev (saved 2024-01-15, 8 entries)
# → python-ml (saved 2024-01-10, 12 entries)
# → minimal (saved 2024-01-05, 4 entries)

# Restore a snapshot
path restore rust-dev

# Show differences from a snapshot
path diff rust-dev
# → + ./node_modules/.bin (not in snapshot)
# → - ~/.pyenv/bin (in snapshot, not current)

# Delete a snapshot
path snapshot delete python-ml

# Export snapshot to file (for sharing)
path snapshot export rust-dev > rust-dev.path.toml

# Import snapshot from file
path snapshot import < rust-dev.path.toml
```

---

## Auto-Reloading Environment

### Watched Files

Colony automatically watches these files for changes:

```
~/.colony/config              # User configuration
~/.colony/env                 # User environment variables
./.colony/env                 # Directory-scoped environment (like direnv)
./colony.toml                 # Project configuration
```

### File Watcher Implementation

```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};
use std::sync::mpsc;
use std::time::Duration;

pub struct EnvironmentWatcher {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<Result<Event, notify::Error>>,
    watched_paths: HashSet<PathBuf>,
    debounce_duration: Duration,
}

impl EnvironmentWatcher {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        
        let watcher = notify::recommended_watcher(move |res| {
            tx.send(res).ok();
        })?;
        
        Ok(Self {
            watcher,
            rx,
            watched_paths: HashSet::new(),
            debounce_duration: Duration::from_millis(100),
        })
    }
    
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        if self.watched_paths.insert(path.to_path_buf()) {
            self.watcher.watch(path, RecursiveMode::NonRecursive)?;
        }
        Ok(())
    }
    
    pub fn poll(&mut self) -> Option<EnvironmentChange> {
        match self.rx.try_recv() {
            Ok(Ok(event)) => {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        self.process_change(&event.paths)
                    }
                    _ => None
                }
            }
            _ => None
        }
    }
    
    fn process_change(&self, paths: &[PathBuf]) -> Option<EnvironmentChange> {
        for path in paths {
            if path.ends_with("env") || path.ends_with("config") {
                return Some(EnvironmentChange {
                    file: path.clone(),
                    kind: ChangeKind::Modified,
                });
            }
        }
        None
    }
}
```

### Hot Reload Behavior

When a watched file changes, Colony:

1. **Detects** the change via inotify (Linux) / FSEvents (macOS) / ReadDirectoryChangesW (Windows)
2. **Parses** the new configuration
3. **Diffs** against current environment
4. **Applies** changes incrementally (no full shell restart)
5. **Notifies** the user with a subtle inline message

```rust
pub struct EnvironmentDelta {
    pub added: Vec<(String, String)>,      // New variables
    pub removed: Vec<String>,               // Removed variables
    pub changed: Vec<(String, String, String)>, // (name, old, new)
    pub path_added: Vec<PathBuf>,
    pub path_removed: Vec<PathBuf>,
}

impl EnvironmentDelta {
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        
        for (name, value) in &self.added {
            parts.push(format!("+{}", name));
        }
        for name in &self.removed {
            parts.push(format!("-{}", name));
        }
        for (name, old, new) in &self.changed {
            parts.push(format!("~{}", name));
        }
        for path in &self.path_added {
            parts.push(format!("PATH +{}", path.display()));
        }
        for path in &self.path_removed {
            parts.push(format!("PATH -{}", path.display()));
        }
        
        parts.join(", ")
    }
}

// User sees:
//  ~/.colony/env changed: +RUST_BACKTRACE, PATH +./target/debug
```

---

## Directory-Scoped Environments

### The `.colony/env` File

Any directory can contain a `.colony/env` file that modifies the environment when you `cd` into it:

```bash
# .colony/env - loaded when entering this directory

# Add to PATH
path add ./bin
path add ./node_modules/.bin
path add ./vendor/bin

# Set environment variables
export DATABASE_URL="postgres://localhost/myapp_dev"
export RUST_BACKTRACE=1
export NODE_ENV=development

# Unset variables (restore parent value on exit)
unset PRODUCTION_SECRET

# Conditionally set based on existing env
if [[ -z "$VIRTUAL_ENV" ]]; then
    source .venv/bin/activate
fi

# Run setup commands (once per session)
once {
    echo "Setting up development environment..."
    npm install --silent
}
```

### Automatic Loading/Unloading on `cd`

```rust
impl Shell {
    pub fn change_directory(&mut self, new_dir: &Path) -> Result<()> {
        let old_dir = self.cwd.clone();
        
        // Find .colony/env files to unload (leaving old_dir ancestors)
        let to_unload = self.find_env_files_between(&new_dir, &old_dir);
        for env_file in to_unload {
            self.unload_directory_env(&env_file)?;
        }
        
        // Find .colony/env files to load (entering new_dir ancestors)
        let to_load = self.find_env_files_between(&old_dir, &new_dir);
        for env_file in to_load {
            if self.is_trusted(&env_file) {
                self.load_directory_env(&env_file)?;
            } else {
                self.prompt_trust(&env_file)?;
            }
        }
        
        self.cwd = new_dir.to_path_buf();
        Ok(())
    }
}
```

### User Experience

```
~/projects $ cd myapp
 Loaded .colony/env: PATH +./bin +./node_modules/.bin, +DATABASE_URL, +NODE_ENV

~/projects/myapp $ echo $DATABASE_URL
postgres://localhost/myapp_dev

~/projects/myapp $ cd ..
 Unloaded myapp/.colony/env: PATH -./bin -./node_modules/.bin, -DATABASE_URL, -NODE_ENV

~/projects $ echo $DATABASE_URL
# (empty - restored to parent state)
```

### Trust Model

Directory environments require explicit trust (security against malicious repos):

```bash
# Trust the current directory's .colony/env
colony trust .
colony trust ./subproject

# Trust with expiration
colony trust . --expires=7d

# Trust all subdirectories of a path
colony trust ~/projects --recursive

# Show trusted directories
colony trust --list
# ┌─────────────────────────┬─────────────┬─────────────────────┐
# │ Directory               │ Trusted     │ Expires             │
# ├─────────────────────────┼─────────────┼─────────────────────┤
# │ ~/projects/myapp        │ 2024-01-15  │ Never               │
# │ ~/projects/client-work  │ 2024-01-10  │ 2024-01-17          │
# │ ~/projects/*            │ 2024-01-01  │ Never (recursive)   │
# └─────────────────────────┴─────────────┴─────────────────────┘

# Revoke trust
colony trust --revoke ~/projects/old-thing

# Show untrusted .colony/env files found
colony trust --pending
# → ~/downloads/sketchy-repo/.colony/env (found 2024-01-15)

# Trust hash - only trust if file hasn't changed
colony trust . --hash
# Trusts current content; prompts again if .colony/env is modified
```

### Untrusted Directory Prompt

When entering a directory with an untrusted `.colony/env`:

```
~/downloads $ cd some-repo
⚠ Untrusted .colony/env found. Contents:

  path add ./bin
  export API_KEY="dev-key-123"

  [t] Trust once   [a] Always trust   [v] View full file
  [i] Ignore once  [n] Never trust    [?] Help

Choice: 
```

---

## Cross-Session Synchronization via Tuple Space

### Broadcasting Changes

Use Colony's tuple space to synchronize environment across terminal sessions:

```bash
# Add to PATH in all Colony sessions
path add ~/.local/bin --broadcast

# Other sessions immediately see:
#  Synced from session[abc123]: PATH +~/.local/bin

# Set env var across all sessions
env set API_ENDPOINT="https://api.prod.example.com" --broadcast

# Sync specific variables automatically
env sync AWS_PROFILE              # This var always syncs
env sync --pattern 'KUBE_*'       # All KUBE_ vars sync
env sync --pattern 'TF_VAR_*'     # Terraform vars sync

# Stop syncing a variable
env unsync AWS_PROFILE

# List synced variables
env sync --list
# → AWS_PROFILE (synced)
# → KUBE_* (pattern, synced)
```

### Tuple Space Protocol

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvironmentTuple {
    PathAdd {
        session_id: String,
        entry: PathEntry,
        timestamp: u64,
    },
    PathRemove {
        session_id: String,
        dir: PathBuf,
        timestamp: u64,
    },
    EnvSet {
        session_id: String,
        name: String,
        value: String,
        timestamp: u64,
    },
    EnvUnset {
        session_id: String,
        name: String,
        timestamp: u64,
    },
}

impl PathManager {
    pub fn add_with_broadcast(&mut self, dir: PathBuf, opts: AddOpts) -> Result<()> {
        // Add locally first
        self.add(dir.clone(), opts.clone())?;
        
        // Broadcast via tuple space
        if opts.broadcast {
            if let Some(ts) = &self.tuple_space {
                ts.put(EnvironmentTuple::PathAdd {
                    session_id: self.session_id.clone(),
                    entry: PathEntry {
                        dir,
                        source: PathSource::Broadcast { 
                            session: self.session_id.clone() 
                        },
                        added_at: Instant::now(),
                        valid: true,
                        description: opts.description,
                    },
                    timestamp: current_timestamp(),
                })?;
            }
        }
        
        Ok(())
    }
    
    /// Called by tuple space listener when receiving broadcasts
    pub fn handle_broadcast(&mut self, tuple: EnvironmentTuple) -> Result<()> {
        match tuple {
            EnvironmentTuple::PathAdd { session_id, entry, .. } => {
                if session_id != self.session_id {
                    self.layers
                        .entry(PathLayer::Session)
                        .or_default()
                        .push(entry.clone());
                    
                    self.notify_user(&format!(
                        " Synced from session[{}]: PATH +{}",
                        &session_id[..8],
                        entry.dir.display()
                    ));
                }
            }
            // ... handle other tuple types
        }
        Ok(())
    }
}
```

### Sync Patterns for Variables

```rust
pub struct EnvSyncManager {
    /// Variables that always sync
    synced_vars: HashSet<String>,
    
    /// Patterns (glob-style) that sync
    synced_patterns: Vec<glob::Pattern>,
    
    /// Local overrides (never sync these even if matched)
    never_sync: HashSet<String>,
}

impl EnvSyncManager {
    pub fn should_sync(&self, var_name: &str) -> bool {
        if self.never_sync.contains(var_name) {
            return false;
        }
        
        if self.synced_vars.contains(var_name) {
            return true;
        }
        
        self.synced_patterns.iter().any(|p| p.matches(var_name))
    }
    
    pub fn on_env_change(&self, name: &str, value: &str, tuple_space: &TupleSpaceClient) {
        if self.should_sync(name) {
            tuple_space.put(EnvironmentTuple::EnvSet {
                session_id: self.session_id.clone(),
                name: name.to_string(),
                value: value.to_string(),
                timestamp: current_timestamp(),
            }).ok();
        }
    }
}
```

---

## Reactive Environment System

### Observable State

Environment changes are observable events that other Colony subsystems can react to:

```rust
pub trait EnvironmentObserver: Send + Sync {
    fn on_path_change(&self, delta: &PathDelta);
    fn on_env_change(&self, name: &str, old: Option<&str>, new: Option<&str>);
}

impl PathManager {
    pub fn subscribe(&mut self, observer: Arc<dyn EnvironmentObserver>) {
        self.observers.push(observer);
    }
    
    fn notify_change(&self, delta: &PathDelta) {
        for observer in &self.observers {
            observer.on_path_change(delta);
        }
    }
}

// Example: Completion system invalidates cache on PATH changes
pub struct CompletionSystem {
    command_cache: HashMap<String, Vec<Completion>>,
}

impl EnvironmentObserver for CompletionSystem {
    fn on_path_change(&self, delta: &PathDelta) {
        // Invalidate command completions since available commands changed
        self.command_cache.clear();
        self.rebuild_command_index();
    }
    
    fn on_env_change(&self, name: &str, old: Option<&str>, new: Option<&str>) {
        // Some completions depend on env vars (e.g., GOPATH, CARGO_HOME)
        if name == "GOPATH" || name == "CARGO_HOME" {
            self.command_cache.clear();
        }
    }
}
```

### User-Defined Hooks

Users can register hooks that run on environment changes:

```bash
# In ~/.colony/config

# Hook: Run on any PATH change
on path_change {
    echo "PATH updated, rehashing..."
    # Custom logic here
}

# Hook: Run when specific variable changes
on env_change DATABASE_URL {
    echo "Database changed to: $DATABASE_URL"
    # Maybe restart development server
    if [[ -n "$DEV_SERVER_PID" ]]; then
        kill -HUP $DEV_SERVER_PID
    fi
}

# Hook: Run when entering a directory with .colony/env
on directory_env_load {
    echo "Loaded environment for: $COLONY_ENV_DIR"
}

# Hook: Run when leaving a directory with .colony/env  
on directory_env_unload {
    echo "Unloaded environment for: $COLONY_ENV_DIR"
}

# Hook: Run on any environment sync from another session
on env_sync {
    echo "Synced $COLONY_SYNC_COUNT changes from session $COLONY_SYNC_SOURCE"
}
```

### Hook Implementation

```rust
#[derive(Debug, Clone)]
pub enum HookTrigger {
    PathChange,
    EnvChange { variable: Option<String> },  // None = any variable
    DirectoryEnvLoad,
    DirectoryEnvUnload,
    EnvSync,
}

pub struct Hook {
    trigger: HookTrigger,
    script: String,
    source_file: PathBuf,
    source_line: usize,
}

pub struct HookManager {
    hooks: Vec<Hook>,
}

impl HookManager {
    pub fn fire(&self, trigger: HookTrigger, env: &mut Environment) -> Result<()> {
        for hook in &self.hooks {
            if self.matches_trigger(&hook.trigger, &trigger) {
                // Set up hook-specific variables
                self.setup_hook_env(&trigger, env);
                
                // Execute hook script
                self.execute_hook(&hook.script, env)?;
            }
        }
        Ok(())
    }
    
    fn matches_trigger(&self, hook_trigger: &HookTrigger, event: &HookTrigger) -> bool {
        match (hook_trigger, event) {
            (HookTrigger::PathChange, HookTrigger::PathChange) => true,
            (HookTrigger::EnvChange { variable: None }, HookTrigger::EnvChange { .. }) => true,
            (HookTrigger::EnvChange { variable: Some(v1) }, 
             HookTrigger::EnvChange { variable: Some(v2) }) => v1 == v2,
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }
}
```

---

## TUI Components

### `path list` Table View

```rust
use ratatui::{
    widgets::{Table, Row, Cell, Block, Borders},
    style::{Style, Color, Modifier},
};

pub fn render_path_table(path_manager: &PathManager) -> Table<'_> {
    let header = Row::new(vec![
        Cell::from("#"),
        Cell::from("Directory"),
        Cell::from("Layer"),
        Cell::from("Valid"),
        Cell::from("Source"),
    ]).style(Style::default().add_modifier(Modifier::BOLD));
    
    let rows: Vec<Row> = path_manager
        .all_entries()
        .enumerate()
        .map(|(i, entry)| {
            let valid_cell = if entry.valid {
                Cell::from("✓").style(Style::default().fg(Color::Green))
            } else {
                Cell::from("✗").style(Style::default().fg(Color::Red))
            };
            
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(entry.dir.display().to_string()),
                Cell::from(format!("{:?}", entry.layer)),
                valid_cell,
                Cell::from(entry.source_description()),
            ])
        })
        .collect();
    
    Table::new(rows)
        .header(header)
        .block(Block::default().title("PATH").borders(Borders::ALL))
        .widths(&[
            Constraint::Length(4),
            Constraint::Percentage(40),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Percentage(30),
        ])
}
```

### `path reorder` Interactive TUI

```rust
pub struct PathReorderApp {
    entries: Vec<PathEntry>,
    selected: usize,
    dragging: Option<usize>,
    modified: bool,
}

impl PathReorderApp {
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    if self.dragging.is_some() {
                        self.entries.swap(self.selected, self.selected - 1);
                        self.modified = true;
                    }
                    self.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < self.entries.len() - 1 {
                    if self.dragging.is_some() {
                        self.entries.swap(self.selected, self.selected + 1);
                        self.modified = true;
                    }
                    self.selected += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Toggle drag mode
                self.dragging = if self.dragging.is_some() {
                    None
                } else {
                    Some(self.selected)
                };
            }
            KeyCode::Char('d') => {
                // Delete selected entry
                self.entries.remove(self.selected);
                self.modified = true;
                if self.selected >= self.entries.len() {
                    self.selected = self.entries.len().saturating_sub(1);
                }
            }
            KeyCode::Char('s') => {
                // Save and exit
                self.save();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                // Exit without saving (prompt if modified)
            }
            _ => {}
        }
    }
}
```

---

## Configuration File Format

### `~/.colony/config`

```toml
[path]
# Persistent PATH entries
entries = [
    { dir = "~/.cargo/bin", description = "Rust toolchain" },
    { dir = "~/.local/bin", description = "User binaries" },
    { dir = "~/go/bin", description = "Go binaries" },
]

# Auto-clean invalid entries on shell start
auto_clean = true

# Warn about duplicate entries
warn_duplicates = true

[environment]
# Variables to sync across sessions
sync = ["AWS_PROFILE", "KUBECONFIG"]
sync_patterns = ["TF_VAR_*", "KUBE_*"]

# Never sync these (security)
never_sync = ["AWS_SECRET_ACCESS_KEY", "API_KEY", "*_PASSWORD"]

[directory_env]
# Global trust settings
trust_home_subdirs = true          # Auto-trust ~/projects/*
prompt_on_untrusted = true         # Show prompt for untrusted dirs
hash_based_trust = false           # Re-prompt if .colony/env changes

# Trusted directories (managed by `colony trust`)
trusted = [
    { path = "~/projects/myapp", trusted_at = "2024-01-15" },
    { path = "~/work/*", trusted_at = "2024-01-01", recursive = true },
]

[hooks]
# Define hooks inline or reference external scripts
path_change = "~/.colony/hooks/on-path-change.sh"

[[hooks.env_change]]
variable = "DATABASE_URL"
script = '''
echo "Database changed to: $DATABASE_URL"
'''

[reload]
# Files to watch for auto-reload
watch = [
    "~/.colony/config",
    "~/.colony/env",
]

# Debounce duration for file changes
debounce_ms = 100

# Show notification on reload
notify = true
```

---

## Command Reference

### PATH Commands

| Command | Description |
|---------|-------------|
| `path add <dir>` | Add directory to PATH (session layer) |
| `path add <dir> --persist` | Add and save to user config |
| `path add <dir> --broadcast` | Add and sync to all sessions |
| `path prepend <dir>` | Add to front of PATH |
| `path remove <dir>` | Remove directory from PATH |
| `path list` | Show all PATH entries with metadata |
| `path list --layer=<layer>` | Filter by layer |
| `path list --invalid` | Show only invalid entries |
| `path search <cmd>` | Find which PATH entry provides command |
| `path which <cmd>` | Enhanced which with context |
| `path has <dir>` | Check if directory is in PATH |
| `path dedup` | Remove duplicate entries |
| `path clean` | Remove invalid entries |
| `path reorder` | Interactive reordering TUI |
| `path move <dir> --position=<n>` | Move entry to position |
| `path save <name>` | Save PATH snapshot |
| `path restore <name>` | Restore PATH snapshot |
| `path diff <name>` | Compare current to snapshot |
| `path snapshots` | List saved snapshots |

### Environment Commands

| Command | Description |
|---------|-------------|
| `env set <name>=<value>` | Set environment variable |
| `env set <name>=<value> --broadcast` | Set and sync to all sessions |
| `env unset <name>` | Unset environment variable |
| `env list` | List all environment variables |
| `env sync <name>` | Enable sync for variable |
| `env sync --pattern '<glob>'` | Enable sync for pattern |
| `env unsync <name>` | Disable sync for variable |
| `env sync --list` | List synced variables |

### Trust Commands

| Command | Description |
|---------|-------------|
| `colony trust .` | Trust current directory |
| `colony trust <dir>` | Trust specific directory |
| `colony trust <dir> --recursive` | Trust directory and children |
| `colony trust <dir> --expires=<duration>` | Trust with expiration |
| `colony trust <dir> --hash` | Trust only current content |
| `colony trust --list` | List trusted directories |
| `colony trust --revoke <dir>` | Revoke trust |
| `colony trust --pending` | Show untrusted .colony/env files |

---

## Future Considerations

### Project-Type Detection

Automatically suggest PATH additions based on project type:

```bash
# Entering a Node.js project
 Detected Node.js project. Suggested: path add ./node_modules/.bin

# Entering a Rust project
 Detected Cargo project. Suggested: path add ./target/debug

# Entering a Python project with venv
 Detected Python venv. Suggested: source .venv/bin/activate
```

### PATH Inheritance for Child Processes

Control how PATH is inherited by child processes:

```bash
# Run command with modified PATH
path with ./special/bin -- ./run-script.sh

# Run command with minimal PATH
path minimal -- compile.sh

# Run command with snapshot PATH
path using rust-dev -- cargo build
```

### Integration with Package Managers

```bash
# Automatically add paths when installing tools
cargo install ripgrep
#  Added ~/.cargo/bin to PATH (already present)

npm install -g typescript
#  Added ~/.npm-global/bin to PATH
```