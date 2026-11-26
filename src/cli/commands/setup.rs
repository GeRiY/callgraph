//! `callgraph setup [--client <list>] [--project <dir> | --user] [--dry-run] [--uninstall] [--yes]` — wires callgraph
//! into the AI coding clients (`crate::setup`). Never asks anything (`--yes` is accepted for scripts).

use std::io::Write;
use std::path::PathBuf;

use crate::cli::Command;
use crate::error::{CallgraphError, Result};
use crate::setup::{Env, Options};

pub struct SetupCommand {
    options: Options,
}

impl SetupCommand {
    pub fn new(options: Options) -> Self {
        Self { options }
    }
}

impl Command for SetupCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let project = match &self.options.project {
            Some(dir) => dir.clone(),
            None => std::env::current_dir()?,
        };
        if !project.is_dir() {
            return Err(CallgraphError::Usage(crate::tf!(
                "not a directory: {}",
                "nem mappa: {}",
                project.display()
            )));
        }
        let project = project.canonicalize().unwrap_or(project);
        let project = PathBuf::from(crate::query::strip_verbatim(&project.to_string_lossy()));
        crate::setup::run(&self.options, &Env::from_system(project)?, out)
    }
}
