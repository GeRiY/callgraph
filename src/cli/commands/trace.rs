//! `callgraph trace <target> [--project <alias|path>]`: the task slice of a route, controller action or page
//! (`crate::packages::frameworks::trace`). An unknown target: one hint line, exit code 0.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::SkillHome;

/// The trace command; the `--project` alias is resolved through the `SkillHome` registry.
pub struct TraceCommand {
    target: String,
    project: Option<String>,
    home: SkillHome,
}

impl TraceCommand {
    pub fn new(target: String, project: Option<String>, home: SkillHome) -> Self {
        Self {
            target,
            project,
            home,
        }
    }
}

impl Command for TraceCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let lang = crate::i18n::lang();
        let text = crate::packages::frameworks::trace::run(&root, &self.target, lang);
        out.write_all(text.as_bytes())?;
        Ok(0)
    }
}
