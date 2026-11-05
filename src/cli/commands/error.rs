//! `callgraph error <forrás> <kontextus> <üzenet>` — a bash `fail` párja a régi, `lib.sh`-t source-oló scripteknek:
//! naplóz az `errors.jsonl`-be, kiírja az üzenetet és az `[error-id: X]` sort, 1-es kóddal lép ki.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::{SkillHome, fail};

pub struct ErrorCommand {
    args: Vec<String>,
    home: SkillHome,
}

impl ErrorCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self { args, home }
    }
}

impl Command for ErrorCommand {
    fn run(&self, _out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let arg = |i: usize| self.args.get(i).map_or("", String::as_str);
        let source = Some(arg(0)).filter(|s| !s.is_empty()).unwrap_or("callgraph");
        let context = Some(arg(1)).filter(|s| !s.is_empty()).unwrap_or("usage");
        fail(&self.home, source, context, arg(2), err)
    }
}
