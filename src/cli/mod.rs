//! CLI-réteg: argumentumok → Command → futtatás.

mod args;
mod command;
pub mod commands;

pub use args::CliArgs;
pub use command::Command;

use crate::error::Result;
use crate::lang::LanguageRegistry;

/// Az argumentumokból a futtatandó parancsot állítja elő (Command minta).
pub fn build_command(args: CliArgs) -> Result<Box<dyn Command>> {
    match args {
        CliArgs::Analyze {
            paths,
            lang,
            cache,
            no_cache,
        } => Ok(Box::new(commands::AnalyzeCommand::new(
            paths,
            lang,
            cache,
            no_cache,
            LanguageRegistry::with_defaults(),
        ))),
        CliArgs::Prune {
            paths,
            cache,
            dry_run,
        } => Ok(Box::new(commands::PruneCommand::new(paths, cache, dry_run))),
        CliArgs::Callers {
            pattern,
            depth,
            project,
        } => Ok(Box::new(commands::CallersCommand::new(
            pattern, depth, project,
        ))),
        CliArgs::Show {
            paths,
            project,
            label,
            format,
            full,
            limit,
            lines,
            strings,
            const_filter,
            body,
            grep,
            category,
            depth,
            guess,
        } => Ok(Box::new(commands::ShowCommand::new(
            paths,
            project,
            label,
            format,
            full,
            limit,
            lines,
            strings,
            const_filter,
            body,
            grep,
            category,
            depth,
            guess,
        ))),
    }
}
