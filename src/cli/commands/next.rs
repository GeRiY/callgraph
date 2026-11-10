//! `callgraph next <rN> <k>[,<k>] [--project X]` — egy korábbi eredmény javasolt következő lépésének futtatása
//! az azonosítójával (`crate::next::run_choices`): a tárolt parancs pontosan, ugyanazzal a binárissal, folyamaton
//! belül.

use std::io::Write;

use crate::cli::Command;
use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

/// A `next` parancs.
pub struct NextCommand {
    id: String,
    choices: Vec<usize>,
    project: Option<String>,
    home: SkillHome,
}

impl NextCommand {
    pub fn new(id: String, choices: Vec<usize>, project: Option<String>, home: SkillHome) -> Self {
        Self {
            id,
            choices,
            project,
            home,
        }
    }
}

impl Command for NextCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        // ismeretlen / lejárt azonosító, hibás választás: használati hiba, 2-es kód (mint a hibás kapcsoló)
        match crate::next::run_choices(&root, &self.id, &self.choices, out, err) {
            Err(CallgraphError::Usage(message)) => {
                writeln!(err, "{message}")?;
                Ok(2)
            }
            other => other,
        }
    }
}
