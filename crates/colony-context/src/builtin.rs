//! `context` builtin command.
//!
//! Usage:
//!   context show [--tokens]     Show current context summary
//!   context add <text>          Add knowledge entry
//!   context add --file <path>   Add file content
//!   context remove <id>         Remove entry by ID (prefix match)
//!   context clear [--all]       Clear non-pinned entries (--all for everything)
//!   context pin <id>            Pin an entry
//!   context prune <max_tokens>  Prune to fit token budget

use crate::entry::{ContextEntry, ContextKind};
use crate::store::ContextStore;

/// Result of running a context subcommand.
pub enum BuiltinResult {
    /// Text to print to stdout.
    Output(String),
    /// Error message.
    Error(String),
}

/// Execute a `context` builtin invocation.
pub fn execute(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let Some(subcmd) = args.first() else {
        return execute_show(store, project_id, false);
    };

    match *subcmd {
        "show" => {
            let tokens_flag = args.iter().any(|a| *a == "--tokens");
            execute_show(store, project_id, tokens_flag)
        }
        "add" => execute_add(&args[1..], store, project_id),
        "remove" => execute_remove(&args[1..], store, project_id),
        "clear" => {
            let all = args.iter().any(|a| *a == "--all");
            execute_clear(store, project_id, all)
        }
        "pin" => execute_pin(&args[1..], store, project_id),
        "prune" => execute_prune(&args[1..], store, project_id),
        _ => BuiltinResult::Error(format!("unknown subcommand: {subcmd}")),
    }
}

fn execute_show(store: &ContextStore, project_id: &str, show_tokens: bool) -> BuiltinResult {
    let file = match store.load(project_id) {
        Ok(f) => f,
        Err(e) => return BuiltinResult::Error(format!("failed to load context: {e}")),
    };

    if file.entries.is_empty() {
        return BuiltinResult::Output("Context is empty.".to_string());
    }

    let mut lines = Vec::new();
    let total = file.total_tokens();
    lines.push(format!(
        "Context: {} entries, ~{} tokens",
        file.entries.len(),
        total
    ));
    lines.push(String::new());

    for entry in &file.entries {
        let pin = if entry.metadata.pinned {
            " [pinned]"
        } else {
            ""
        };
        let id_short = &entry.id.to_string()[..8];
        let label = entry.label.as_deref().unwrap_or("\u{2014}");
        let kind = format!("{:?}", entry.kind);

        let mut line = format!("  {id_short}  {kind:<12}  {label}");
        if show_tokens {
            line.push_str(&format!("  (~{} tokens)", entry.metadata.tokens));
        }
        line.push_str(pin);
        lines.push(line);
    }

    BuiltinResult::Output(lines.join("\n"))
}

fn execute_add(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::Error(
            "usage: context add <text> | context add --file <path>".to_string(),
        );
    }

    if args.first() == Some(&"--file") {
        let Some(path) = args.get(1) else {
            return BuiltinResult::Error("usage: context add --file <path>".to_string());
        };
        let Ok(content) = std::fs::read_to_string(path) else {
            return BuiltinResult::Error(format!("failed to read file: {path}"));
        };
        let entry = ContextEntry::new(ContextKind::File, content).with_label(*path);
        if let Err(e) = store.add_entry(project_id, entry) {
            return BuiltinResult::Error(format!("failed to add: {e}"));
        }
        return BuiltinResult::Output(format!("Added file: {path}"));
    }

    let text = args.join(" ");
    let entry = ContextEntry::new(ContextKind::Knowledge, text);
    if let Err(e) = store.add_entry(project_id, entry) {
        return BuiltinResult::Error(format!("failed to add: {e}"));
    }
    BuiltinResult::Output("Added to context.".to_string())
}

fn execute_remove(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let Some(id_prefix) = args.first() else {
        return BuiltinResult::Error("usage: context remove <id-prefix>".to_string());
    };

    let file = match store.load(project_id) {
        Ok(f) => f,
        Err(e) => return BuiltinResult::Error(format!("failed to load: {e}")),
    };

    let matching: Vec<&ContextEntry> = file
        .entries
        .iter()
        .filter(|e| e.id.to_string().starts_with(id_prefix))
        .collect();

    if matching.is_empty() {
        return BuiltinResult::Error(format!("no entry matching: {id_prefix}"));
    }
    if matching.len() > 1 {
        return BuiltinResult::Error(format!(
            "{} entries match \u{2014} be more specific",
            matching.len()
        ));
    }

    let id = matching[0].id;
    match store.remove_entry(project_id, id) {
        Ok(true) => BuiltinResult::Output("Removed.".to_string()),
        Ok(false) => BuiltinResult::Error("entry not found".to_string()),
        Err(e) => BuiltinResult::Error(format!("failed to remove: {e}")),
    }
}

fn execute_clear(store: &ContextStore, project_id: &str, all: bool) -> BuiltinResult {
    match store.clear(project_id, !all) {
        Ok(n) => BuiltinResult::Output(format!("Cleared {n} entries.")),
        Err(e) => BuiltinResult::Error(format!("failed to clear: {e}")),
    }
}

fn execute_pin(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let Some(id_prefix) = args.first() else {
        return BuiltinResult::Error("usage: context pin <id-prefix>".to_string());
    };

    let file = match store.load(project_id) {
        Ok(f) => f,
        Err(e) => return BuiltinResult::Error(format!("failed to load: {e}")),
    };

    // Find entry by prefix
    let matching: Vec<&ContextEntry> = file
        .entries
        .iter()
        .filter(|e| e.id.to_string().starts_with(id_prefix))
        .collect();

    if matching.is_empty() {
        return BuiltinResult::Error(format!("no entry matching: {id_prefix}"));
    }
    if matching.len() > 1 {
        return BuiltinResult::Error(format!(
            "{} entries match \u{2014} be more specific",
            matching.len()
        ));
    }

    let id = matching[0].id;
    match store.pin_entry(project_id, id) {
        Ok(true) => BuiltinResult::Output("Pinned.".to_string()),
        Ok(false) => BuiltinResult::Error("entry not found".to_string()),
        Err(e) => BuiltinResult::Error(format!("failed to pin: {e}")),
    }
}

fn execute_prune(args: &[&str], store: &ContextStore, project_id: &str) -> BuiltinResult {
    let max_tokens: usize = args
        .first()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    match store.prune(project_id, max_tokens) {
        Ok(n) => BuiltinResult::Output(format!("Pruned {n} entries.")),
        Err(e) => BuiltinResult::Error(format!("failed to prune: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, ContextStore) {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());
        (tmp, store)
    }

    #[test]
    fn show_empty() {
        let (_tmp, store) = make_store();
        let result = execute(&["show"], &store, "proj");
        match result {
            BuiltinResult::Output(s) => assert!(s.contains("empty")),
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn add_and_show() {
        let (_tmp, store) = make_store();
        let result = execute(&["add", "hello", "world"], &store, "proj");
        match result {
            BuiltinResult::Output(s) => assert!(s.contains("Added")),
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }

        let result = execute(&["show"], &store, "proj");
        match result {
            BuiltinResult::Output(s) => {
                assert!(s.contains("1 entries"));
                assert!(s.contains("Knowledge"));
            }
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn show_with_tokens() {
        let (_tmp, store) = make_store();
        execute(&["add", "some", "text"], &store, "proj");

        let result = execute(&["show", "--tokens"], &store, "proj");
        match result {
            BuiltinResult::Output(s) => assert!(s.contains("tokens")),
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn clear_entries() {
        let (_tmp, store) = make_store();
        execute(&["add", "one"], &store, "proj");
        execute(&["add", "two"], &store, "proj");

        let result = execute(&["clear"], &store, "proj");
        match result {
            BuiltinResult::Output(s) => assert!(s.contains("Cleared 2")),
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn prune_entries() {
        let (_tmp, store) = make_store();
        execute(&["add", "big", "text", "entry"], &store, "proj");

        let result = execute(&["prune", "0"], &store, "proj");
        match result {
            BuiltinResult::Output(s) => assert!(s.contains("Pruned")),
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn unknown_subcommand() {
        let (_tmp, store) = make_store();
        let result = execute(&["bogus"], &store, "proj");
        match result {
            BuiltinResult::Error(s) => assert!(s.contains("unknown subcommand")),
            BuiltinResult::Output(s) => panic!("expected error, got: {s}"),
        }
    }

    #[test]
    fn no_args_shows_context() {
        let (_tmp, store) = make_store();
        let result = execute(&[], &store, "proj");
        match result {
            BuiltinResult::Output(s) => assert!(s.contains("empty")),
            BuiltinResult::Error(e) => panic!("unexpected error: {e}"),
        }
    }
}
