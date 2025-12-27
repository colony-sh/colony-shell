//! Expansion context - provides shell state for expansions

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::cell::Cell;
#[cfg(feature = "secure")]
use std::path::Path;
#[cfg(feature = "secure")]
use std::sync::OnceLock;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Global keychain cache for secure variable decryption
/// Initialized on first use, then cached for performance
#[cfg(feature = "secure")]
static SECURE_KEY_CACHE: OnceLock<Option<[u8; 32]>> = OnceLock::new();

#[cfg(feature = "secure")]
fn get_cached_secure_key() -> Option<&'static [u8; 32]> {
    SECURE_KEY_CACHE.get_or_init(|| {
        match colony_secure::Keychain::new() {
            Ok(keychain) => keychain.get_key().ok(),
            Err(_) => None,
        }
    }).as_ref()
}

#[cfg(feature = "secure")]
fn maybe_decrypt_value(value: String) -> String {
    if colony_secure::is_encrypted(&value) {
        if let Some(key) = get_cached_secure_key() {
            if let Ok(decrypted) = colony_secure::decrypt(&value, key) {
                return decrypted;
            }
        }
    }
    value
}

/// System variables that don't change during shell execution
/// Computed once at shell startup
#[derive(Debug, Clone)]
pub struct SystemVars {
    /// Real user ID
    pub uid: u32,
    /// Effective user ID
    pub euid: u32,
    /// Parent process ID
    pub ppid: i32,
    /// Machine hostname
    pub hostname: String,
    /// CPU architecture (e.g., "aarch64", "x86_64")
    pub hosttype: String,
    /// OS type with version (e.g., "darwin24.4.0", "linux-gnu")
    pub ostype: String,
    /// Full machine type (e.g., "aarch64-apple-darwin24.4.0")
    pub machtype: String,
    /// Path to shell executable (may be overridden by env)
    pub shell_path: String,
}

impl SystemVars {
    pub fn new() -> Self {
        let uid = nix::unistd::getuid().as_raw();
        let euid = nix::unistd::geteuid().as_raw();
        let ppid = nix::unistd::getppid().as_raw();
        let hostname = Self::get_hostname();
        let hosttype = Self::get_hosttype();
        let ostype = Self::get_ostype();
        let machtype = Self::get_machtype(&hosttype, &ostype);
        let shell_path = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "/bin/sh".to_string());

        Self {
            uid,
            euid,
            ppid,
            hostname,
            hosttype,
            ostype,
            machtype,
            shell_path,
        }
    }

    fn get_hostname() -> String {
        if let Ok(hostname) = nix::unistd::gethostname() {
            return hostname.to_string_lossy().to_string();
        }
        std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string())
    }

    fn get_hosttype() -> String {
        std::env::consts::ARCH.to_string()
    }

    fn get_ostype() -> String {
        #[cfg(target_os = "macos")]
        {
            let version = Self::get_darwin_version();
            format!("darwin{}", version)
        }

        #[cfg(target_os = "linux")]
        {
            "linux-gnu".to_string()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            std::env::consts::OS.to_string()
        }
    }

    #[cfg(target_os = "macos")]
    fn get_darwin_version() -> String {
        unsafe {
            let mut utsname: libc::utsname = std::mem::zeroed();
            if libc::uname(&mut utsname) == 0 {
                let release = std::ffi::CStr::from_ptr(utsname.release.as_ptr())
                    .to_string_lossy()
                    .to_string();
                return release;
            }
        }
        String::new()
    }

    #[cfg(not(target_os = "macos"))]
    fn get_darwin_version() -> String {
        String::new()
    }

    fn get_machtype(hosttype: &str, ostype: &str) -> String {
        #[cfg(target_os = "macos")]
        let vendor = "apple";

        #[cfg(target_os = "linux")]
        let vendor = "pc";

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let vendor = "unknown";

        format!("{}-{}-{}", hosttype, vendor, ostype)
    }
}

impl Default for SystemVars {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellOptions {
    pub errexit: bool,
    pub nounset: bool,
    pub xtrace: bool,
    pub noglob: bool,
    pub noexec: bool,
    pub verbose: bool,
    pub noclobber: bool,
    pub pipefail: bool,
}

impl ShellOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Display for ShellOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut flags = String::new();
        if self.noclobber { flags.push('C'); }
        if self.errexit { flags.push('e'); }
        if self.noglob { flags.push('f'); }
        if self.noexec { flags.push('n'); }
        if self.nounset { flags.push('u'); }
        if self.verbose { flags.push('v'); }
        if self.xtrace { flags.push('x'); }
        write!(f, "{}", flags)
    }
}

/// Bash shopt-style options (controlled by `shopt -s/-u`)
/// These are separate from POSIX `set` options in ShellOptions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShoptOptions {
    /// Extended glob patterns: ?(pat), *(pat), +(pat), @(pat), !(pat)
    pub extglob: bool,
    /// If set, `**` matches directories recursively
    pub globstar: bool,
    /// If set, patterns that match no files expand to empty string
    pub nullglob: bool,
    /// If set, `*` and `?` match files beginning with `.`
    pub dotglob: bool,
    /// If set, patterns that match no files result in an error
    pub failglob: bool,
    /// If set, glob matching is case-insensitive
    pub nocaseglob: bool,
    /// If set, case/[[ matching is case-insensitive
    pub nocasematch: bool,
    /// If set, last command in pipeline runs in current shell
    pub lastpipe: bool,
    /// If set, aliases are expanded (default on for interactive)
    pub expand_aliases: bool,
    /// If set, use PATH for source/. command
    pub sourcepath: bool,
    /// If set, check window size after each command
    pub checkwinsize: bool,
    /// If set, enable ! style history substitution
    pub histexpand: bool,
    /// If set, enable programmable completion
    pub progcomp: bool,
    /// If set, auto-correct minor cd spelling errors
    pub cdspell: bool,
    /// If set, check for running jobs before exit
    pub checkjobs: bool,
    /// If set, save multi-line commands in history as single entry
    pub cmdhist: bool,
    /// If set, auto-correct minor directory spelling errors
    pub dirspell: bool,
    /// If set, enable editing of failed history substitution
    pub histreedit: bool,
    /// If set, enable hostname completion
    pub hostcomplete: bool,
    /// If set, save multi-line commands with embedded newlines
    pub lithist: bool,
    /// If set, don't complete on empty command line
    pub no_empty_cmd_completion: bool,
    /// If set, expand prompt strings
    pub promptvars: bool,
    /// If set, warn if shift count exceeds positional params
    pub shift_verbose: bool,
    /// If set, echo interprets backslash escapes (XPG style)
    pub xpg_echo: bool,
}

impl Default for ShoptOptions {
    fn default() -> Self {
        Self {
            // Colony enables extglob by default for better compatibility
            extglob: true,
            globstar: false,
            nullglob: false,
            dotglob: false,
            failglob: false,
            nocaseglob: false,
            nocasematch: false,
            lastpipe: false,
            expand_aliases: true,
            sourcepath: true,
            checkwinsize: true,
            histexpand: false,
            progcomp: true,
            cdspell: false,
            checkjobs: false,
            cmdhist: true,
            dirspell: false,
            histreedit: false,
            hostcomplete: true,
            lithist: false,
            no_empty_cmd_completion: false,
            promptvars: true,
            shift_verbose: false,
            xpg_echo: false,
        }
    }
}

impl ShoptOptions {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get option by name
    pub fn get(&self, name: &str) -> Option<bool> {
        match name {
            "extglob" => Some(self.extglob),
            "globstar" => Some(self.globstar),
            "nullglob" => Some(self.nullglob),
            "dotglob" => Some(self.dotglob),
            "failglob" => Some(self.failglob),
            "nocaseglob" => Some(self.nocaseglob),
            "nocasematch" => Some(self.nocasematch),
            "lastpipe" => Some(self.lastpipe),
            "expand_aliases" => Some(self.expand_aliases),
            "sourcepath" => Some(self.sourcepath),
            "checkwinsize" => Some(self.checkwinsize),
            "histexpand" => Some(self.histexpand),
            "progcomp" => Some(self.progcomp),
            "cdspell" => Some(self.cdspell),
            "checkjobs" => Some(self.checkjobs),
            "cmdhist" => Some(self.cmdhist),
            "dirspell" => Some(self.dirspell),
            "histreedit" => Some(self.histreedit),
            "hostcomplete" => Some(self.hostcomplete),
            "lithist" => Some(self.lithist),
            "no_empty_cmd_completion" => Some(self.no_empty_cmd_completion),
            "promptvars" => Some(self.promptvars),
            "shift_verbose" => Some(self.shift_verbose),
            "xpg_echo" => Some(self.xpg_echo),
            _ => None,
        }
    }
    
    /// Set option by name. Returns true if option exists.
    pub fn set(&mut self, name: &str, value: bool) -> bool {
        match name {
            "extglob" => { self.extglob = value; true }
            "globstar" => { self.globstar = value; true }
            "nullglob" => { self.nullglob = value; true }
            "dotglob" => { self.dotglob = value; true }
            "failglob" => { self.failglob = value; true }
            "nocaseglob" => { self.nocaseglob = value; true }
            "nocasematch" => { self.nocasematch = value; true }
            "lastpipe" => { self.lastpipe = value; true }
            "expand_aliases" => { self.expand_aliases = value; true }
            "sourcepath" => { self.sourcepath = value; true }
            "checkwinsize" => { self.checkwinsize = value; true }
            "histexpand" => { self.histexpand = value; true }
            "progcomp" => { self.progcomp = value; true }
            "cdspell" => { self.cdspell = value; true }
            "checkjobs" => { self.checkjobs = value; true }
            "cmdhist" => { self.cmdhist = value; true }
            "dirspell" => { self.dirspell = value; true }
            "histreedit" => { self.histreedit = value; true }
            "hostcomplete" => { self.hostcomplete = value; true }
            "lithist" => { self.lithist = value; true }
            "no_empty_cmd_completion" => { self.no_empty_cmd_completion = value; true }
            "promptvars" => { self.promptvars = value; true }
            "shift_verbose" => { self.shift_verbose = value; true }
            "xpg_echo" => { self.xpg_echo = value; true }
            _ => false,
        }
    }
    
    /// Get all option names (sorted alphabetically)
    pub fn all_options() -> &'static [&'static str] {
        &[
            "cdspell",
            "checkjobs", 
            "checkwinsize",
            "cmdhist",
            "dirspell",
            "dotglob",
            "expand_aliases",
            "extglob",
            "failglob",
            "globstar",
            "histexpand",
            "histreedit",
            "hostcomplete",
            "lastpipe",
            "lithist",
            "no_empty_cmd_completion",
            "nocaseglob",
            "nocasematch",
            "nullglob",
            "progcomp",
            "promptvars",
            "shift_verbose",
            "sourcepath",
            "xpg_echo",
        ]
    }
}

/// Alias for backward compatibility
pub type ShopOptions = ShoptOptions;

/// Variable attributes (bash-compatible)
/// These flags control variable behavior on assignment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VarAttributes {
    /// Integer attribute (-i): arithmetic evaluation on assignment
    pub integer: bool,
    /// Lowercase attribute (-l): convert to lowercase on assignment
    pub lowercase: bool,
    /// Uppercase attribute (-u): convert to uppercase on assignment
    pub uppercase: bool,
    /// Readonly attribute (-r): cannot modify or unset
    pub readonly: bool,
    /// Export attribute (-x): export to environment
    pub export: bool,
    /// Trace attribute (-t): trace with DEBUG trap
    pub trace: bool,
}

impl VarAttributes {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Returns true if any attribute is set
    pub fn has_any(&self) -> bool {
        self.integer || self.lowercase || self.uppercase || 
        self.readonly || self.export || self.trace
    }
    
    /// Format attributes as declare flags (e.g., "-ir" for integer+readonly)
    pub fn to_flags(&self) -> String {
        let mut flags = String::new();
        if self.integer { flags.push('i'); }
        if self.lowercase { flags.push('l'); }
        if self.uppercase { flags.push('u'); }
        if self.readonly { flags.push('r'); }
        if self.export { flags.push('x'); }
        if self.trace { flags.push('t'); }
        if flags.is_empty() {
            String::new()
        } else {
            format!("-{}", flags)
        }
    }
    
    /// Parse attribute flags from declare-style string (e.g., "ilr" or "-ilr")
    pub fn from_flags(flags: &str) -> Self {
        let flags = flags.trim_start_matches('-');
        Self {
            integer: flags.contains('i'),
            lowercase: flags.contains('l'),
            uppercase: flags.contains('u'),
            readonly: flags.contains('r'),
            export: flags.contains('x'),
            trace: flags.contains('t'),
        }
    }
}

/// Shell variable value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Variable {
    String(String),
    Array(Vec<String>),
    AssocArray(HashMap<String, String>),
    /// Name reference - value is the name of the target variable
    NameRef(String),
}

impl Variable {
    pub fn as_str(&self) -> &str {
        match self {
            Variable::String(s) => s,
            Variable::Array(arr) => arr.first().map(|s| s.as_str()).unwrap_or(""),
            Variable::AssocArray(map) => map.values().next().map(|s| s.as_str()).unwrap_or(""),
            Variable::NameRef(target) => target,
        }
    }

    pub fn is_set(&self) -> bool {
        match self {
            Variable::String(s) => !s.is_empty(),
            Variable::Array(arr) => !arr.is_empty(),
            Variable::AssocArray(map) => !map.is_empty(),
            Variable::NameRef(target) => !target.is_empty(),
        }
    }
    
    pub fn is_nameref(&self) -> bool {
        matches!(self, Variable::NameRef(_))
    }
    
    pub fn nameref_target(&self) -> Option<&str> {
        match self {
            Variable::NameRef(target) => Some(target),
            _ => None,
        }
    }
}

/// Saved state for a local variable
#[derive(Debug, Clone)]
pub struct SavedVariable {
    /// The previous value (None if the variable was unset)
    pub previous: Option<Variable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrapSignal {
    Exit,
    Err,
    Debug,
    Return,
    Signal(i32),
}

impl TrapSignal {
    pub fn parse(s: &str) -> Option<Self> {
        let s_upper = s.to_uppercase();
        let s_upper: &str = s_upper.strip_prefix("SIG").unwrap_or(&s_upper);
        
        match s_upper {
            "EXIT" | "0" => Some(TrapSignal::Exit),
            "ERR" => Some(TrapSignal::Err),
            "DEBUG" => Some(TrapSignal::Debug),
            "RETURN" => Some(TrapSignal::Return),
            "HUP" | "1" => Some(TrapSignal::Signal(1)),
            "INT" | "2" => Some(TrapSignal::Signal(2)),
            "QUIT" | "3" => Some(TrapSignal::Signal(3)),
            "ILL" | "4" => Some(TrapSignal::Signal(4)),
            "TRAP" | "5" => Some(TrapSignal::Signal(5)),
            "ABRT" | "6" => Some(TrapSignal::Signal(6)),
            "BUS" | "7" => Some(TrapSignal::Signal(7)),
            "FPE" | "8" => Some(TrapSignal::Signal(8)),
            "KILL" | "9" => Some(TrapSignal::Signal(9)),
            "USR1" | "10" => Some(TrapSignal::Signal(10)),
            "SEGV" | "11" => Some(TrapSignal::Signal(11)),
            "USR2" | "12" => Some(TrapSignal::Signal(12)),
            "PIPE" | "13" => Some(TrapSignal::Signal(13)),
            "ALRM" | "14" => Some(TrapSignal::Signal(14)),
            "TERM" | "15" => Some(TrapSignal::Signal(15)),
            "STKFLT" | "16" => Some(TrapSignal::Signal(16)),
            "CHLD" | "17" => Some(TrapSignal::Signal(17)),
            "CONT" | "18" => Some(TrapSignal::Signal(18)),
            "STOP" | "19" => Some(TrapSignal::Signal(19)),
            "TSTP" | "20" => Some(TrapSignal::Signal(20)),
            "TTIN" | "21" => Some(TrapSignal::Signal(21)),
            "TTOU" | "22" => Some(TrapSignal::Signal(22)),
            _ => s.parse::<i32>().ok().and_then(|n| {
                if (1..=31).contains(&n) {
                    Some(TrapSignal::Signal(n))
                } else {
                    None
                }
            }),
        }
    }
    
    pub fn name(&self) -> &'static str {
        match self {
            TrapSignal::Exit => "EXIT",
            TrapSignal::Err => "ERR",
            TrapSignal::Debug => "DEBUG",
            TrapSignal::Return => "RETURN",
            TrapSignal::Signal(1) => "HUP",
            TrapSignal::Signal(2) => "INT",
            TrapSignal::Signal(3) => "QUIT",
            TrapSignal::Signal(4) => "ILL",
            TrapSignal::Signal(5) => "TRAP",
            TrapSignal::Signal(6) => "ABRT",
            TrapSignal::Signal(7) => "BUS",
            TrapSignal::Signal(8) => "FPE",
            TrapSignal::Signal(9) => "KILL",
            TrapSignal::Signal(10) => "USR1",
            TrapSignal::Signal(11) => "SEGV",
            TrapSignal::Signal(12) => "USR2",
            TrapSignal::Signal(13) => "PIPE",
            TrapSignal::Signal(14) => "ALRM",
            TrapSignal::Signal(15) => "TERM",
            TrapSignal::Signal(16) => "STKFLT",
            TrapSignal::Signal(17) => "CHLD",
            TrapSignal::Signal(18) => "CONT",
            TrapSignal::Signal(19) => "STOP",
            TrapSignal::Signal(20) => "TSTP",
            TrapSignal::Signal(21) => "TTIN",
            TrapSignal::Signal(22) => "TTOU",
            TrapSignal::Signal(_) => "UNKNOWN",
        }
    }
    
    pub fn can_trap(&self) -> bool {
        !matches!(self, TrapSignal::Signal(9) | TrapSignal::Signal(19))
    }
    
    pub fn all_signal_names() -> &'static [&'static str] {
        &[
            "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE",
            "KILL", "USR1", "SEGV", "USR2", "PIPE", "ALRM", "TERM",
            "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU",
        ]
    }
}

impl fmt::Display for TrapSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrapAction {
    Ignore,
    Default,
    Command(String),
}

/// Completion action types for built-in completion actions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionAction {
    /// -a: alias names
    Alias,
    /// -b: builtin command names
    Builtin,
    /// -c: command names (builtins + external + functions)
    Command,
    /// -d: directory names
    Directory,
    /// -e: exported variable names
    Export,
    /// -f: file names
    File,
    /// -g: group names
    Group,
    /// -j: job names
    Job,
    /// -k: reserved words
    Keyword,
    /// -s: service names
    Service,
    /// -u: user names
    User,
    /// -v: variable names
    Variable,
    /// -A function: function names
    Function,
    /// -A hostname: hostnames
    Hostname,
    /// -A helptopic: help topics
    HelpTopic,
    /// -A signal: signal names
    Signal,
    /// -A stopped: stopped job names
    Stopped,
    /// -A running: running job names
    Running,
    /// -A setopt: set -o option names
    SetOpt,
    /// -A shopt: shopt option names
    ShOpt,
    /// -A arrayvar: array variable names
    ArrayVar,
    /// -A binding: readline key binding names
    Binding,
}

impl CompletionAction {
    /// Parse completion action from -A argument
    pub fn from_action_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "alias" => Some(CompletionAction::Alias),
            "arrayvar" => Some(CompletionAction::ArrayVar),
            "binding" => Some(CompletionAction::Binding),
            "builtin" => Some(CompletionAction::Builtin),
            "command" => Some(CompletionAction::Command),
            "directory" => Some(CompletionAction::Directory),
            "disabled" => None, // Not implemented
            "enabled" => None,  // Not implemented
            "export" => Some(CompletionAction::Export),
            "file" => Some(CompletionAction::File),
            "function" => Some(CompletionAction::Function),
            "group" => Some(CompletionAction::Group),
            "helptopic" => Some(CompletionAction::HelpTopic),
            "hostname" => Some(CompletionAction::Hostname),
            "job" => Some(CompletionAction::Job),
            "keyword" => Some(CompletionAction::Keyword),
            "running" => Some(CompletionAction::Running),
            "service" => Some(CompletionAction::Service),
            "setopt" => Some(CompletionAction::SetOpt),
            "shopt" => Some(CompletionAction::ShOpt),
            "signal" => Some(CompletionAction::Signal),
            "stopped" => Some(CompletionAction::Stopped),
            "user" => Some(CompletionAction::User),
            "variable" => Some(CompletionAction::Variable),
            _ => None,
        }
    }
}

/// Completion specification for a command
#[derive(Debug, Clone, Default)]
pub struct CompletionSpec {
    /// -F: Function to call for generating completions
    pub function: Option<String>,
    /// -W: Word list to complete from
    pub wordlist: Option<String>,
    /// -G: Glob pattern for filename expansion
    pub globpat: Option<String>,
    /// -C: Command to run for generating completions (stdout is word list)
    pub command: Option<String>,
    /// -P: Prefix to prepend to each completion
    pub prefix: Option<String>,
    /// -S: Suffix to append to each completion
    pub suffix: Option<String>,
    /// -X: Filter pattern - remove completions matching this pattern
    pub filter_pattern: Option<String>,
    /// -o: Completion options (nospace, default, dirnames, filenames, etc.)
    pub options: Vec<String>,
    /// Built-in completion actions (-a, -b, -c, -d, -e, -f, etc.)
    pub actions: Vec<CompletionAction>,
}

impl CompletionSpec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this spec is empty (no completion sources defined)
    pub fn is_empty(&self) -> bool {
        self.function.is_none()
            && self.wordlist.is_none()
            && self.globpat.is_none()
            && self.command.is_none()
            && self.actions.is_empty()
    }
}

pub struct ExpansionContext {
    variables: HashMap<String, Variable>,
    var_attributes: HashMap<String, VarAttributes>,
    environment: HashMap<String, String>,
    aliases: HashMap<String, String>,
    functions: HashMap<String, colony_parser::FunctionDef>,
    /// Set of function names that are marked for export to child processes
    exported_functions: std::collections::HashSet<String>,
    completions: HashMap<String, CompletionSpec>,
    positional_params: Vec<String>,
    shell_name: String,
    cwd: PathBuf,
    ifs: String,
    home: Option<PathBuf>,
    last_exit_status: i32,
    function_depth: usize,
    local_vars_stack: Vec<HashMap<String, SavedVariable>>,
    local_attrs_stack: Vec<HashMap<String, Option<VarAttributes>>>,
    shell_options: ShellOptions,
    shopt_options: ShoptOptions,
    traps: HashMap<TrapSignal, TrapAction>,
    dir_stack: Vec<PathBuf>,
    system_vars: SystemVars,
    shell_start_time: Instant,
    seconds_offset: Cell<i64>,
    rng: Cell<Option<StdRng>>,
    current_lineno: Cell<u32>,
    shell_pid: u32,
    last_bg_pid: Cell<Option<u32>>,
    interactive: bool,
    #[cfg(feature = "secure")]
    current_script_path: Option<PathBuf>,
}

impl fmt::Debug for ExpansionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ExpansionContext");
        s.field("variables", &self.variables)
            .field("environment", &format!("<{} vars>", self.environment.len()))
            .field("aliases", &self.aliases)
            .field("functions", &format!("<{} funcs>", self.functions.len()))
            .field("positional_params", &self.positional_params)
            .field("shell_name", &self.shell_name)
            .field("cwd", &self.cwd)
            .field("ifs", &self.ifs)
            .field("home", &self.home)
            .field("last_exit_status", &self.last_exit_status)
            .field("function_depth", &self.function_depth)
            .field("shell_options", &self.shell_options)
            .field("dir_stack", &self.dir_stack)
            .field("system_vars", &self.system_vars);
        #[cfg(feature = "secure")]
        s.field("current_script_path", &self.current_script_path);
        s.finish()
    }
}

impl Clone for ExpansionContext {
    fn clone(&self) -> Self {
        let cloned_rng = if let Some(mut rng) = self.rng.take() {
            let seed: u64 = rng.gen();
            self.rng.set(Some(rng));
            Some(StdRng::seed_from_u64(seed))
        } else {
            None
        };
        
        Self {
            variables: self.variables.clone(),
            var_attributes: self.var_attributes.clone(),
            environment: self.environment.clone(),
            aliases: self.aliases.clone(),
            functions: self.functions.clone(),
            exported_functions: self.exported_functions.clone(),
            completions: self.completions.clone(),
            positional_params: self.positional_params.clone(),
            shell_name: self.shell_name.clone(),
            cwd: self.cwd.clone(),
            ifs: self.ifs.clone(),
            home: self.home.clone(),
            last_exit_status: self.last_exit_status,
            function_depth: self.function_depth,
            local_vars_stack: self.local_vars_stack.clone(),
            local_attrs_stack: self.local_attrs_stack.clone(),
            shell_options: self.shell_options.clone(),
            shopt_options: self.shopt_options.clone(),
            traps: self.traps.clone(),
            dir_stack: self.dir_stack.clone(),
            system_vars: self.system_vars.clone(),
            shell_start_time: self.shell_start_time,
            seconds_offset: Cell::new(self.seconds_offset.get()),
            rng: Cell::new(cloned_rng),
            current_lineno: Cell::new(self.current_lineno.get()),
            shell_pid: self.shell_pid,
            last_bg_pid: Cell::new(self.last_bg_pid.get()),
            interactive: self.interactive,
            #[cfg(feature = "secure")]
            current_script_path: self.current_script_path.clone(),
        }
    }
}

impl ExpansionContext {
    pub fn new() -> Self {
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let system_vars = SystemVars::new();
        let machtype_for_versinfo = system_vars.machtype.clone();

        let mut ctx = Self {
            variables: HashMap::new(),
            var_attributes: HashMap::new(),
            environment: std::env::vars().collect(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            exported_functions: std::collections::HashSet::new(),
            completions: HashMap::new(),
            positional_params: Vec::new(),
            shell_name: "colony".to_string(),
            last_exit_status: 0,
            cwd: cwd.clone(),
            ifs: " \t\n".to_string(),
            home,
            function_depth: 0,
            local_vars_stack: Vec::new(),
            local_attrs_stack: Vec::new(),
            shell_options: ShellOptions::new(),
            shopt_options: ShoptOptions::new(),
            traps: HashMap::new(),
            dir_stack: vec![cwd],
            system_vars,
            shell_start_time: Instant::now(),
            seconds_offset: Cell::new(0),
            rng: Cell::new(None),
            current_lineno: Cell::new(1),
            shell_pid: std::process::id(),
            last_bg_pid: Cell::new(None),
            interactive: false,
            #[cfg(feature = "secure")]
            current_script_path: None,
        };

        ctx.variables.insert("BASH_VERSINFO".to_string(), Variable::Array(vec![
            "5".to_string(),
            "2".to_string(),
            "0".to_string(),
            "1".to_string(),
            "release".to_string(),
            machtype_for_versinfo,
        ]));

        ctx.variables.insert("OPTIND".to_string(), Variable::String("1".to_string()));

        ctx
    }

    /// Get a shell variable
    pub fn get_variable(&self, name: &str) -> Option<&Variable> {
        // Check shell variables first
        if let Some(var) = self.variables.get(name) {
            return Some(var);
        }

        // Fall back to environment variables (handled by get_var_or_env instead)
        None
    }
    
    /// Resolve nameref chain and return the final target variable name
    /// Returns None if there's a circular reference (cycle detection)
    /// Max depth of 10 to prevent infinite loops from circular refs
    pub fn resolve_nameref(&self, name: &str) -> Option<String> {
        let mut current = name.to_string();
        let mut depth = 0;
        const MAX_DEPTH: usize = 10;
        
        while depth < MAX_DEPTH {
            if let Some(var) = self.variables.get(&current) {
                if let Variable::NameRef(target) = var {
                    if target == name || target == &current {
                        return None;
                    }
                    current = target.clone();
                    depth += 1;
                } else {
                    return Some(current);
                }
            } else {
                return Some(current);
            }
        }
        None
    }
    
    /// Get variable resolving namerefs to their targets
    pub fn get_variable_resolved(&self, name: &str) -> Option<&Variable> {
        if let Some(var) = self.variables.get(name) {
            if let Variable::NameRef(target) = var {
                if let Some(resolved) = self.resolve_nameref(name) {
                    return self.variables.get(&resolved);
                }
                return None;
            }
            return Some(var);
        }
        None
    }

    /// Get variable or environment variable as string
    pub fn get_var_or_env(&self, name: &str) -> Option<String> {
        let value = self.get_var_or_env_raw(name);
        
        #[cfg(feature = "secure")]
        {
            value.map(maybe_decrypt_value)
        }
        
        #[cfg(not(feature = "secure"))]
        {
            value
        }
    }
    
    fn get_var_or_env_raw(&self, name: &str) -> Option<String> {
        match name {
            "0" => return Some(self.shell_name.clone()),
            "HOME" => return self.home.as_ref().map(|p| p.display().to_string()),
            "PWD" => return Some(self.cwd.display().to_string()),
            "IFS" => return Some(self.ifs.clone()),
            "#" => return Some(self.positional_params.len().to_string()),
            "@" | "*" => return Some(self.positional_params.join(" ")),
            "?" => return Some(self.last_exit_status.to_string()),
            "$" => return Some(self.shell_pid.to_string()),
            "!" => return self.last_bg_pid.get().map(|p| p.to_string()),
            "-" => return Some(self.shell_options.to_string()),
            "BASH_VERSION" => return Some("5.2.0(1)-colony".to_string()),
            "BASH_VERSINFO" => return Some("5".to_string()),
            "UID" => return Some(self.system_vars.uid.to_string()),
            "EUID" => return Some(self.system_vars.euid.to_string()),
            "PPID" => return Some(self.system_vars.ppid.to_string()),
            "HOSTNAME" => return Some(self.system_vars.hostname.clone()),
            "HOSTTYPE" => return Some(self.system_vars.hosttype.clone()),
            "OSTYPE" => return Some(self.system_vars.ostype.clone()),
            "MACHTYPE" => return Some(self.system_vars.machtype.clone()),
            "SHELL" => {
                if let Some(shell) = self.environment.get("SHELL") {
                    return Some(shell.clone());
                }
                return Some(self.system_vars.shell_path.clone());
            }
            "USER" => {
                if let Some(user) = self.environment.get("USER") {
                    return Some(user.clone());
                }
                return self.get_username_from_uid();
            }
            "BASHPID" => return Some(nix::unistd::getpid().as_raw().to_string()),
            "RANDOM" => return Some(self.get_random().to_string()),
            "SRANDOM" => return Some(self.get_srandom().to_string()),
            "SECONDS" => return Some(self.get_seconds().to_string()),
            "EPOCHSECONDS" => return Some(self.get_epochseconds().to_string()),
            "EPOCHREALTIME" => return Some(self.get_epochrealtime()),
            "LINENO" => return Some(self.current_lineno.get().to_string()),
            _ => {}
        }

        if let Ok(index) = name.parse::<usize>() {
            return self.get_positional_param(index).map(|s| s.to_string());
        }

        if let Some(var) = self.variables.get(name) {
            if let Variable::NameRef(target) = var {
                if let Some(resolved) = self.resolve_nameref(name) {
                    if let Some(target_var) = self.variables.get(&resolved) {
                        return Some(target_var.as_str().to_string());
                    }
                    return self.environment.get(&resolved).cloned();
                }
                return None;
            }
            return Some(var.as_str().to_string());
        }

        self.environment.get(name).cloned()
    }

    fn get_username_from_uid(&self) -> Option<String> {
        if let Ok(user) = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(self.system_vars.uid)) {
            return user.map(|u| u.name);
        }
        None
    }

    fn get_random(&self) -> u16 {
        let mut rng = self.rng.take().unwrap_or_else(|| StdRng::from_entropy());
        let value: u16 = rng.gen_range(0..32768);
        self.rng.set(Some(rng));
        value
    }

    fn get_srandom(&self) -> u32 {
        let mut buf = [0u8; 4];
        getrandom::getrandom(&mut buf).unwrap_or_else(|_| {
            let mut rng = self.rng.take().unwrap_or_else(|| StdRng::from_entropy());
            buf = rng.gen::<[u8; 4]>();
            self.rng.set(Some(rng));
        });
        u32::from_ne_bytes(buf)
    }

    fn get_seconds(&self) -> i64 {
        let elapsed = self.shell_start_time.elapsed().as_secs() as i64;
        elapsed + self.seconds_offset.get()
    }

    fn get_epochseconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn get_epochrealtime(&self) -> String {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}.{:06}", duration.as_secs(), duration.subsec_micros())
    }

    pub fn set_lineno(&self, lineno: u32) {
        self.current_lineno.set(lineno);
    }

    pub fn lineno(&self) -> u32 {
        self.current_lineno.get()
    }

    pub fn seed_random(&self, seed: u64) {
        self.rng.set(Some(StdRng::seed_from_u64(seed)));
    }

    pub fn set_seconds(&self, value: i64) {
        let elapsed = self.shell_start_time.elapsed().as_secs() as i64;
        self.seconds_offset.set(value - elapsed);
    }

    /// Set a shell variable
    pub fn set_variable(&mut self, name: String, value: Variable) {
        if let Some(existing) = self.variables.get(&name) {
            if let Variable::NameRef(target) = existing {
                if let Some(resolved) = self.resolve_nameref(&name) {
                    self.variables.insert(resolved, value);
                    return;
                }
            }
        }
        self.variables.insert(name, value);
    }

    /// Set a shell variable from string
    /// Returns false if the variable is readonly
    pub fn set_var(&mut self, name: String, value: String) -> bool {
        if self.is_readonly(&name) {
            return false;
        }
        
        match name.as_str() {
            "IFS" => self.ifs = value.clone(),
            "HOME" => self.home = Some(PathBuf::from(&value)),
            "RANDOM" => {
                if let Ok(seed) = value.parse::<u64>() {
                    self.seed_random(seed);
                }
                return true;
            }
            "SECONDS" => {
                if let Ok(secs) = value.parse::<i64>() {
                    self.set_seconds(secs);
                }
                return true;
            }
            _ => {}
        }
        
        let transformed = self.apply_attribute_transforms(&name, value.clone());
        
        if let Some(existing) = self.variables.get(&name) {
            if let Variable::NameRef(_) = existing {
                if let Some(resolved) = self.resolve_nameref(&name) {
                    if self.is_readonly(&resolved) {
                        return false;
                    }
                    match resolved.as_str() {
                        "IFS" => self.ifs = transformed.clone(),
                        "HOME" => self.home = Some(PathBuf::from(&transformed)),
                        _ => {}
                    }
                    let final_value = self.apply_attribute_transforms(&resolved, transformed);
                    self.variables.insert(resolved, Variable::String(final_value));
                    return true;
                }
            }
        }
        
        if self.is_exported(&name) {
            self.environment.insert(name.clone(), transformed.clone());
        }
        
        self.variables.insert(name, Variable::String(transformed));
        true
    }
    
    /// Set a nameref variable
    pub fn set_nameref(&mut self, name: String, target: String) {
        self.variables.insert(name, Variable::NameRef(target));
    }
    
    pub fn set_array_element(&mut self, name: &str, index: usize, value: &str) {
        match self.variables.get_mut(name) {
            Some(Variable::Array(arr)) => {
                if index >= arr.len() {
                    arr.resize(index + 1, String::new());
                }
                arr[index] = value.to_string();
            }
            Some(Variable::AssocArray(map)) => {
                map.insert(index.to_string(), value.to_string());
            }
            _ => {
                let mut arr = vec![String::new(); index + 1];
                arr[index] = value.to_string();
                self.variables.insert(name.to_string(), Variable::Array(arr));
            }
        }
    }
    
    /// Unset a nameref itself (not its target)
    pub fn unset_nameref(&mut self, name: &str) {
        if let Some(var) = self.variables.get(name) {
            if let Variable::NameRef(_) = var {
                self.variables.remove(name);
            }
        }
    }

    /// Unset a variable (follows nameref to target)
    pub fn unset_variable(&mut self, name: &str) {
        if let Some(var) = self.variables.get(name) {
            if let Variable::NameRef(_) = var {
                if let Some(resolved) = self.resolve_nameref(name) {
                    self.variables.remove(&resolved);
                    return;
                }
            }
        }
        self.variables.remove(name);
    }

    /// Get environment variable
    pub fn get_env(&self, name: &str) -> Option<&str> {
        self.environment.get(name).map(|s| s.as_str())
    }

    /// Set environment variable
    pub fn set_env(&mut self, name: &str, value: &str) {
        match name {
            "HOME" => self.home = Some(PathBuf::from(value)),
            "PWD" => self.cwd = PathBuf::from(value),
            _ => {}
        }
        self.environment.insert(name.to_string(), value.to_string());
    }

    /// Remove environment variable
    pub fn unset_env(&mut self, name: &str) {
        self.environment.remove(name);
    }

    /// Get current working directory
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Set current working directory
    pub fn set_cwd(&mut self, path: PathBuf) {
        self.cwd = path;
    }

    /// Get home directory
    pub fn home(&self) -> Option<PathBuf> {
        if let Some(var) = self.variables.get("HOME") {
            return Some(PathBuf::from(var.as_str()));
        }
        self.home.clone()
    }

    /// Get IFS (field separator) characters
    pub fn ifs(&self) -> &str {
        &self.ifs
    }

    /// Set IFS
    pub fn set_ifs(&mut self, ifs: String) {
        self.ifs = ifs;
    }

    /// Get an alias
    pub fn get_alias(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(|s| s.as_str())
    }

    /// Set an alias
    pub fn set_alias(&mut self, name: String, value: String) {
        self.aliases.insert(name, value);
    }

    /// Remove an alias
    pub fn unset_alias(&mut self, name: &str) {
        self.aliases.remove(name);
    }

    /// Remove all aliases
    pub fn clear_aliases(&mut self) {
        self.aliases.clear();
    }

    /// Get all aliases (for listing)
    pub fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    /// Define a function
    pub fn define_function(&mut self, name: String, def: colony_parser::FunctionDef) {
        self.functions.insert(name, def);
    }

    /// Get a function definition
    pub fn get_function(&self, name: &str) -> Option<&colony_parser::FunctionDef> {
        self.functions.get(name)
    }

    /// Remove a function
    pub fn unset_function(&mut self, name: &str) {
        self.functions.remove(name);
        self.exported_functions.remove(name);
    }

    /// Check if a name is a defined function
    pub fn is_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
    
    /// Get all defined functions
    pub fn functions(&self) -> &HashMap<String, colony_parser::FunctionDef> {
        &self.functions
    }
    
    /// Mark a function for export to child processes
    pub fn export_function(&mut self, name: &str) -> bool {
        if self.functions.contains_key(name) {
            self.exported_functions.insert(name.to_string());
            true
        } else {
            false
        }
    }
    
    /// Remove export flag from a function
    pub fn unexport_function(&mut self, name: &str) {
        self.exported_functions.remove(name);
    }
    
    /// Check if a function is marked for export
    pub fn is_function_exported(&self, name: &str) -> bool {
        self.exported_functions.contains(name)
    }
    
    /// Get all exported function names
    pub fn exported_function_names(&self) -> impl Iterator<Item = &String> {
        self.exported_functions.iter()
    }

    /// Set positional parameters ($1, $2, ...)
    pub fn set_positional_params(&mut self, params: Vec<String>) {
        self.positional_params = params;
    }

    /// Set shell name ($0)
    pub fn set_shell_name(&mut self, name: String) {
        self.shell_name = name;
    }

    /// Get positional parameter by index (1-based: $1 is index 0)
    pub fn get_positional_param(&self, index: usize) -> Option<&str> {
        if index == 0 {
            Some(&self.shell_name)
        } else {
            self.positional_params.get(index - 1).map(|s| s.as_str())
        }
    }

    /// Get all positional parameters
    pub fn get_all_positional_params(&self) -> &[String] {
        &self.positional_params
    }

    /// Get positional parameter count
    pub fn positional_param_count(&self) -> usize {
        self.positional_params.len()
    }

    pub fn set_last_exit_status(&mut self, status: i32) {
        self.last_exit_status = status;
    }

    pub fn last_exit_status(&self) -> i32 {
        self.last_exit_status
    }

    pub fn shell_pid(&self) -> u32 {
        self.shell_pid
    }

    pub fn last_bg_pid(&self) -> Option<u32> {
        self.last_bg_pid.get()
    }

    pub fn set_last_bg_pid(&self, pid: u32) {
        self.last_bg_pid.set(Some(pid));
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    pub fn set_interactive(&mut self, interactive: bool) {
        self.interactive = interactive;
    }

    pub fn shell_options(&self) -> &ShellOptions {
        &self.shell_options
    }

    pub fn shell_options_mut(&mut self) -> &mut ShellOptions {
        &mut self.shell_options
    }

    pub fn shopt_options(&self) -> &ShoptOptions {
        &self.shopt_options
    }

    pub fn shopt_options_mut(&mut self) -> &mut ShoptOptions {
        &mut self.shopt_options
    }

    pub fn in_function(&self) -> bool {
        self.function_depth > 0
    }

    pub fn enter_function(&mut self) {
        self.function_depth += 1;
        self.local_vars_stack.push(HashMap::new());
    }

    pub fn exit_function(&mut self) {
        if self.function_depth > 0 {
            if let Some(local_vars) = self.local_vars_stack.pop() {
                for (name, saved) in local_vars {
                    match saved.previous {
                        Some(var) => {
                            self.variables.insert(name, var);
                        }
                        None => {
                            self.variables.remove(&name);
                        }
                    }
                }
            }
            self.function_depth -= 1;
        }
    }

    pub fn declare_local(&mut self, name: &str, value: Option<Variable>) {
        if self.function_depth == 0 {
            return;
        }

        if let Some(locals) = self.local_vars_stack.last_mut() {
            if !locals.contains_key(name) {
                let previous = self.variables.get(name).cloned();
                locals.insert(name.to_string(), SavedVariable { previous });
            }
        }

        match value {
            Some(var) => {
                self.variables.insert(name.to_string(), var);
            }
            None => {
                self.variables.insert(name.to_string(), Variable::String(String::new()));
            }
        }
    }

    pub fn set_trap(&mut self, signal: TrapSignal, action: TrapAction) {
        match action {
            TrapAction::Default => {
                self.traps.remove(&signal);
            }
            _ => {
                self.traps.insert(signal, action);
            }
        }
    }

    pub fn get_trap(&self, signal: &TrapSignal) -> Option<&TrapAction> {
        self.traps.get(signal)
    }

    pub fn clear_trap(&mut self, signal: &TrapSignal) {
        self.traps.remove(signal);
    }

    pub fn traps(&self) -> &HashMap<TrapSignal, TrapAction> {
        &self.traps
    }

    pub fn clear_non_ignored_traps(&mut self) {
        self.traps.retain(|_, action| matches!(action, TrapAction::Ignore));
    }

    pub fn dir_stack(&self) -> &[PathBuf] {
        &self.dir_stack
    }

    pub fn dir_stack_mut(&mut self) -> &mut Vec<PathBuf> {
        &mut self.dir_stack
    }

    pub fn dir_stack_len(&self) -> usize {
        self.dir_stack.len()
    }

    pub fn sync_dir_stack_top(&mut self) {
        if self.dir_stack.is_empty() {
            self.dir_stack.push(self.cwd.clone());
        } else {
            self.dir_stack[0] = self.cwd.clone();
        }
    }

    #[cfg(feature = "secure")]
    pub fn set_current_script_path(&mut self, path: Option<PathBuf>) {
        self.current_script_path = path;
    }

    #[cfg(feature = "secure")]
    pub fn current_script_path(&self) -> Option<&Path> {
        self.current_script_path.as_deref()
    }

    pub fn set_completion(&mut self, command: String, spec: CompletionSpec) {
        self.completions.insert(command, spec);
    }

    pub fn get_completion(&self, command: &str) -> Option<&CompletionSpec> {
        self.completions.get(command)
    }

    pub fn remove_completion(&mut self, command: &str) -> bool {
        self.completions.remove(command).is_some()
    }

    pub fn completions(&self) -> &HashMap<String, CompletionSpec> {
        &self.completions
    }

    pub fn function_names(&self) -> impl Iterator<Item = &String> {
        self.functions.keys()
    }

    pub fn variable_names(&self) -> impl Iterator<Item = &String> {
        self.variables.keys()
    }

    pub fn exported_variable_names(&self) -> impl Iterator<Item = &String> {
        self.environment.keys()
    }

    pub fn get_var_attributes(&self, name: &str) -> Option<&VarAttributes> {
        self.var_attributes.get(name)
    }

    pub fn set_var_attributes(&mut self, name: &str, attrs: VarAttributes) {
        if attrs.has_any() {
            self.var_attributes.insert(name.to_string(), attrs);
        } else {
            self.var_attributes.remove(name);
        }
    }

    pub fn is_readonly(&self, name: &str) -> bool {
        self.var_attributes
            .get(name)
            .map(|a| a.readonly)
            .unwrap_or(false)
    }

    pub fn is_integer(&self, name: &str) -> bool {
        self.var_attributes
            .get(name)
            .map(|a| a.integer)
            .unwrap_or(false)
    }

    pub fn is_exported(&self, name: &str) -> bool {
        self.var_attributes
            .get(name)
            .map(|a| a.export)
            .unwrap_or(false)
            || self.environment.contains_key(name)
    }

    pub fn apply_attribute_transforms(&self, name: &str, value: String) -> String {
        if let Some(attrs) = self.var_attributes.get(name) {
            if attrs.integer {
                return self.evaluate_integer_assignment(&value);
            }
            if attrs.lowercase {
                return value.to_lowercase();
            }
            if attrs.uppercase {
                return value.to_uppercase();
            }
        }
        value
    }

    fn evaluate_integer_assignment(&self, value: &str) -> String {
        let expr = value.trim();
        if expr.is_empty() {
            return "0".to_string();
        }
        
        let resolved = self.resolve_arithmetic_variables(expr);
        
        match crate::expand_arithmetic_immut(&resolved, self) {
            Ok(result) => result,
            Err(_) => "0".to_string(),
        }
    }

    fn resolve_arithmetic_variables(&self, expr: &str) -> String {
        let mut result = String::new();
        let mut chars = expr.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch.is_ascii_alphabetic() || ch == '_' {
                let mut name = String::new();
                name.push(ch);
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        name.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if let Some(var_value) = self.get_var_or_env(&name) {
                    if let Ok(n) = var_value.parse::<i64>() {
                        result.push_str(&n.to_string());
                    } else {
                        result.push('0');
                    }
                } else {
                    result.push('0');
                }
            } else {
                result.push(ch);
            }
        }
        
        result
    }

    pub fn set_var_with_attrs(
        &mut self, 
        name: String, 
        value: String, 
        attrs: Option<VarAttributes>
    ) -> Result<(), &'static str> {
        if self.is_readonly(&name) {
            return Err("readonly variable");
        }

        if let Some(a) = attrs {
            self.set_var_attributes(&name, a);
        }

        let transformed = self.apply_attribute_transforms(&name, value);

        if self.is_exported(&name) {
            self.environment.insert(name.clone(), transformed.clone());
        }

        self.set_var(name, transformed);
        Ok(())
    }

    pub fn declare_with_attrs(
        &mut self,
        name: &str,
        value: Option<String>,
        attrs: VarAttributes,
    ) -> Result<(), &'static str> {
        let existing_attrs = self.var_attributes.get(name).cloned();
        
        if let Some(existing) = &existing_attrs {
            if existing.readonly && !attrs.readonly {
                return Err("readonly variable");
            }
        }

        let mut final_attrs = existing_attrs.unwrap_or_default();
        
        if attrs.integer { final_attrs.integer = true; }
        if attrs.lowercase { final_attrs.lowercase = true; }
        if attrs.uppercase { final_attrs.uppercase = true; }
        if attrs.readonly { final_attrs.readonly = true; }
        if attrs.export { final_attrs.export = true; }
        if attrs.trace { final_attrs.trace = true; }

        self.set_var_attributes(name, final_attrs);

        if let Some(val) = value {
            let transformed = self.apply_attribute_transforms(name, val);
            
            if self.is_exported(name) || final_attrs.export {
                self.environment.insert(name.to_string(), transformed.clone());
            }
            
            self.variables.insert(name.to_string(), Variable::String(transformed));
        } else if !self.variables.contains_key(name) {
            self.variables.insert(name.to_string(), Variable::String(String::new()));
        }

        Ok(())
    }

    pub fn remove_attributes(&mut self, name: &str, remove: VarAttributes) -> Result<(), &'static str> {
        if let Some(existing) = self.var_attributes.get(name) {
            if existing.readonly && remove.readonly {
                return Err("readonly variable");
            }
        }

        if let Some(attrs) = self.var_attributes.get_mut(name) {
            if remove.integer { attrs.integer = false; }
            if remove.lowercase { attrs.lowercase = false; }
            if remove.uppercase { attrs.uppercase = false; }
            if remove.export { 
                attrs.export = false; 
                self.environment.remove(name);
            }
            if remove.trace { attrs.trace = false; }
            
            if !attrs.has_any() {
                self.var_attributes.remove(name);
            }
        }

        Ok(())
    }

    pub fn unset_variable_checked(&mut self, name: &str) -> Result<(), &'static str> {
        if self.is_readonly(name) {
            return Err("readonly variable");
        }
        self.unset_variable(name);
        self.var_attributes.remove(name);
        Ok(())
    }

    pub fn variables_with_attrs(&self) -> impl Iterator<Item = (&String, &Variable, Option<&VarAttributes>)> {
        self.variables.iter().map(|(name, var)| {
            (name, var, self.var_attributes.get(name))
        })
    }
    
    pub fn sync_variables_from(&mut self, other: &ExpansionContext) {
        for (name, value) in &other.variables {
            if !self.is_readonly(name) {
                self.variables.insert(name.clone(), value.clone());
            }
        }
        
        for (name, attrs) in &other.var_attributes {
            self.var_attributes.insert(name.clone(), *attrs);
        }
    }

    pub fn enter_function_with_attrs(&mut self) {
        self.function_depth += 1;
        self.local_vars_stack.push(HashMap::new());
        self.local_attrs_stack.push(HashMap::new());
    }

    pub fn exit_function_with_attrs(&mut self) {
        if self.function_depth > 0 {
            if let Some(local_vars) = self.local_vars_stack.pop() {
                for (name, saved) in local_vars {
                    match saved.previous {
                        Some(var) => {
                            self.variables.insert(name, var);
                        }
                        None => {
                            self.variables.remove(&name);
                        }
                    }
                }
            }
            
            if let Some(local_attrs) = self.local_attrs_stack.pop() {
                for (name, saved_attrs) in local_attrs {
                    match saved_attrs {
                        Some(attrs) => {
                            self.var_attributes.insert(name, attrs);
                        }
                        None => {
                            self.var_attributes.remove(&name);
                        }
                    }
                }
            }
            
            self.function_depth -= 1;
        }
    }

    pub fn declare_local_with_attrs(
        &mut self,
        name: &str,
        value: Option<Variable>,
        attrs: Option<VarAttributes>,
    ) {
        if self.function_depth == 0 {
            return;
        }

        if let Some(locals) = self.local_vars_stack.last_mut() {
            if !locals.contains_key(name) {
                let previous = self.variables.get(name).cloned();
                locals.insert(name.to_string(), SavedVariable { previous });
            }
        }
        
        if let Some(local_attrs) = self.local_attrs_stack.last_mut() {
            if !local_attrs.contains_key(name) {
                let previous_attrs = self.var_attributes.get(name).cloned();
                local_attrs.insert(name.to_string(), previous_attrs);
            }
        }

        if let Some(a) = attrs {
            self.set_var_attributes(name, a);
        }

        match value {
            Some(var) => {
                if let Variable::String(s) = var {
                    let transformed = self.apply_attribute_transforms(name, s);
                    self.variables.insert(name.to_string(), Variable::String(transformed));
                } else {
                    self.variables.insert(name.to_string(), var);
                }
            }
            None => {
                self.variables.insert(name.to_string(), Variable::String(String::new()));
            }
        }
    }
}

impl Default for ExpansionContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_attribute_arithmetic_evaluation() {
        let mut ctx = ExpansionContext::new();
        
        // Set integer attribute first
        let attrs = VarAttributes {
            integer: true,
            ..Default::default()
        };
        ctx.set_var_attributes("num", attrs);
        
        // Verify attribute is set
        assert!(ctx.is_integer("num"), "num should be an integer variable");
        assert!(ctx.get_var_attributes("num").is_some(), "num should have attributes");
        
        // Now set variable - should evaluate arithmetic
        ctx.set_var("num".to_string(), "5+3".to_string());
        
        // Should be evaluated to 8
        assert_eq!(ctx.get_var_or_env("num"), Some("8".to_string()), "5+3 should evaluate to 8 for integer variable");
    }

    #[test]
    fn test_lowercase_attribute() {
        let mut ctx = ExpansionContext::new();
        
        let attrs = VarAttributes {
            lowercase: true,
            ..Default::default()
        };
        ctx.set_var_attributes("str", attrs);
        ctx.set_var("str".to_string(), "HELLO".to_string());
        
        assert_eq!(ctx.get_var_or_env("str"), Some("hello".to_string()));
    }

    #[test]
    fn test_uppercase_attribute() {
        let mut ctx = ExpansionContext::new();
        
        let attrs = VarAttributes {
            uppercase: true,
            ..Default::default()
        };
        ctx.set_var_attributes("str", attrs);
        ctx.set_var("str".to_string(), "hello".to_string());
        
        assert_eq!(ctx.get_var_or_env("str"), Some("HELLO".to_string()));
    }

    #[test]
    fn test_readonly_attribute_blocks_set() {
        let mut ctx = ExpansionContext::new();
        
        ctx.set_var("const".to_string(), "original".to_string());
        
        let attrs = VarAttributes {
            readonly: true,
            ..Default::default()
        };
        ctx.set_var_attributes("const", attrs);
        
        // Attempt to modify should fail (return false)
        let result = ctx.set_var("const".to_string(), "modified".to_string());
        assert!(!result, "set_var should return false for readonly variable");
        
        // Value should remain unchanged
        assert_eq!(ctx.get_var_or_env("const"), Some("original".to_string()));
    }
}
