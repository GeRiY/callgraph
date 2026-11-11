//! CLI-réteg: argumentumok → Command → futtatás.

mod args;
mod command;
pub mod commands;
mod next_args;
mod profile_args;
pub mod signatures;
mod stats_args;
pub mod strict;
mod telemetry_args;

pub use args::{CliArgs, HookAction, NO_GITIGNORE_FLAG, NoteAction, take_no_gitignore};
pub use command::Command;
pub use profile_args::LayerArgs;
pub use telemetry_args::TelemetryAction;

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
            by_layer,
        } => Ok(Box::new(
            commands::CallersCommand::new(pattern, depth, project).by_layer(by_layer),
        )),
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
            body_mode,
            grep,
            category,
            depth,
            guess,
            notes,
            layers,
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
            body_mode,
            grep,
            category,
            depth,
            guess,
            notes,
            layers,
        ))),
        CliArgs::Note { action, project } => Ok(Box::new(commands::NoteCommand::new(
            action,
            project,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Refresh {
            paths,
            project,
            background,
        } => Ok(Box::new(commands::RefreshCommand::new(
            paths,
            project,
            background,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Locate {
            terms,
            all,
            project,
        } => Ok(Box::new(commands::LocateCommand::new(
            terms,
            all,
            project,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Hook { action, settings } => Ok(Box::new(commands::HookCommand::new(
            action,
            settings,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Next {
            id,
            choices,
            project,
        } => Ok(Box::new(commands::NextCommand::new(
            id,
            choices,
            project,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Telemetry { action } => Ok(Box::new(commands::TelemetryCommand::new(
            action,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Version => Ok(Box::new(commands::VersionCommand)),
        CliArgs::Stats { days } => Ok(Box::new(commands::StatsCommand::new(
            days,
            crate::skill::SkillHome::from_env(),
        ))),
        CliArgs::Profile {
            project,
            full,
            with_vendor,
        } => Ok(Box::new(
            commands::ProfileCommand::new(project, full, crate::skill::SkillHome::from_env())
                .with_vendor(with_vendor),
        )),
        CliArgs::Mcp { auto_update } => Ok(Box::new(commands::McpCommand::new(
            crate::skill::SkillHome::from_env(),
            auto_update,
        ))),
        CliArgs::Skill { command, args } => {
            let home = crate::skill::SkillHome::from_env();
            Ok(match command.as_str() {
                "project" => Box::new(commands::ProjectCommand::new(args, home)),
                "feedback" => Box::new(commands::FeedbackCommand::new(args, home)),
                "error" => Box::new(commands::ErrorCommand::new(args, home)),
                "set-lang" => Box::new(commands::LangCommand::new(args, home)),
                "self-update" => Box::new(commands::SelfUpdateCommand::new(args, home)),
                "skill-sync" => Box::new(commands::SkillSyncCommand::new(args, home)),
                _ => Box::new(commands::WrapCommand::new(args, home)),
            })
        }
    }
}
