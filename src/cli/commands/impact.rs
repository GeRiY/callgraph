//! `callgraph impact [--diff [<range>|-]] [--staged] [--depth N] [--project <alias|dir>] [--compact] [--format json]`:
//! the changed symbols of the git diff with their callers, framework links and tests (`crate::impact`). A git or
//! range problem: one error line, exit code 2.

use std::io::Write;

use crate::cli::Command;
use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

/// The impact command; the `--project` alias is resolved through the `SkillHome` registry.
pub struct ImpactCommand {
    request: crate::impact::Request,
    project: Option<String>,
    home: SkillHome,
}

impl ImpactCommand {
    pub fn new(request: crate::impact::Request, project: Option<String>, home: SkillHome) -> Self {
        Self {
            request,
            project,
            home,
        }
    }
}

impl Command for ImpactCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let stdin = std::io::stdin();
        match crate::impact::run(&root, &self.request, &mut stdin.lock()) {
            Ok(text) => {
                out.write_all(text.as_bytes())?;
                Ok(0)
            }
            // a user problem (no git, not a repository, a refused range): one line on the error channel
            Err(CallgraphError::Usage(message)) => {
                writeln!(err, "{message}")?;
                Ok(2)
            }
            Err(other) => Err(other),
        }
    }
}
