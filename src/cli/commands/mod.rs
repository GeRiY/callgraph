//! A CLI-alparancsok (egy fájl = egy parancs).

mod analyze;
mod callers;
mod error;
mod feedback;
mod generate;
mod help;
mod hook;
mod lang;
mod locate;
mod mcp;
mod next;
mod note;
mod profile;
mod project;
mod prune;
mod refresh;
mod self_update;
mod show;
mod skill_sync;
mod stats;
mod telemetry;
mod usage_check;
mod version;
mod wrap;

use std::path::{Path, PathBuf};

use crate::error::{CallgraphError, Result};
use crate::skill::{ProjectRegistry, SkillHome};

pub use analyze::AnalyzeCommand;
pub use callers::CallersCommand;
pub use error::ErrorCommand;
pub use feedback::FeedbackCommand;
pub use generate::{GenerateCommand, UndoCommand};
pub use help::HelpCommand;
pub use hook::HookCommand;
pub use lang::LangCommand;
pub use locate::LocateCommand;
pub use mcp::McpCommand;
pub use next::NextCommand;
pub use note::NoteCommand;
pub use profile::ProfileCommand;
pub use project::ProjectCommand;
pub use prune::PruneCommand;
pub use refresh::RefreshCommand;
pub use self_update::SelfUpdateCommand;
pub use show::ShowCommand;
pub use skill_sync::SkillSyncCommand;
pub use stats::StatsCommand;
pub use telemetry::TelemetryCommand;
pub use usage_check::UsageCheckCommand;
pub use version::VersionCommand;
pub use wrap::WrapCommand;

/// A `--project <alias|út>` gyökere (kanonikus): előbb a `projects.json` aliasa, utána mappa-út; nélküle a
/// munkakönyvtár (`note`, `refresh`).
pub(crate) fn project_root(home: &SkillHome, project: Option<&str>) -> Result<PathBuf> {
    let root = match project {
        None => std::env::current_dir()?,
        Some(value) => {
            let file = home.registry_file();
            let alias = if file.is_file() {
                ProjectRegistry::load(file)?.root(value)
            } else {
                None
            };
            match alias {
                Some(path) => PathBuf::from(path),
                None if Path::new(value).is_dir() => PathBuf::from(value),
                None => {
                    return Err(CallgraphError::Usage(crate::tf!(
                        "Unknown project: {value} (neither a registered alias nor a directory)",
                        "Ismeretlen projekt: {value} (se nem regisztrált alias, se nem mappa)"
                    )));
                }
            }
        }
    };
    Ok(root.canonicalize().unwrap_or(root))
}
