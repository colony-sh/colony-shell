use clap::{ArgAction, Parser as ClapParser};
use std::path::PathBuf;

#[derive(Debug, ClapParser, Default, Clone)]
#[command(
    name = "colony",
    version = env!("CARGO_PKG_VERSION"),
    about = "POSIX-compliant shell with AI agent integration",
    disable_help_subcommand = true
)]
pub(crate) struct ShellOptions {
    #[arg(short = 'l', long, action = ArgAction::SetTrue)]
    pub(crate) login: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) noprofile: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) norc: bool,

    #[arg(long, alias = "init-file", value_name = "FILE")]
    pub(crate) rcfile: Option<PathBuf>,

    #[arg(short = 'c', value_name = "STRING", conflicts_with = "script")]
    pub(crate) command: Option<String>,

    #[arg(value_name = "SCRIPT", conflicts_with = "command")]
    pub(crate) script: Option<PathBuf>,

    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    pub(crate) positional_args: Vec<String>,

    #[arg(long, short = 'v', action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,

    #[arg(long, short = 'T', action = ArgAction::SetTrue)]
    pub(crate) tutorial: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) teach: bool,

    #[arg(long, default_value_t = 2, env = "COLONY_DEPTH", value_name = "LEVEL")]
    pub(crate) depth: u8,

    /// Start the script debugger (breakpoints, stepping, inspection)
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,

    /// Start the X-Ray tracer (auto-play with real-time annotations)
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) xray: bool,
}

impl ShellOptions {
    pub(crate) fn parse() -> Self {
        let mut opts = ShellOptions::parse_from(std::env::args_os());

        if let Some(argv0) = std::env::args_os().next() {
            if let Some(basename) = std::path::Path::new(&argv0).file_name() {
                if let Some(s) = basename.to_str() {
                    if s.starts_with('-') {
                        opts.login = true;
                    }
                }
            }
        }

        opts
    }

    pub(crate) fn is_interactive(&self) -> bool {
        self.command.is_none() && self.script.is_none() && unsafe { libc::isatty(0) != 0 }
    }
}
