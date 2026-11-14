//! `callgraph gen '<kifejezés>' [--dry|--write|--into|--replace …]` és `callgraph undo [<fájl>]` — a kódgenerátor
//! (`crate::generate`) CLI-je. A kimenet egy-két sor (mi hova íródott, melyik mentés készült, melyik név nem
//! oldódott fel); kódot csak a `--dry` ír ki. Hiba: egy sor a stderr-en, 2-es kilépési kód, a fájlok érintetlenek.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::generate::{
    Env, GenRequest, Generator, PhpEmitter, RealFs, SystemClock, VueGenerator, wants_vue,
};
use crate::skill::SkillHome;

/// A `gen` parancs.
pub struct GenerateCommand {
    request: GenRequest,
    project: Option<String>,
    home: SkillHome,
}

impl GenerateCommand {
    pub fn new(request: GenRequest, project: Option<String>, home: SkillHome) -> Self {
        Self {
            request,
            project,
            home,
        }
    }
}

impl Command for GenerateCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let env = Env {
            root: &root,
            fs: &RealFs,
            clock: &SystemClock,
        };
        // Vue / JS / TS: a cél-fájl kiterjesztése (vagy cél nélkül a kifejezés feje) dönt; minden más a PHP-generátoré
        let result = if wants_vue(&self.request) {
            VueGenerator::new(env).run(&self.request)
        } else {
            Generator::new(env, PhpEmitter).run(&self.request)
        };
        match result {
            Ok(output) => {
                write!(out, "{}", output.render())?;
                Ok(0)
            }
            Err(e) => {
                writeln!(err, "gen: {e}")?;
                Ok(2)
            }
        }
    }
}

/// Az `undo` parancs.
pub struct UndoCommand {
    file: Option<String>,
    project: Option<String>,
    home: SkillHome,
}

impl UndoCommand {
    pub fn new(file: Option<String>, project: Option<String>, home: SkillHome) -> Self {
        Self {
            file,
            project,
            home,
        }
    }
}

impl Command for UndoCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let env = Env {
            root: &root,
            fs: &RealFs,
            clock: &SystemClock,
        };
        match Generator::new(env, PhpEmitter).undo(self.file.as_deref()) {
            Ok(output) => {
                write!(out, "{}", output.render())?;
                Ok(0)
            }
            Err(e) => {
                writeln!(err, "undo: {e}")?;
                Ok(2)
            }
        }
    }
}
