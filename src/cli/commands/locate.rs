//! `callgraph locate <kifejezés>… [--all] [--project <alias|út>]`: kiindulópont-kereső — a feladat neveiből és
//! fogalmaiból rangsorolt „innen indulj” fájl-lista a projekt-térképből (`crate::locate`).

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::locate::Locator;
use crate::skill::SkillHome;

/// A kiindulópont-kereső parancs; a `--project` aliast a `SkillHome` registryje oldja fel.
pub struct LocateCommand {
    terms: Vec<String>,
    all: bool,
    project: Option<String>,
    home: SkillHome,
}

impl LocateCommand {
    pub fn new(terms: Vec<String>, all: bool, project: Option<String>, home: SkillHome) -> Self {
        Self {
            terms,
            all,
            project,
            home,
        }
    }
}

impl Command for LocateCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let (text, broken) = Locator::new(&root).run(&self.terms, self.all)?;
        out.write_all(text.as_bytes())?;
        Ok(crate::cli::strict::exit_code(broken > 0))
    }
}
