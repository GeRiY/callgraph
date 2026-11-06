//! `callgraph profile [--project <alias|mappa>] [--full]`: a projekt-profil — architektúra, rétegek mappánként,
//! tervezési minták, ismeretlen mappák és a rétegek közti hívásirány (`crate::profile`). Tömör (~15 sor)
//! alapból; `--full`: mappánként, bizonyosság szerint, a hívásirány párjaival.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::profile::{Tip, render, with_graph};
use crate::skill::SkillHome;

/// A profil parancsa; a `--project` aliast a `SkillHome` registryje oldja fel (különben mappa-út, vagy a
/// munkakönyvtár).
pub struct ProfileCommand {
    project: Option<String>,
    full: bool,
    home: SkillHome,
}

impl ProfileCommand {
    pub fn new(project: Option<String>, full: bool, home: SkillHome) -> Self {
        Self {
            project,
            full,
            home,
        }
    }
}

impl Command for ProfileCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let profile = with_graph(&root);
        write!(
            out,
            "{}",
            render(&profile, crate::i18n::lang(), self.full, Tip::Show)
        )?;
        Ok(0)
    }
}
