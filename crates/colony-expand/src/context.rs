//! Expansion context - provides shell state for expansions

use std::collections::HashMap;
use std::path::PathBuf;

/// Shell variable value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Variable {
    /// String value
    String(String),
    /// Array value (for future bash-isms, not POSIX)
    Array(Vec<String>),
}

impl Variable {
    pub fn as_str(&self) -> &str {
        match self {
            Variable::String(s) => s,
            Variable::Array(arr) => arr.first().map(|s| s.as_str()).unwrap_or(""),
        }
    }

    pub fn is_set(&self) -> bool {
        match self {
            Variable::String(s) => !s.is_empty(),
            Variable::Array(arr) => !arr.is_empty(),
        }
    }
}

/// Context for expansion operations
///
/// Provides access to shell state: variables, environment, current directory, aliases, functions
#[derive(Debug, Clone)]
pub struct ExpansionContext {
    /// Shell variables
    variables: HashMap<String, Variable>,
    /// Environment variables (separate from shell variables in POSIX)
    environment: HashMap<String, String>,
    /// Command aliases: alias name -> replacement
    aliases: HashMap<String, String>,
    /// Shell functions: function name -> definition
    functions: HashMap<String, colony_parser::FunctionDef>,
    /// Positional parameters: $1, $2, etc. (for function/script args)
    positional_params: Vec<String>,
    /// Current working directory
    cwd: PathBuf,
    /// Field separator (IFS variable controls field splitting)
    ifs: String,
    /// Home directory
    home: Option<PathBuf>,
}

impl ExpansionContext {
    pub fn new() -> Self {
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

        Self {
            variables: HashMap::new(),
            environment: std::env::vars().collect(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            positional_params: Vec::new(),
            cwd,
            ifs: " \t\n".to_string(), // POSIX default IFS
            home,
        }
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
        // Special variables
        match name {
            "HOME" => return self.home.as_ref().map(|p| p.display().to_string()),
            "PWD" => return Some(self.cwd.display().to_string()),
            "IFS" => return Some(self.ifs.clone()),
            // Positional parameter count
            "#" => return Some(self.positional_params.len().to_string()),
            // All positional parameters (space-separated)
            "@" | "*" => return Some(self.positional_params.join(" ")),
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

    /// Set a shell variable
    pub fn set_variable(&mut self, name: String, value: Variable) {
        self.variables.insert(name, value);
    }

    /// Set a shell variable from string
    pub fn set_var(&mut self, name: String, value: String) {
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
    pub fn home(&self) -> Option<&PathBuf> {
        self.home.as_ref()
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

    /// Get positional parameter by index (1-based: $1 is index 0)
    pub fn get_positional_param(&self, index: usize) -> Option<&str> {
        if index == 0 {
            // $0 is shell name, not a parameter
            None
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
}

impl Default for ExpansionContext {
    fn default() -> Self {
        Self::new()
    }
}
