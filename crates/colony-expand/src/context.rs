//! Expansion context - provides shell state for expansions

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

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
}

impl ShellOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Display for ShellOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut flags = String::new();
        if self.errexit { flags.push('e'); }
        if self.noglob { flags.push('f'); }
        if self.noexec { flags.push('n'); }
        if self.nounset { flags.push('u'); }
        if self.verbose { flags.push('v'); }
        if self.xtrace { flags.push('x'); }
        write!(f, "{}", flags)
    }
}

/// Shell variable value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Variable {
    String(String),
    Array(Vec<String>),
    AssocArray(HashMap<String, String>),
}

impl Variable {
    pub fn as_str(&self) -> &str {
        match self {
            Variable::String(s) => s,
            Variable::Array(arr) => arr.first().map(|s| s.as_str()).unwrap_or(""),
            Variable::AssocArray(map) => map.values().next().map(|s| s.as_str()).unwrap_or(""),
        }
    }

    pub fn is_set(&self) -> bool {
        match self {
            Variable::String(s) => !s.is_empty(),
            Variable::Array(arr) => !arr.is_empty(),
            Variable::AssocArray(map) => !map.is_empty(),
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

#[derive(Debug, Clone)]
pub struct ExpansionContext {
    variables: HashMap<String, Variable>,
    environment: HashMap<String, String>,
    aliases: HashMap<String, String>,
    functions: HashMap<String, colony_parser::FunctionDef>,
    positional_params: Vec<String>,
    shell_name: String,
    cwd: PathBuf,
    ifs: String,
    home: Option<PathBuf>,
    last_exit_status: i32,
    function_depth: usize,
    local_vars_stack: Vec<HashMap<String, SavedVariable>>,
    shell_options: ShellOptions,
    traps: HashMap<TrapSignal, TrapAction>,
    dir_stack: Vec<PathBuf>,
    system_vars: SystemVars,
}

impl ExpansionContext {
    pub fn new() -> Self {
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let system_vars = SystemVars::new();
        let machtype_for_versinfo = system_vars.machtype.clone();

        let mut ctx = Self {
            variables: HashMap::new(),
            environment: std::env::vars().collect(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            positional_params: Vec::new(),
            shell_name: "colony".to_string(),
            last_exit_status: 0,
            cwd: cwd.clone(),
            ifs: " \t\n".to_string(),
            home,
            function_depth: 0,
            local_vars_stack: Vec::new(),
            shell_options: ShellOptions::new(),
            traps: HashMap::new(),
            dir_stack: vec![cwd],
            system_vars,
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

    /// Get variable or environment variable as string
    pub fn get_var_or_env(&self, name: &str) -> Option<String> {
        match name {
            "0" => return Some(self.shell_name.clone()),
            "HOME" => return self.home.as_ref().map(|p| p.display().to_string()),
            "PWD" => return Some(self.cwd.display().to_string()),
            "IFS" => return Some(self.ifs.clone()),
            "#" => return Some(self.positional_params.len().to_string()),
            "@" | "*" => return Some(self.positional_params.join(" ")),
            "?" => return Some(self.last_exit_status.to_string()),
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
            _ => {}
        }

        // Check for numeric positional parameters: $1, $2, etc.
        if let Ok(index) = name.parse::<usize>() {
            return self.get_positional_param(index).map(|s| s.to_string());
        }

        // Check shell variables first
        if let Some(var) = self.variables.get(name) {
            return Some(var.as_str().to_string());
        }

        // Fall back to environment
        self.environment.get(name).cloned()
    }

    fn get_username_from_uid(&self) -> Option<String> {
        if let Ok(user) = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(self.system_vars.uid)) {
            return user.map(|u| u.name);
        }
        None
    }

    /// Set a shell variable
    pub fn set_variable(&mut self, name: String, value: Variable) {
        self.variables.insert(name, value);
    }

    /// Set a shell variable from string
    pub fn set_var(&mut self, name: String, value: String) {
        match name.as_str() {
            "IFS" => self.ifs = value.clone(),
            "HOME" => self.home = Some(PathBuf::from(&value)),
            _ => {}
        }
        self.variables.insert(name, Variable::String(value));
    }

    /// Unset a variable
    pub fn unset_variable(&mut self, name: &str) {
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
    }

    /// Check if a name is a defined function
    pub fn is_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
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

    pub fn shell_options(&self) -> &ShellOptions {
        &self.shell_options
    }

    pub fn shell_options_mut(&mut self) -> &mut ShellOptions {
        &mut self.shell_options
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
}

impl Default for ExpansionContext {
    fn default() -> Self {
        Self::new()
    }
}
