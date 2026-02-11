# Part 13: Interactive Features

## 13.1 Overview

Colony's interactive features enhance the command-line experience with real-time feedback, intelligent assistance, and safety checks.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Interactive Features                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  $ rm -rf ./build                                               │   │
│  │  ┌─────────────────────────────────────────────────────────┐   │   │
│  │  │ ⚠ This will delete 342 files (15.2 MB)                  │   │   │
│  │  │ Preview: build/{src,test,dist}/**/*                     │   │   │
│  │  │ [Enter] Execute  [Tab] Show files  [Esc] Cancel         │   │   │
│  │  └─────────────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  $ git rebase -i HEAD~5                                         │   │
│  │  ┌─────────────────────────────────────────────────────────┐   │   │
│  │  │ 📖 Interactive rebase: Edit last 5 commits               │   │   │
│  │  │ Commands: pick, reword, edit, squash, fixup, drop       │   │   │
│  │  │ [F1] Full help  [Tab] Complete                          │   │   │
│  │  └─────────────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## 13.2 Live Command Preview

```rust
// crates/colony-ui/src/preview.rs

use std::time::Duration;

/// Live preview system for commands
pub struct CommandPreview {
    /// Current input line
    input: String,
    /// Parsed command (if valid)
    parsed: Option<ParsedCommand>,
    /// Preview result
    preview: Option<PreviewResult>,
    /// Preview generation state
    state: PreviewState,
    /// Debounce timer
    debounce: Option<tokio::time::Instant>,
    /// Configuration
    config: PreviewConfig,
}

#[derive(Clone, Debug)]
pub struct PreviewConfig {
    /// Delay before generating preview (ms)
    pub debounce_ms: u64,
    /// Maximum items to show in preview
    pub max_items: usize,
    /// Enable for destructive commands
    pub warn_destructive: bool,
    /// Commands that trigger preview
    pub preview_commands: Vec<String>,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 150,
            max_items: 10,
            warn_destructive: true,
            preview_commands: vec![
                "rm".to_string(),
                "mv".to_string(),
                "cp".to_string(),
                "git".to_string(),
                "find".to_string(),
                "grep".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub enum PreviewState {
    Idle,
    Pending,
    Loading,
    Ready,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct PreviewResult {
    /// Type of preview
    pub kind: PreviewKind,
    /// Preview content
    pub content: PreviewContent,
    /// Warnings (if any)
    pub warnings: Vec<PreviewWarning>,
    /// Suggested actions
    pub actions: Vec<PreviewAction>,
}

#[derive(Clone, Debug)]
pub enum PreviewKind {
    /// File listing (ls, find, etc.)
    FileList,
    /// Diff/changes preview
    Diff,
    /// Destructive operation warning
    Destructive,
    /// Command output preview
    Output,
    /// Help/documentation
    Help,
    /// Completion suggestions
    Completions,
}

#[derive(Clone, Debug)]
pub enum PreviewContent {
    /// Text lines
    Lines(Vec<String>),
    /// File entries
    Files(Vec<FilePreview>),
    /// Diff hunks
    Diff(Vec<DiffHunk>),
    /// Structured data
    Structured(Value),
    /// Empty
    Empty,
}

#[derive(Clone, Debug)]
pub struct FilePreview {
    pub path: String,
    pub size: Option<u64>,
    pub file_type: FileType,
    pub modified: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Clone, Copy, Debug)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug)]
pub struct PreviewWarning {
    pub level: WarningLevel,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum WarningLevel {
    Info,
    Warning,
    Danger,
}

#[derive(Clone, Debug)]
pub struct PreviewAction {
    pub key: String,
    pub label: String,
    pub action: ActionKind,
}

#[derive(Clone, Debug)]
pub enum ActionKind {
    Execute,
    Cancel,
    ShowMore,
    ShowHelp,
    Modify(String),
}

impl CommandPreview {
    pub fn new(config: PreviewConfig) -> Self {
        Self {
            input: String::new(),
            parsed: None,
            preview: None,
            state: PreviewState::Idle,
            debounce: None,
            config,
        }
    }
    
    /// Update input and trigger preview generation
    pub fn update_input(&mut self, input: &str) {
        self.input = input.to_string();
        self.debounce = Some(tokio::time::Instant::now());
        self.state = PreviewState::Pending;
        
        // Try to parse
        self.parsed = parse_command_line(input).ok();
    }
    
    /// Check if preview should be generated
    pub fn should_generate(&self) -> bool {
        if let Some(debounce) = self.debounce {
            let elapsed = debounce.elapsed();
            elapsed >= Duration::from_millis(self.config.debounce_ms)
                && matches!(self.state, PreviewState::Pending)
        } else {
            false
        }
    }
    
    /// Generate preview for current input
    pub async fn generate(&mut self) -> Option<PreviewResult> {
        self.state = PreviewState::Loading;
        
        let parsed = self.parsed.as_ref()?;
        let cmd_name = parsed.command_name()?;
        
        // Check if this command should have preview
        if !self.should_preview(&cmd_name) {
            self.state = PreviewState::Idle;
            return None;
        }
        
        let result = match cmd_name.as_str() {
            "rm" => self.preview_rm(parsed).await,
            "mv" => self.preview_mv(parsed).await,
            "cp" => self.preview_cp(parsed).await,
            "find" => self.preview_find(parsed).await,
            "grep" => self.preview_grep(parsed).await,
            "git" => self.preview_git(parsed).await,
            "ls" => self.preview_ls(parsed).await,
            _ => None,
        };
        
        if let Some(ref r) = result {
            self.preview = Some(r.clone());
            self.state = PreviewState::Ready;
        } else {
            self.state = PreviewState::Idle;
        }
        
        result
    }
    
    fn should_preview(&self, cmd: &str) -> bool {
        self.config.preview_commands.iter()
            .any(|c| c == cmd || cmd.starts_with(&format!("{}:", c)))
    }
    
    async fn preview_rm(&self, parsed: &ParsedCommand) -> Option<PreviewResult> {
        let args = parsed.args();
        let mut files = Vec::new();
        let mut total_size = 0u64;
        let recursive = args.iter().any(|a| a.contains('r'));
        
        for arg in args.iter().filter(|a| !a.starts_with('-')) {
            let path = Path::new(arg);
            if path.exists() {
                if path.is_dir() && recursive {
                    // Count directory contents
                    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_file() {
                            if let Ok(meta) = entry.metadata() {
                                total_size += meta.len();
                            }
                            files.push(FilePreview {
                                path: entry.path().to_string_lossy().to_string(),
                                size: entry.metadata().ok().map(|m| m.len()),
                                file_type: FileType::File,
                                modified: None,
                            });
                        }
                    }
                } else if path.is_file() {
                    if let Ok(meta) = path.metadata() {
                        total_size += meta.len();
                    }
                    files.push(FilePreview {
                        path: path.to_string_lossy().to_string(),
                        size: path.metadata().ok().map(|m| m.len()),
                        file_type: FileType::File,
                        modified: None,
                    });
                }
            }
        }
        
        if files.is_empty() {
            return None;
        }
        
        let count = files.len();
        files.truncate(self.config.max_items);
        
        let mut warnings = Vec::new();
        
        if count > 10 {
            warnings.push(PreviewWarning {
                level: WarningLevel::Warning,
                message: format!("This will delete {} files ({:.1} MB)", 
                    count, total_size as f64 / 1_000_000.0),
                details: None,
            });
        }
        
        if recursive && args.iter().any(|a| a == "-f" || a.contains("rf") || a.contains("fr")) {
            warnings.push(PreviewWarning {
                level: WarningLevel::Danger,
                message: "Force recursive delete - no confirmation!".to_string(),
                details: Some("Consider using trash instead of rm".to_string()),
            });
        }
        
        Some(PreviewResult {
            kind: PreviewKind::Destructive,
            content: PreviewContent::Files(files),
            warnings,
            actions: vec![
                PreviewAction {
                    key: "Enter".to_string(),
                    label: "Execute".to_string(),
                    action: ActionKind::Execute,
                },
                PreviewAction {
                    key: "Tab".to_string(),
                    label: "Show all files".to_string(),
                    action: ActionKind::ShowMore,
                },
                PreviewAction {
                    key: "Esc".to_string(),
                    label: "Cancel".to_string(),
                    action: ActionKind::Cancel,
                },
            ],
        })
    }
    
    async fn preview_find(&self, parsed: &ParsedCommand) -> Option<PreviewResult> {
        // Execute find with -print and limit results
        let args = parsed.args();
        let output = tokio::process::Command::new("find")
            .args(args)
            .arg("-print")
            .output()
            .await
            .ok()?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = stdout.lines()
            .take(self.config.max_items)
            .map(|s| s.to_string())
            .collect();
        
        let total = stdout.lines().count();
        
        let mut warnings = Vec::new();
        if total > self.config.max_items {
            warnings.push(PreviewWarning {
                level: WarningLevel::Info,
                message: format!("Showing {} of {} results", self.config.max_items, total),
                details: None,
            });
        }
        
        Some(PreviewResult {
            kind: PreviewKind::FileList,
            content: PreviewContent::Lines(lines),
            warnings,
            actions: vec![
                PreviewAction {
                    key: "Enter".to_string(),
                    label: "Execute".to_string(),
                    action: ActionKind::Execute,
                },
            ],
        })
    }
    
    async fn preview_git(&self, parsed: &ParsedCommand) -> Option<PreviewResult> {
        let args = parsed.args();
        let subcommand = args.first()?;
        
        match subcommand.as_str() {
            "status" => {
                let output = tokio::process::Command::new("git")
                    .args(["status", "--short"])
                    .output()
                    .await
                    .ok()?;
                
                let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .take(self.config.max_items)
                    .map(|s| s.to_string())
                    .collect();
                
                Some(PreviewResult {
                    kind: PreviewKind::Output,
                    content: PreviewContent::Lines(lines),
                    warnings: Vec::new(),
                    actions: vec![],
                })
            }
            
            "diff" => {
                let output = tokio::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output()
                    .await
                    .ok()?;
                
                let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .take(self.config.max_items)
                    .map(|s| s.to_string())
                    .collect();
                
                Some(PreviewResult {
                    kind: PreviewKind::Diff,
                    content: PreviewContent::Lines(lines),
                    warnings: Vec::new(),
                    actions: vec![],
                })
            }
            
            "rebase" if args.iter().any(|a| a == "-i" || a == "--interactive") => {
                Some(PreviewResult {
                    kind: PreviewKind::Help,
                    content: PreviewContent::Lines(vec![
                        "Interactive rebase commands:".to_string(),
                        "  pick   = use commit".to_string(),
                        "  reword = edit commit message".to_string(),
                        "  edit   = stop for amending".to_string(),
                        "  squash = meld into previous".to_string(),
                        "  fixup  = squash, discard message".to_string(),
                        "  drop   = remove commit".to_string(),
                    ]),
                    warnings: vec![PreviewWarning {
                        level: WarningLevel::Warning,
                        message: "This rewrites history".to_string(),
                        details: Some("Don't rebase published commits".to_string()),
                    }],
                    actions: vec![
                        PreviewAction {
                            key: "F1".to_string(),
                            label: "Full help".to_string(),
                            action: ActionKind::ShowHelp,
                        },
                    ],
                })
            }
            
            _ => None,
        }
    }
    
    async fn preview_ls(&self, parsed: &ParsedCommand) -> Option<PreviewResult> {
        let args = parsed.args();
        let path = args.iter()
            .find(|a| !a.starts_with('-'))
            .map(|s| s.as_str())
            .unwrap_or(".");
        
        let mut files = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()).take(self.config.max_items) {
                let meta = entry.metadata().ok();
                let file_type = if entry.file_type().ok().map(|t| t.is_dir()).unwrap_or(false) {
                    FileType::Directory
                } else {
                    FileType::File
                };
                
                files.push(FilePreview {
                    path: entry.file_name().to_string_lossy().to_string(),
                    size: meta.as_ref().map(|m| m.len()),
                    file_type,
                    modified: meta.and_then(|m| m.modified().ok())
                        .map(|t| DateTime::<Utc>::from(t)),
                });
            }
        }
        
        Some(PreviewResult {
            kind: PreviewKind::FileList,
            content: PreviewContent::Files(files),
            warnings: Vec::new(),
            actions: vec![],
        })
    }
    
    async fn preview_mv(&self, _parsed: &ParsedCommand) -> Option<PreviewResult> { None }
    async fn preview_cp(&self, _parsed: &ParsedCommand) -> Option<PreviewResult> { None }
    async fn preview_grep(&self, _parsed: &ParsedCommand) -> Option<PreviewResult> { None }
    
    pub fn preview(&self) -> Option<&PreviewResult> { self.preview.as_ref() }
    pub fn state(&self) -> &PreviewState { &self.state }
    pub fn clear(&mut self) {
        self.preview = None;
        self.state = PreviewState::Idle;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FileType { File, Directory, Symlink, Other }
```

## 13.3 Contextual Help System

```rust
// crates/colony-ui/src/help.rs

/// Real-time help system
pub struct HelpSystem {
    database: HelpDatabase,
    context: HelpContext,
    cache: LruCache<String, HelpEntry>,
}

#[derive(Clone, Debug)]
pub struct HelpEntry {
    pub name: String,
    pub brief: String,
    pub description: String,
    pub examples: Vec<HelpExample>,
    pub see_also: Vec<String>,
    pub options: Vec<HelpOption>,
    pub source: HelpSource,
}

#[derive(Clone, Debug)]
pub struct HelpExample {
    pub code: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct HelpOption {
    pub short: Option<String>,
    pub long: Option<String>,
    pub argument: Option<String>,
    pub description: String,
    pub required: bool,
}

#[derive(Clone, Debug)]
pub enum HelpSource { Builtin, ManPage, Tldr, Custom, Agent }

#[derive(Clone, Debug, Default)]
pub struct HelpContext {
    pub command: Option<String>,
    pub arg_position: usize,
    pub current_option: Option<String>,
    pub words: Vec<String>,
}

impl HelpSystem {
    pub fn new() -> Self {
        Self {
            database: HelpDatabase::new(),
            context: HelpContext::default(),
            cache: LruCache::new(100),
        }
    }
    
    pub fn update_context(&mut self, input: &str, cursor_pos: usize) {
        let words: Vec<&str> = input[..cursor_pos].split_whitespace().collect();
        self.context = HelpContext {
            command: words.first().map(|s| s.to_string()),
            arg_position: words.len().saturating_sub(1),
            current_option: words.last()
                .filter(|w| w.starts_with('-'))
                .map(|s| s.to_string()),
            words: words.iter().map(|s| s.to_string()).collect(),
        };
    }
    
    pub fn get_contextual_help(&mut self) -> Option<ContextualHelp> {
        let cmd = self.context.command.as_ref()?;
        if let Some(entry) = self.cache.get(cmd) {
            return Some(self.build_contextual_help(entry));
        }
        let entry = self.database.lookup(cmd)?;
        self.cache.put(cmd.clone(), entry.clone());
        Some(self.build_contextual_help(&entry))
    }
    
    fn build_contextual_help(&self, entry: &HelpEntry) -> ContextualHelp {
        if let Some(ref opt) = self.context.current_option {
            if let Some(option) = entry.options.iter()
                .find(|o| o.short.as_ref() == Some(opt) || o.long.as_ref() == Some(opt)) 
            {
                return ContextualHelp {
                    title: format!("{} {}", entry.name, opt),
                    content: option.description.clone(),
                    suggestions: Vec::new(),
                };
            }
        }
        let example = entry.examples.first().map(|e| format!("Example: {}", e.code));
        ContextualHelp {
            title: entry.name.clone(),
            content: entry.brief.clone(),
            suggestions: example.into_iter().collect(),
        }
    }
    
    pub fn get_full_help(&mut self, topic: &str) -> Option<HelpEntry> {
        if let Some(entry) = self.cache.get(topic) {
            return Some(entry.clone());
        }
        let entry = self.database.lookup(topic)?;
        self.cache.put(topic.to_string(), entry.clone());
        Some(entry)
    }
    
    pub fn search(&self, query: &str) -> Vec<HelpSearchResult> {
        self.database.search(query)
    }
}

#[derive(Clone, Debug)]
pub struct ContextualHelp {
    pub title: String,
    pub content: String,
    pub suggestions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct HelpSearchResult {
    pub name: String,
    pub brief: String,
    pub relevance: f32,
}

pub struct HelpDatabase {
    builtins: IndexMap<String, HelpEntry>,
    tldr_cache: Option<PathBuf>,
}

impl HelpDatabase {
    pub fn new() -> Self {
        let mut db = Self {
            builtins: IndexMap::new(),
            tldr_cache: dirs::cache_dir().map(|p| p.join("colony/tldr")),
        };
        db.load_builtins();
        db
    }
    
    fn load_builtins(&mut self) {
        // Colony builtin help entries loaded here
    }
    
    pub fn lookup(&self, topic: &str) -> Option<HelpEntry> {
        if let Some(entry) = self.builtins.get(topic) { return Some(entry.clone()); }
        if let Some(entry) = self.lookup_tldr(topic) { return Some(entry); }
        self.parse_man_page(topic)
    }
    
    fn lookup_tldr(&self, topic: &str) -> Option<HelpEntry> {
        let cache_dir = self.tldr_cache.as_ref()?;
        let path = cache_dir.join(format!("{}.md", topic));
        if !path.exists() { return None; }
        let content = std::fs::read_to_string(&path).ok()?;
        self.parse_tldr(&content, topic)
    }
    
    fn parse_tldr(&self, content: &str, name: &str) -> Option<HelpEntry> {
        let mut brief = String::new();
        let mut examples = Vec::new();
        let mut current_desc = String::new();
        for line in content.lines() {
            if line.starts_with('>') { brief = line[1..].trim().to_string(); }
            else if line.starts_with('-') { current_desc = line[1..].trim().to_string(); }
            else if line.starts_with('`') && line.ends_with('`') {
                examples.push(HelpExample {
                    code: line[1..line.len()-1].to_string(),
                    description: current_desc.clone(),
                });
            }
        }
        Some(HelpEntry {
            name: name.to_string(), brief, description: String::new(),
            examples, see_also: Vec::new(), options: Vec::new(), source: HelpSource::Tldr,
        })
    }
    
    fn parse_man_page(&self, topic: &str) -> Option<HelpEntry> {
        let output = std::process::Command::new("man").args(["-f", topic]).output().ok()?;
        let brief = String::from_utf8_lossy(&output.stdout).lines().next()?.to_string();
        Some(HelpEntry {
            name: topic.to_string(), brief, description: String::new(),
            examples: Vec::new(), see_also: Vec::new(), options: Vec::new(),
            source: HelpSource::ManPage,
        })
    }
    
    pub fn search(&self, query: &str) -> Vec<HelpSearchResult> {
        let query_lower = query.to_lowercase();
        self.builtins.iter()
            .filter_map(|(name, entry)| {
                let relevance = if name.contains(&query_lower) { 1.0 }
                    else if entry.brief.to_lowercase().contains(&query_lower) { 0.7 }
                    else { return None };
                Some(HelpSearchResult { name: name.clone(), brief: entry.brief.clone(), relevance })
            })
            .collect()
    }
}
```

## 13.4 Safety Checking

```rust
// crates/colony-runtime/src/safety.rs

pub struct SafetyChecker {
    config: SafetyConfig,
    patterns: Vec<DangerPattern>,
}

#[derive(Clone, Debug)]
pub struct SafetyConfig {
    pub enabled: bool,
    pub require_confirmation: bool,
    pub block_dangerous: bool,
    pub custom_patterns: Vec<String>,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self { enabled: true, require_confirmation: true, block_dangerous: true, custom_patterns: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct DangerPattern {
    pub pattern: String,
    pub regex: Regex,
    pub level: DangerLevel,
    pub description: String,
    pub suggestion: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DangerLevel { Info, Warning, Danger, Critical }

#[derive(Clone, Debug)]
pub struct SafetyCheck {
    pub passed: bool,
    pub level: DangerLevel,
    pub warnings: Vec<SafetyWarning>,
    pub blocked: bool,
}

#[derive(Clone, Debug)]
pub struct SafetyWarning {
    pub level: DangerLevel,
    pub message: String,
    pub pattern_matched: String,
    pub suggestion: Option<String>,
}

impl SafetyChecker {
    pub fn new(config: SafetyConfig) -> Self {
        let mut checker = Self { config, patterns: Vec::new() };
        checker.load_default_patterns();
        checker
    }
    
    fn load_default_patterns(&mut self) {
        let defaults = vec![
            // Critical
            (r"rm\s+-rf\s+/\s*$", DangerLevel::Critical, "Removes entire filesystem", Some("Use specific paths")),
            (r"rm\s+-rf\s+/\*", DangerLevel::Critical, "Removes all root directories", None),
            (r"dd\s+.*of=/dev/[sh]d[a-z]\b", DangerLevel::Critical, "Overwrites disk device", None),
            (r":(){:|:&};:", DangerLevel::Critical, "Fork bomb", None),
            // Danger
            (r"rm\s+-rf\s+~", DangerLevel::Danger, "Removes home directory", None),
            (r"chmod\s+-R\s+777", DangerLevel::Danger, "World-writable permissions", None),
            // Warning
            (r"rm\s+-rf", DangerLevel::Warning, "Recursive force delete", Some("Consider trash")),
            (r"git\s+push.*--force\b", DangerLevel::Warning, "Force push", Some("Use --force-with-lease")),
            (r"git\s+reset\s+--hard", DangerLevel::Warning, "Discards uncommitted changes", None),
        ];
        
        for (pattern, level, desc, suggestion) in defaults {
            if let Ok(regex) = Regex::new(pattern) {
                self.patterns.push(DangerPattern {
                    pattern: pattern.to_string(), regex, level,
                    description: desc.to_string(),
                    suggestion: suggestion.map(|s| s.to_string()),
                });
            }
        }
    }
    
    pub fn check(&self, command: &str) -> SafetyCheck {
        if !self.config.enabled {
            return SafetyCheck { passed: true, level: DangerLevel::Info, warnings: Vec::new(), blocked: false };
        }
        
        let mut warnings = Vec::new();
        let mut max_level = DangerLevel::Info;
        
        for pattern in &self.patterns {
            if pattern.regex.is_match(command) {
                warnings.push(SafetyWarning {
                    level: pattern.level,
                    message: pattern.description.clone(),
                    pattern_matched: pattern.pattern.clone(),
                    suggestion: pattern.suggestion.clone(),
                });
                if pattern.level > max_level { max_level = pattern.level; }
            }
        }
        
        let blocked = self.config.block_dangerous && max_level >= DangerLevel::Critical;
        let passed = warnings.is_empty() || (!blocked && !self.config.require_confirmation);
        
        SafetyCheck { passed, level: max_level, warnings, blocked }
    }
}
```

## 13.5 Completion Engine

```rust
// crates/colony-ui/src/completions.rs

pub struct CompletionEngine {
    file_completer: FileCompleter,
    command_completer: CommandCompleter,
    history_completer: HistoryCompleter,
    custom_completers: IndexMap<String, Box<dyn Completer>>,
}

pub trait Completer: Send + Sync {
    fn complete(&self, context: &CompletionContext) -> Vec<Completion>;
}

#[derive(Clone, Debug)]
pub struct CompletionContext {
    pub line: String,
    pub cursor: usize,
    pub word: String,
    pub word_start: usize,
    pub previous_words: Vec<String>,
    pub command: Option<String>,
    pub arg_position: usize,
}

#[derive(Clone, Debug)]
pub struct Completion {
    pub text: String,
    pub display: String,
    pub description: Option<String>,
    pub kind: CompletionKind,
    pub priority: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    File, Directory, Command, Builtin, Alias, Function,
    Variable, Option, Argument, History, Agent, Keyword,
}

impl CompletionEngine {
    pub fn new() -> Self {
        Self {
            file_completer: FileCompleter::new(),
            command_completer: CommandCompleter::new(),
            history_completer: HistoryCompleter::new(),
            custom_completers: IndexMap::new(),
        }
    }
    
    pub fn register(&mut self, command: &str, completer: Box<dyn Completer>) {
        self.custom_completers.insert(command.to_string(), completer);
    }
    
    pub fn complete(&self, context: &CompletionContext) -> Vec<Completion> {
        let mut completions = Vec::new();
        
        if context.arg_position == 0 {
            completions.extend(self.command_completer.complete(context));
            completions.extend(self.history_completer.complete(context));
        } else {
            if let Some(cmd) = &context.command {
                if let Some(completer) = self.custom_completers.get(cmd) {
                    completions.extend(completer.complete(context));
                }
            }
            if completions.is_empty() || context.word.starts_with('/') || context.word.starts_with('.') {
                completions.extend(self.file_completer.complete(context));
            }
            if context.word.starts_with('$') {
                completions.extend(self.complete_variables(context));
            }
        }
        
        completions.sort_by(|a, b| b.priority.cmp(&a.priority));
        completions.dedup_by(|a, b| a.text == b.text);
        completions
    }
    
    fn complete_variables(&self, context: &CompletionContext) -> Vec<Completion> {
        let prefix = &context.word[1..];
        std::env::vars()
            .filter(|(k, _)| k.to_lowercase().starts_with(&prefix.to_lowercase()))
            .map(|(k, v)| Completion {
                text: format!("${}", k),
                display: format!("${}", k),
                description: Some(v.chars().take(30).collect()),
                kind: CompletionKind::Variable,
                priority: 50,
            })
            .collect()
    }
}

pub struct FileCompleter { show_hidden: bool }
impl FileCompleter { pub fn new() -> Self { Self { show_hidden: false } } }

impl Completer for FileCompleter {
    fn complete(&self, context: &CompletionContext) -> Vec<Completion> {
        let word = &context.word;
        let (dir, prefix) = if let Some(pos) = word.rfind('/') {
            (&word[..=pos], &word[pos+1..])
        } else { (".", word.as_str()) };
        
        let dir_path = PathBuf::from(shellexpand::tilde(dir).as_ref());
        let mut completions = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(&dir_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') && !self.show_hidden && !prefix.starts_with('.') { continue; }
                if name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let display = if is_dir { format!("{}/", name) } else { name.clone() };
                    let text = if dir == "." { display.clone() } else { format!("{}{}", dir, display) };
                    completions.push(Completion {
                        text, display, description: None,
                        kind: if is_dir { CompletionKind::Directory } else { CompletionKind::File },
                        priority: if is_dir { 40 } else { 30 },
                    });
                }
            }
        }
        completions
    }
}

pub struct CommandCompleter { commands: Vec<String>, builtins: Vec<String> }
impl CommandCompleter {
    pub fn new() -> Self {
        let builtins = vec!["cd", "echo", "exit", "export", "alias", "source", "var", "const", 
            "fn", "proc", "agent", "context", "llm", "ts", "mcp", "pane", "buffer"]
            .into_iter().map(|s| s.to_string()).collect();
        let mut completer = Self { commands: Vec::new(), builtins };
        completer.scan_path();
        completer
    }
    
    fn scan_path(&mut self) {
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        if let Ok(ft) = entry.file_type() {
                            if ft.is_file() || ft.is_symlink() {
                                self.commands.push(entry.file_name().to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
        self.commands.sort();
        self.commands.dedup();
    }
}

impl Completer for CommandCompleter {
    fn complete(&self, context: &CompletionContext) -> Vec<Completion> {
        let prefix = &context.word;
        let mut completions: Vec<_> = self.builtins.iter()
            .filter(|b| b.starts_with(prefix))
            .map(|b| Completion {
                text: b.clone(), display: b.clone(),
                description: Some("builtin".to_string()),
                kind: CompletionKind::Builtin, priority: 80,
            })
            .chain(self.commands.iter()
                .filter(|c| c.starts_with(prefix))
                .map(|c| Completion {
                    text: c.clone(), display: c.clone(), description: None,
                    kind: CompletionKind::Command, priority: 60,
                }))
            .collect();
        completions
    }
}

pub struct HistoryCompleter { max_suggestions: usize }
impl HistoryCompleter { pub fn new() -> Self { Self { max_suggestions: 5 } } }
impl Completer for HistoryCompleter {
    fn complete(&self, _context: &CompletionContext) -> Vec<Completion> { Vec::new() }
}
```

## 13.6 Interactive Commands

```colony
# Preview settings
set preview on
set preview.debounce 200

# Safety settings
set safety on
set safety.confirm on
set safety.block on

# Help system
help                  # General help
help cd               # Specific command
help --search "git"   # Search topics

# Key bindings
bind "ctrl+h" "help"
bind "ctrl+p" "preview:toggle"
bind "F1" "help:context"

# Completion settings
set completion.fuzzy on
set completion.max 20
```

## 13.7 Prompt System

Colony uses a **reactive prompt architecture** where the shell generates prompt state and the terminal renders it. This separation enables rich, contextual prompts that update in response to environment changes without blocking shell execution.

### 13.7.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Prompt Architecture                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────┐          ┌─────────────────────────────────┐  │
│  │     Colony Shell        │          │       Colony Terminal           │  │
│  │                         │          │                                 │  │
│  │  ┌─────────────────┐   │  OSC     │   ┌─────────────────────────┐   │  │
│  │  │  PromptState    │───┼─────────▶│   │   Prompt Renderer       │   │  │
│  │  │  - cwd          │   │  2337    │   │   - Format/theme        │   │  │
│  │  │  - git_info     │   │ Prompt=  │   │   - Layout              │   │  │
│  │  │  - user/host    │   │          │   │   - Colors/icons        │   │  │
│  │  │  - exit_status  │   │          │   └─────────────────────────┘   │  │
│  │  │  - context[]    │   │          │                                 │  │
│  │  └─────────────────┘   │          │           ▼                     │  │
│  │          ▲              │          │   ┌─────────────────────────┐   │  │
│  │          │              │          │   │  ~/src/colony (main) $  │   │  │
│  │  ┌───────┴───────┐     │          │   └─────────────────────────┘   │  │
│  │  │  Tuple Space  │     │          │                                 │  │
│  │  │  ("prompt",   │     │          │                                 │  │
│  │  │   field,      │     │          │                                 │  │
│  │  │   value)      │     │          │                                 │  │
│  │  └───────────────┘     │          │                                 │  │
│  └─────────────────────────┘          └─────────────────────────────────┘  │
│                                                                             │
│  Reactive Flow: cd → tuple out → PromptState update → OSC emit → render    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 13.7.2 Prompt State

```rust
// crates/colony-shell/src/prompt.rs

use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// Complete prompt state sent to terminal
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptState {
    /// Current working directory
    pub cwd: PathBuf,
    /// Shortened display path (with ~ for home)
    pub display_path: String,
    /// Username
    pub user: String,
    /// Hostname
    pub host: String,
    /// Last command exit status
    pub exit_status: i32,
    /// Git repository information (if in repo)
    pub git: Option<GitInfo>,
    /// Active agent information
    pub agent: Option<AgentInfo>,
    /// Environment context markers
    pub context: Vec<ContextMarker>,
    /// Shell nesting level
    pub shlvl: u32,
    /// Whether shell is running in a job
    pub in_job: bool,
    /// Custom segments from extensions
    pub custom: Vec<PromptSegment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitInfo {
    /// Current branch name
    pub branch: String,
    /// Detached HEAD commit (if detached)
    pub commit: Option<String>,
    /// Working tree status
    pub status: GitStatus,
    /// Number of commits ahead of upstream
    pub ahead: u32,
    /// Number of commits behind upstream
    pub behind: u32,
    /// Current operation (rebase, merge, etc.)
    pub operation: Option<GitOperation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitStatus {
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
    pub conflicted: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GitOperation {
    Rebase { current: u32, total: u32 },
    Merge,
    CherryPick,
    Bisect,
    Revert,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent name
    pub name: String,
    /// Current state
    pub state: AgentState,
    /// Brief status message
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Thinking,
    Executing,
    WaitingApproval,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextMarker {
    /// Marker type (env, project, mode)
    pub kind: ContextKind,
    /// Display label
    pub label: String,
    /// Optional color hint
    pub color: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ContextKind {
    /// Environment (dev, staging, prod)
    Environment,
    /// Project context loaded
    Project,
    /// Special mode (debug, diagnostic)
    Mode,
    /// Virtual environment (python, nix)
    VirtualEnv,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptSegment {
    /// Segment identifier
    pub id: String,
    /// Display content
    pub content: String,
    /// Position hint
    pub position: SegmentPosition,
    /// Optional icon
    pub icon: Option<String>,
    /// Optional color
    pub color: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SegmentPosition {
    Left,
    Right,
    BeforeCwd,
    AfterCwd,
}
```

### 13.7.3 Prompt Manager

```rust
// crates/colony-shell/src/prompt.rs

use crate::tuple::TupleSpace;
use std::sync::Arc;

/// Manages prompt state and reactive updates
pub struct PromptManager {
    state: PromptState,
    tuple_space: Arc<TupleSpace>,
    config: PromptConfig,
    git_cache: Option<(PathBuf, GitInfo, std::time::Instant)>,
}

#[derive(Clone, Debug)]
pub struct PromptConfig {
    /// Enable git information
    pub git_enabled: bool,
    /// Git info cache TTL in milliseconds
    pub git_cache_ms: u64,
    /// Path display style
    pub path_style: PathStyle,
    /// Maximum path components to show
    pub max_path_components: usize,
    /// Enable agent status in prompt
    pub show_agent: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum PathStyle {
    /// Full path
    Full,
    /// Replace home with ~
    Tilde,
    /// Shorten intermediate directories
    Shortened,
    /// Only basename
    Basename,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            git_enabled: true,
            git_cache_ms: 500,
            path_style: PathStyle::Tilde,
            max_path_components: 4,
            show_agent: true,
        }
    }
}

impl PromptManager {
    pub fn new(tuple_space: Arc<TupleSpace>, config: PromptConfig) -> Self {
        let state = PromptState {
            cwd: std::env::current_dir().unwrap_or_default(),
            display_path: String::new(),
            user: std::env::var("USER").unwrap_or_else(|_| "user".to_string()),
            host: gethostname::gethostname().to_string_lossy().to_string(),
            exit_status: 0,
            git: None,
            agent: None,
            context: Vec::new(),
            shlvl: std::env::var("SHLVL").ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            in_job: false,
            custom: Vec::new(),
        };

        let mut manager = Self {
            state,
            tuple_space,
            config,
            git_cache: None,
        };
        manager.refresh_all();
        manager
    }

    /// Subscribe to state changes via tuple space
    pub fn subscribe(&self) {
        // Spawn listener for directory changes
        let ts = self.tuple_space.clone();
        tokio::spawn(async move {
            loop {
                // Block waiting for dir_change tuple
                if let Some(tuple) = ts.rd(&["dir_change".into(), Value::Wildcard]).await {
                    // Trigger prompt refresh
                    ts.out(Tuple::from(vec![
                        "prompt_refresh".into(),
                        "cwd".into(),
                    ]));
                }
            }
        });

        // Spawn listener for env changes
        let ts = self.tuple_space.clone();
        tokio::spawn(async move {
            loop {
                if let Some(_) = ts.rd(&["env_change".into(), Value::Wildcard, Value::Wildcard, Value::Wildcard]).await {
                    ts.out(Tuple::from(vec![
                        "prompt_refresh".into(),
                        "env".into(),
                    ]));
                }
            }
        });
    }

    /// Refresh all prompt state
    pub fn refresh_all(&mut self) {
        self.refresh_cwd();
        self.refresh_git();
    }

    /// Refresh current directory state
    pub fn refresh_cwd(&mut self) {
        if let Ok(cwd) = std::env::current_dir() {
            self.state.cwd = cwd.clone();
            self.state.display_path = self.format_path(&cwd);
        }
    }

    /// Refresh git information
    pub fn refresh_git(&mut self) {
        if !self.config.git_enabled {
            self.state.git = None;
            return;
        }

        // Check cache
        if let Some((ref cached_path, ref info, instant)) = self.git_cache {
            if cached_path == &self.state.cwd
                && instant.elapsed().as_millis() < self.config.git_cache_ms as u128
            {
                self.state.git = Some(info.clone());
                return;
            }
        }

        // Fetch git info (this is fast with libgit2)
        self.state.git = self.get_git_info();

        // Update cache
        if let Some(ref info) = self.state.git {
            self.git_cache = Some((
                self.state.cwd.clone(),
                info.clone(),
                std::time::Instant::now(),
            ));
        }
    }

    fn get_git_info(&self) -> Option<GitInfo> {
        let repo = git2::Repository::discover(&self.state.cwd).ok()?;

        let head = repo.head().ok()?;
        let branch = if head.is_branch() {
            head.shorthand()?.to_string()
        } else {
            // Detached HEAD
            head.target()?.to_string()[..7].to_string()
        };

        let mut status = GitStatus {
            staged: 0,
            modified: 0,
            untracked: 0,
            conflicted: 0,
        };

        if let Ok(statuses) = repo.statuses(None) {
            for entry in statuses.iter() {
                let s = entry.status();
                if s.is_index_new() || s.is_index_modified() || s.is_index_deleted() {
                    status.staged += 1;
                }
                if s.is_wt_modified() || s.is_wt_deleted() {
                    status.modified += 1;
                }
                if s.is_wt_new() {
                    status.untracked += 1;
                }
                if s.is_conflicted() {
                    status.conflicted += 1;
                }
            }
        }

        // Check for ongoing operation
        let operation = if repo.state() == git2::RepositoryState::Rebase
            || repo.state() == git2::RepositoryState::RebaseInteractive
        {
            Some(GitOperation::Rebase { current: 0, total: 0 })
        } else if repo.state() == git2::RepositoryState::Merge {
            Some(GitOperation::Merge)
        } else {
            None
        };

        Some(GitInfo {
            branch,
            commit: None,
            status,
            ahead: 0,  // TODO: Calculate from tracking branch
            behind: 0,
            operation,
        })
    }

    fn format_path(&self, path: &PathBuf) -> String {
        let home = dirs::home_dir();

        match self.config.path_style {
            PathStyle::Full => path.display().to_string(),

            PathStyle::Tilde => {
                if let Some(ref home) = home {
                    if let Ok(stripped) = path.strip_prefix(home) {
                        return format!("~/{}", stripped.display());
                    }
                }
                path.display().to_string()
            }

            PathStyle::Shortened => {
                let components: Vec<_> = path.components().collect();
                if components.len() <= self.config.max_path_components {
                    return self.format_path_tilde(path, &home);
                }

                // Shorten intermediate components
                let mut parts = Vec::new();
                for (i, comp) in components.iter().enumerate() {
                    if i == 0 || i >= components.len() - 2 {
                        parts.push(comp.as_os_str().to_string_lossy().to_string());
                    } else {
                        // First char only
                        let s = comp.as_os_str().to_string_lossy();
                        parts.push(s.chars().next().unwrap_or('.').to_string());
                    }
                }
                parts.join("/")
            }

            PathStyle::Basename => {
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string())
            }
        }
    }

    fn format_path_tilde(&self, path: &PathBuf, home: &Option<PathBuf>) -> String {
        if let Some(ref home) = home {
            if let Ok(stripped) = path.strip_prefix(home) {
                return format!("~/{}", stripped.display());
            }
        }
        path.display().to_string()
    }

    /// Update exit status after command execution
    pub fn set_exit_status(&mut self, status: i32) {
        self.state.exit_status = status;
    }

    /// Update agent state
    pub fn set_agent(&mut self, agent: Option<AgentInfo>) {
        self.state.agent = agent;
    }

    /// Add context marker
    pub fn add_context(&mut self, marker: ContextMarker) {
        self.state.context.push(marker);
    }

    /// Clear context markers
    pub fn clear_context(&mut self) {
        self.state.context.clear();
    }

    /// Get current state for emission
    pub fn state(&self) -> &PromptState {
        &self.state
    }

    /// Emit prompt state via OSC 2337
    pub fn emit<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let payload = serde_json::to_string(&self.state)?;
        crate::osc::emit(writer, "Prompt=state", &payload)
    }
}
```

### 13.7.4 OSC 2337 Prompt Commands

| Command | Direction | Description |
|---------|-----------|-------------|
| `Prompt=state` | Shell→Terminal | Full prompt state |
| `Prompt=update` | Shell→Terminal | Partial state update |
| `Prompt=ready` | Shell→Terminal | Shell ready for input |
| `Prompt=busy` | Shell→Terminal | Shell executing command |

**Prompt=state payload:**
```json
{
  "cwd": "/home/user/src/colony",
  "display_path": "~/src/colony",
  "user": "user",
  "host": "hostname",
  "exit_status": 0,
  "git": {
    "branch": "main",
    "status": { "staged": 2, "modified": 1, "untracked": 0, "conflicted": 0 },
    "ahead": 1,
    "behind": 0,
    "operation": null
  },
  "agent": {
    "name": "assistant",
    "state": "Idle",
    "message": null
  },
  "context": [
    { "kind": "Project", "label": "colony-shell", "color": "blue" }
  ],
  "shlvl": 1,
  "in_job": false,
  "custom": []
}
```

**Prompt=update payload (partial):**
```json
{
  "exit_status": 1,
  "git": { "status": { "modified": 2 } }
}
```

### 13.7.5 Terminal Rendering

The terminal is responsible for rendering the prompt based on received state. This allows:
- User-customizable themes without shell changes
- Rich rendering (colors, icons, Unicode)
- Responsive layouts (right-aligned segments, multi-line)

Example rendering configurations (terminal-side):

```toml
# colony-terminal prompt theme
[prompt]
style = "powerline"  # or "minimal", "classic", "starship"

[prompt.left]
segments = ["context", "path", "git"]

[prompt.right]
segments = ["exit_status", "agent", "time"]

[prompt.icons]
git_branch = ""
git_modified = "✱"
git_staged = "●"
folder = ""
home = ""

[prompt.colors]
path = "blue"
git_clean = "green"
git_dirty = "yellow"
error = "red"
```

### 13.7.6 Fallback Prompt

When running in a non-Colony terminal, the shell emits a simple text prompt:

```rust
impl PromptManager {
    /// Generate fallback text prompt for non-Colony terminals
    pub fn fallback_prompt(&self) -> String {
        let mut prompt = String::new();

        // Exit status indicator
        if self.state.exit_status != 0 {
            prompt.push_str(&format!("[{}] ", self.state.exit_status));
        }

        // Path
        prompt.push_str(&self.state.display_path);

        // Git branch
        if let Some(ref git) = self.state.git {
            prompt.push_str(&format!(" ({})", git.branch));
            if git.status.modified > 0 || git.status.staged > 0 {
                prompt.push('*');
            }
        }

        // Prompt character
        prompt.push_str(" $ ");

        prompt
    }

    /// Check if running in Colony terminal
    pub fn is_colony_terminal(&self) -> bool {
        std::env::var("COLONY_TERMINAL").is_ok()
    }
}
```

### 13.7.7 Reactive Updates via Tuple Space

The prompt subscribes to shell state changes through the tuple space:

```rust
// Emit directory change (from cd builtin)
tuple_space.out(Tuple::from(vec![
    "dir_change".into(),
    old_path.to_string_lossy().into(),
    new_path.to_string_lossy().into(),
]));

// Emit environment change (from export/unset)
tuple_space.out(Tuple::from(vec![
    "env_change".into(),
    var_name.into(),
    old_value.unwrap_or_default().into(),
    new_value.into(),
]));

// Emit command completion
tuple_space.out(Tuple::from(vec![
    "cmd_complete".into(),
    exit_status.into(),
    duration_ms.into(),
]));
```

### 13.7.8 Configuration

```colony
# Prompt configuration
set prompt.git on
set prompt.git.cache_ms 500
set prompt.path tilde        # full, tilde, shortened, basename
set prompt.path.max 4
set prompt.agent on

# Context markers (loaded from .colonyrc)
context add --type project --label "colony-shell"
context add --type env --label "dev" --color green
```

---

*Continue to Part 14: Builtins Reference*
