//! `callgraph skill-sync` — a skill-utasítás (`<skill>/SKILL.md`) előállítása az aktuális (érvényes) nyelven a
//! `skill-i18n/SKILL.<nyelv>.md` forrásból. A telepítő (`install.sh`) hívja; forrás nélkül csendben semmit sem tesz.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::i18n;
use crate::skill::{SkillFailure, SkillHome, finish, sync_skill_md};
use crate::{t, tf};

const SOURCE: &str = "callgraph skill-sync";

pub struct SkillSyncCommand {
    args: Vec<String>,
    home: SkillHome,
}

impl SkillSyncCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self { args, home }
    }

    fn sync(&self, out: &mut dyn Write) -> std::result::Result<(), SkillFailure> {
        if !self.args.is_empty() {
            return Err(SkillFailure::usage(t!(
                "usage: callgraph skill-sync",
                "használat: callgraph skill-sync"
            )));
        }
        let lang = i18n::lang();
        if sync_skill_md(&self.home, lang)? {
            writeln!(
                out,
                "{}",
                tf!(
                    "✓ skill instructions (SKILL.md): {}",
                    "✓ skill-utasítás (SKILL.md): {}",
                    lang.code()
                )
            )?;
        }
        Ok(())
    }
}

impl Command for SkillSyncCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let result = self.sync(out);
        finish(&self.home, SOURCE, result, err)
    }
}
