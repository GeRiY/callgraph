//! `callgraph feedback add|resolve|list|show` — a `feedback.sh` (jq) utódja.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::{ErrorLog, FeedbackStore, SkillFailure, SkillHome, finish};

type Outcome = std::result::Result<(), SkillFailure>;

const SOURCE: &str = "feedback.sh";
const S: &str = "bash ~/.claude/skills/callgraph/feedback.sh";

pub struct FeedbackCommand {
    args: Vec<String>,
    home: SkillHome,
}

impl FeedbackCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self { args, home }
    }

    fn arg(&self, i: usize) -> &str {
        self.args.get(i).map_or("", String::as_str)
    }

    fn exec(&self, out: &mut dyn Write, err: &mut dyn Write) -> Outcome {
        let store = FeedbackStore::new(self.home.feedback_file());
        let errors = ErrorLog::new(self.home.errors_file());
        match self.arg(0) {
            "add" => {
                let (error_id, description) = if self.arg(1) == "--note" {
                    ("", self.arg(2))
                } else {
                    (self.arg(1), self.arg(2))
                };
                if description.is_empty() {
                    return Err(SkillFailure::usage(crate::tf!(
                        "Usage: {S} add <error_id> \"<description>\"  OR  {S} add --note \"<description>\"",
                        "Használat: {S} add <error_id> \"<leírás>\"  VAGY  {S} add --note \"<leírás>\""
                    )));
                }
                if !error_id.is_empty() && errors.raw_matching(error_id).is_empty() {
                    writeln!(
                        err,
                        "{}",
                        crate::tf!(
                            "warning: no logged error with id '{error_id}' — the description is saved anyway, unlinked",
                            "figyelem: nincs '{error_id}' azonosítójú naplózott hiba — a leírás így is, kapcsolat nélkül, elmentve"
                        )
                    )?;
                }
                let id = store.add(Some(error_id).filter(|e| !e.is_empty()), description)?;
                writeln!(
                    out,
                    "{}",
                    crate::tf!(
                        "saved  [feedback-id: {id}]",
                        "elmentve  [feedback-id: {id}]"
                    )
                )?;
            }
            "resolve" => {
                let id = self.arg(1);
                if id.is_empty() {
                    return Err(SkillFailure::usage(crate::tf!(
                        "Usage: {S} resolve <feedback_id>",
                        "Használat: {S} resolve <feedback_id>"
                    )));
                }
                if !store.resolve(id)? {
                    return Err(SkillFailure::not_found(crate::tf!(
                        "No feedback entry with id '{id}' (see: {S} list)",
                        "Nincs '{id}' azonosítójú feedback-bejegyzés (lásd: {S} list)"
                    )));
                }
                writeln!(out, "{}", crate::tf!("resolved: {id}", "megoldva: {id}"))?;
            }
            "list" => {
                if !store.exists() {
                    writeln!(out, "{}", crate::t!("(no feedback)", "(nincs feedback)"))?;
                    return Ok(());
                }
                for line in store.list(self.arg(1) == "--open", &errors) {
                    writeln!(out, "{line}")?;
                }
            }
            "show" => {
                let id = self.arg(1);
                if id.is_empty() {
                    return Err(SkillFailure::usage(crate::tf!(
                        "Usage: {S} show <error_id>",
                        "Használat: {S} show <error_id>"
                    )));
                }
                if errors.exists() {
                    writeln!(out, "{}", crate::t!("-- error --", "-- hiba --"))?;
                    for line in errors.raw_matching(id) {
                        writeln!(out, "{line}")?;
                    }
                }
                if store.exists() {
                    writeln!(
                        out,
                        "{}",
                        crate::t!("-- related feedback --", "-- kapcsolódó feedback --")
                    )?;
                    for line in store.raw_for_error(id) {
                        writeln!(out, "{line}")?;
                    }
                }
            }
            _ => {
                return Err(SkillFailure::usage(crate::tf!(
                    "Usage: {S} add <error_id> \"<description>\" | add --note \"<description>\" | resolve <feedback_id> | list [--open] | show <error_id>",
                    "Használat: {S} add <error_id> \"<leírás>\" | add --note \"<leírás>\" | resolve <feedback_id> | list [--open] | show <error_id>"
                )));
            }
        }
        Ok(())
    }
}

impl Command for FeedbackCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let result = self.exec(out, err);
        finish(&self.home, SOURCE, result, err)
    }
}
