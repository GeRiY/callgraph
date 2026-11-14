//! `callgraph rename <route|class|method> <régi> <új> [--dry] [--strings] [--keep-table]` — a biztonságos átnevezés
//! (`crate::rename`) CLI-je. A kimenet egy összesítő sor, fájlonként egy szám, a bizonytalan helyek és az `undo`
//! tipp. Hiba: egy sor a stderr-en, 2-es kilépési kód, a fájlok érintetlenek.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::generate::{Env, RealFs, SystemClock};
use crate::rename::{RenameRequest, Renamer};
use crate::skill::SkillHome;

/// A `rename` parancs.
pub struct RenameCommand {
    request: RenameRequest,
    project: Option<String>,
    home: SkillHome,
}

impl RenameCommand {
    pub fn new(request: RenameRequest, project: Option<String>, home: SkillHome) -> Self {
        Self {
            request,
            project,
            home,
        }
    }
}

impl Command for RenameCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let env = Env {
            root: &root,
            fs: &RealFs,
            clock: &SystemClock,
        };
        match Renamer::new(env).run(&self.request) {
            Ok(output) => {
                write!(out, "{}", output.render())?;
                Ok(0)
            }
            Err(e) => {
                writeln!(err, "rename: {e}")?;
                Ok(2)
            }
        }
    }
}
