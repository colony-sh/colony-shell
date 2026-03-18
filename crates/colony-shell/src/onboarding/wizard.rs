//! First-run setup wizard for Colony Shell.
//!
//! A 5-step terminal-based wizard that runs before the REPL on first launch.
//! Uses simple `println!`/`stdin` prompts with optional crossterm colors.
//! Steps:
//! 1. Import existing shell config
//! 2. Choose keybinding style
//! 3. Configure LLM provider
//! 4. Claude Code plugin install (placeholder)
//! 5. Quick tour offer
//!
//! Writes `~/.colony/config.toml` with the user's preferences.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor, ResetColor};

use super::converter;
use super::importer;

/// Result of the wizard run.
#[derive(Debug)]
pub struct WizardResult {
    /// Whether the wizard completed successfully.
    pub completed: bool,
    /// The keybinding style chosen.
    pub keybindings: String,
    /// The LLM provider chosen (or "skip").
    pub provider: String,
    /// Whether the user wants the quick tour.
    pub wants_tour: bool,
    /// Whether shell config was imported.
    pub imported_config: bool,
    /// The shell that was imported from.
    pub imported_from: String,
}

/// Run the first-run setup wizard.
///
/// This is a blocking function that reads from stdin. It should be called
/// before the REPL loop starts, before any TUI initialization.
pub fn run() -> WizardResult {
    let version = env!("CARGO_PKG_VERSION");

    println!();
    print_colored("Welcome to Colony Shell v", Color::White);
    print_colored(version, Color::Cyan);
    println!();
    print_colored(
        "An AI-native shell for developers and agents.",
        Color::DarkGrey,
    );
    println!();
    println!();
    print_separator();
    println!();

    // Step 1: Import shell config
    let (imported_config, imported_from) = step_import_config();

    // Step 2: Keybinding style
    let keybindings = step_keybindings();

    // Step 3: LLM provider
    let provider = step_llm_provider();

    // Step 4: Claude Code plugin (placeholder)
    step_plugin_install();

    // Step 5: Quick tour
    let wants_tour = step_quick_tour();

    // Write config.toml
    let colony_dir = colony_dir();
    if let Err(e) = std::fs::create_dir_all(&colony_dir) {
        eprintln!("Warning: failed to create ~/.colony: {e}");
    }

    let config_content = build_config_toml(&keybindings, &provider, &imported_from);
    let config_path = colony_dir.join("config.toml");
    if let Err(e) = std::fs::write(&config_path, &config_content) {
        eprintln!("Warning: failed to write config.toml: {e}");
    }

    // Create empty subdirectories
    let _ = std::fs::create_dir_all(colony_dir.join("context"));
    let _ = std::fs::create_dir_all(colony_dir.join("recipes"));

    println!();
    print_separator();
    println!();
    print_colored("Colony Shell is ready.", Color::Green);
    println!(" Type # or ? to talk to the agent.");
    println!();

    WizardResult {
        completed: true,
        keybindings,
        provider,
        wants_tour,
        imported_config,
        imported_from,
    }
}

// ── Step 1: Import shell config ──────────────────────────────

fn step_import_config() -> (bool, String) {
    print_step(1, 5, "Import existing shell config?");
    println!();

    let configs = importer::detect_shell_configs();
    if configs.is_empty() {
        println!("  No existing shell configs found (.zshrc, .bashrc, etc.).");
        println!("  Starting with a clean Colony profile.");
        println!();
        return (false, String::new());
    }

    let shell = importer::detect_primary_shell();
    println!("  We found:");
    for config in &configs {
        let line_count = std::fs::read_to_string(config)
            .map(|c| c.lines().count())
            .unwrap_or(0);
        let name = config
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        println!("    ~/{name} ({line_count} lines)");
    }
    println!();
    println!("  Colony can import your aliases, environment variables,");
    println!("  functions, and PATH config so everything works immediately.");
    println!();

    let options = vec![
        format!("Import from {shell} [recommended]"),
        "Start fresh".to_string(),
        "Skip (decide later)".to_string(),
    ];
    let choice = prompt_choice(&options, 0);

    match choice {
        0 => {
            // Import
            let results: Vec<_> = configs.iter().map(|p| importer::parse_config(p)).collect();
            let summary = converter::summarize(&results);

            println!();
            print_colored("  Imported: ", Color::Green);
            println!("{summary}");

            let profile_content = converter::generate_profile(&results);
            let Some(profile_path) = converter::default_profile_path() else {
                eprintln!("  Warning: could not determine profile path");
                return (true, shell.to_string());
            };

            if let Err(e) = converter::write_profile(&profile_path, &profile_content) {
                eprintln!("  Warning: failed to write profile: {e}");
            } else {
                println!("  Written to: {}", profile_path.display());
            }

            println!();
            (true, shell.to_string())
        }
        1 => {
            println!();
            println!("  Starting with a clean Colony profile.");
            println!();
            (false, String::new())
        }
        _ => {
            println!();
            println!("  Skipped. Run `colony onboard` later to import.");
            println!();
            (false, String::new())
        }
    }
}

// ── Step 2: Keybinding style ─────────────────────────────────

fn step_keybindings() -> String {
    print_step(2, 5, "Choose your keybinding style");
    println!();

    let options = vec![
        "Vim (default)".to_string(),
        "Emacs".to_string(),
        "Hybrid (vim normal + emacs insert)".to_string(),
    ];
    let choice = prompt_choice(&options, 0);

    let style = match choice {
        0 => "vim",
        1 => "emacs",
        2 => "hybrid",
        _ => "vim",
    };

    println!();
    print_colored("  Keybindings: ", Color::Green);
    println!("{style}");
    println!();

    style.to_string()
}

// ── Step 3: LLM provider ────────────────────────────────────

fn step_llm_provider() -> String {
    print_step(3, 5, "LLM Provider");
    println!();
    println!("  Colony's agent needs an API key. Which provider?");
    println!();

    let options = vec![
        "Anthropic (Claude)".to_string(),
        "OpenAI (GPT-4)".to_string(),
        "Ollama (local models)".to_string(),
        "Skip (configure later)".to_string(),
    ];
    let choice = prompt_choice(&options, 0);

    let provider = match choice {
        0 => {
            let key = prompt_api_key("ANTHROPIC_API_KEY", "sk-ant-");
            if !key.is_empty() {
                std::env::set_var("ANTHROPIC_API_KEY", &key);
                println!();
                print_colored("  Provider set: ", Color::Green);
                println!("Anthropic (Claude)");
            }
            "anthropic"
        }
        1 => {
            let key = prompt_api_key("OPENAI_API_KEY", "sk-");
            if !key.is_empty() {
                std::env::set_var("OPENAI_API_KEY", &key);
                println!();
                print_colored("  Provider set: ", Color::Green);
                println!("OpenAI");
            }
            "openai"
        }
        2 => {
            println!();
            println!("  Ollama selected. Make sure `ollama serve` is running.");
            print_colored("  Provider set: ", Color::Green);
            println!("Ollama (local)");
            "ollama"
        }
        _ => {
            println!();
            println!("  Skipped. Set ANTHROPIC_API_KEY or OPENAI_API_KEY later.");
            "skip"
        }
    };

    println!();
    provider.to_string()
}

fn prompt_api_key(env_var: &str, expected_prefix: &str) -> String {
    // Check if already set in env
    if let Ok(existing) = std::env::var(env_var) {
        if !existing.is_empty() {
            let masked = mask_key(&existing);
            println!();
            print_colored("  Found existing key: ", Color::Green);
            println!("{masked}");
            return existing;
        }
    }

    println!();
    print!("  Enter API key (or press Enter to skip): ");
    io::stdout().flush().ok();

    let mut input = String::new();
    let Ok(_) = io::stdin().lock().read_line(&mut input) else {
        return String::new();
    };
    let key = input.trim().to_string();

    if key.is_empty() {
        return String::new();
    }

    if !key.starts_with(expected_prefix) {
        print_colored(
            "  Warning: key doesn't start with expected prefix. Using anyway.\n",
            Color::Yellow,
        );
    }

    key
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "*".repeat(key.len());
    }
    format!("{}...{}", &key[..6], &key[key.len() - 4..])
}

// ── Step 4: Claude Code plugin ──────────────────────────────

fn step_plugin_install() {
    print_step(4, 5, "Claude Code Integration");
    println!();
    println!("  Colony includes a Claude Code plugin that gives Claude");
    println!("  awareness of your shell environment and canvas.");
    println!();

    let options = vec![
        "Install plugin to ~/.claude/plugins/colony/".to_string(),
        "Skip".to_string(),
    ];
    let choice = prompt_choice(&options, 0);

    if choice == 0 {
        // Placeholder: actual plugin install logic would go here
        println!();
        print_colored("  Plugin install: ", Color::Yellow);
        println!("not yet implemented (placeholder)");
        println!("  Run `colony plugin install claude-code` when ready.");
    } else {
        println!();
        println!("  Skipped. Run `colony plugin install claude-code` later.");
    }
    println!();
}

// ── Step 5: Quick tour ──────────────────────────────────────

fn step_quick_tour() -> bool {
    print_step(5, 5, "Quick Tour?");
    println!();
    println!("  Want a 2-minute interactive tour of Colony's features?");
    println!();

    let options = vec![
        "Yes, show me around".to_string(),
        "No, drop me at the prompt".to_string(),
    ];
    let choice = prompt_choice(&options, 1);

    println!();
    choice == 0
}

// ── Config generation ────────────────────────────────────────

fn build_config_toml(keybindings: &str, provider: &str, imported_from: &str) -> String {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let model = match provider {
        "anthropic" => "claude-sonnet-4-5-20250514",
        "openai" => "gpt-4o",
        "ollama" => "llama3",
        _ => "",
    };

    let mut config = String::new();
    config.push_str("[shell]\n");
    config.push_str(&format!("keybindings = \"{keybindings}\"\n"));
    config.push_str("theme = \"colony-dark\"\n");
    config.push('\n');

    if provider != "skip" {
        config.push_str("[agent]\n");
        config.push_str(&format!("provider = \"{provider}\"\n"));
        if !model.is_empty() {
            config.push_str(&format!("model = \"{model}\"\n"));
        }
        config.push_str("# API key stored in environment variable, NOT in config file\n");
        config.push('\n');
    }

    config.push_str("[onboarding]\n");
    config.push_str("completed = true\n");
    config.push_str(&format!("completed_at = \"{now}\"\n"));
    if !imported_from.is_empty() {
        config.push_str(&format!("imported_from = \"{imported_from}\"\n"));
    }

    config
}

// ── Prompt helpers ───────────────────────────────────────────

fn print_step(step: usize, total: usize, title: &str) {
    print_colored(&format!("[{step}/{total}] "), Color::Cyan);
    print!("{}", SetAttribute(Attribute::Bold));
    print!("{title}");
    println!("{}", SetAttribute(Attribute::Reset));
}

fn print_separator() {
    print_colored(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
        Color::DarkGrey,
    );
    println!();
}

fn print_colored(text: &str, color: Color) {
    print!("{}{text}{}", SetForegroundColor(color), ResetColor);
}

/// Display a list of options and return the selected index.
///
/// Shows a `>` marker next to the default option. Accepts a number
/// (1-based) or Enter for the default.
fn prompt_choice(options: &[String], default: usize) -> usize {
    for (i, option) in options.iter().enumerate() {
        let marker = if i == default { ">" } else { " " };
        let num = i + 1;
        if i == default {
            print!("  {marker} ");
            print_colored(&format!("{num}. {option}"), Color::White);
            println!();
        } else {
            println!("  {marker} {num}. {option}");
        }
    }

    println!();
    print!("  Choice [{}]: ", default + 1);
    io::stdout().flush().ok();

    let mut input = String::new();
    let Ok(_) = io::stdin().lock().read_line(&mut input) else {
        return default;
    };
    let input = input.trim();

    if input.is_empty() {
        return default;
    }

    input
        .parse::<usize>()
        .ok()
        .filter(|&n| n >= 1 && n <= options.len())
        .map(|n| n - 1)
        .unwrap_or(default)
}

// ── First-run detection ──────────────────────────────────────

/// Check if this is the first run of Colony Shell.
///
/// Returns `true` if `~/.colony/config.toml` does not exist,
/// indicating onboarding has not been completed.
pub fn is_first_run() -> bool {
    let config_path = colony_dir().join("config.toml");
    !config_path.exists()
}

/// Get the Colony configuration directory path.
fn colony_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".colony")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_config_toml_anthropic() {
        let config = build_config_toml("vim", "anthropic", "zsh");
        assert!(config.contains("keybindings = \"vim\""));
        assert!(config.contains("provider = \"anthropic\""));
        assert!(config.contains("model = \"claude-sonnet-4-5-20250514\""));
        assert!(config.contains("completed = true"));
        assert!(config.contains("imported_from = \"zsh\""));
        assert!(config.contains("NOT in config file"));
    }

    #[test]
    fn build_config_toml_skip_provider() {
        let config = build_config_toml("emacs", "skip", "");
        assert!(config.contains("keybindings = \"emacs\""));
        assert!(!config.contains("[agent]"));
        assert!(!config.contains("imported_from"));
    }

    #[test]
    fn build_config_toml_ollama() {
        let config = build_config_toml("hybrid", "ollama", "bash");
        assert!(config.contains("provider = \"ollama\""));
        assert!(config.contains("model = \"llama3\""));
        assert!(config.contains("imported_from = \"bash\""));
    }

    #[test]
    fn mask_key_short() {
        assert_eq!(mask_key("abc"), "***");
    }

    #[test]
    fn mask_key_long() {
        let masked = mask_key("sk-ant-api03-abcdefghijk");
        assert!(masked.starts_with("sk-ant"));
        assert!(masked.ends_with("hijk"));
        assert!(masked.contains("..."));
    }
}
