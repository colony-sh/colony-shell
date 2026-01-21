use colony_exec::Executor;
use colony_expand::{
    expand_tilde, CompletionAction, CompletionSpec, ExpansionContext, ExpansionError,
};
use colony_parser::pest_api::parse_statement;
use colony_parser::Statement;
use std::path::PathBuf;

#[allow(dead_code)]
type Result<T> = std::result::Result<T, ExpansionError>;

pub(crate) async fn maybe_source_file(path: &str, executor: &mut Executor, verbose: bool) -> bool {
    let expanded = match expand_tilde(Some(path), executor.context()) {
        Ok(p) => PathBuf::from(p), // Convert String to PathBuf
        Err(e) => {
            if verbose {
                eprintln!("colony: failed to expand {}: {}", path, e);
            }
            return false;
        }
    };

    if expanded.is_file() {
        if verbose {
            eprintln!("colony: sourcing {}", expanded.display());
        }
        match executor.execute_source_file(&expanded, vec![]).await {
            Ok(_) => true,
            Err(e) => {
                eprintln!("colony: error sourcing {}: {}", expanded.display(), e);
                false
            }
        }
    } else {
        false
    }
}

pub(crate) fn load_exported_functions(context: &mut ExpansionContext) {
    const BASHFUNC_PREFIX: &str = "BASH_FUNC_";
    const BASHFUNC_SUFFIX: &str = "%%";

    for (key, value) in std::env::vars() {
        if key.starts_with(BASHFUNC_PREFIX) && key.ends_with(BASHFUNC_SUFFIX) {
            let name_start = BASHFUNC_PREFIX.len();
            let name_end = key.len() - BASHFUNC_SUFFIX.len();
            let func_name = &key[name_start..name_end];

            if !value.starts_with("() {") {
                continue;
            }

            let func_source = format!("{} {}", func_name, value);

            if let Ok(Statement::FunctionDef(func)) = parse_statement(&func_source) {
                context.define_function(func.name.clone(), func);
                context.export_function(func_name);
            }
        }
    }
}

pub(crate) async fn run_startup_files(opts: &crate::cli::ShellOptions, executor: &mut Executor) {
    let is_login = opts.login;
    let is_interactive = opts.is_interactive();
    let verbose = opts.verbose;

    if verbose {
        eprintln!("colony: login={}, interactive={}", is_login, is_interactive);
    }

    if is_login && !opts.noprofile {
        maybe_source_file("/etc/profile", executor, verbose).await;

        if !maybe_source_file("~/.bash_profile", executor, verbose).await
            && !maybe_source_file("~/.bash_login", executor, verbose).await
        {
            maybe_source_file("~/.profile", executor, verbose).await;
        }
    }

    if is_interactive && !is_login && !opts.norc {
        if let Some(ref rcfile) = opts.rcfile {
            if verbose {
                eprintln!("colony: sourcing {}", rcfile.display());
            }
            if rcfile.is_file() {
                if let Err(e) = executor.execute_source_file(rcfile, vec![]).await {
                    eprintln!("colony: error sourcing {}: {}", rcfile.display(), e);
                }
            } else {
                eprintln!("colony: {}: No such file or directory", rcfile.display());
            }
        } else {
            maybe_source_file("/etc/bash.bashrc", executor, verbose).await;
            maybe_source_file("~/.bashrc", executor, verbose).await;
        }
    }

    if !is_interactive {
        if let Ok(env_file) = std::env::var("BASH_ENV") {
            if !env_file.is_empty() {
                maybe_source_file(&env_file, executor, verbose).await;
            }
        } else if let Ok(env_file) = std::env::var("ENV") {
            if !env_file.is_empty() {
                maybe_source_file(&env_file, executor, verbose).await;
            }
        }
    }

    executor
        .context_mut()
        .set_var("COLONY_STARTUP_SOURCED".to_string(), "1".to_string());

    register_builtin_completions(executor.context_mut());
}

fn register_builtin_completions(context: &mut ExpansionContext) {
    let mut path_spec = CompletionSpec::new();
    path_spec.wordlist = Some("add prepend remove list has".to_string());
    path_spec.actions.push(CompletionAction::Directory);
    context.set_completion("path".to_string(), path_spec);
}
