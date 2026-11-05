//! Hibanapló (`errors.jsonl`) és a hibakimenet: üzenet + `[error-id: X]  report: <GitHub issue-űrlap> …` sor, kilépési kód 1.
//! A hibát GitHub issue-ban jelezzük (mint a weboldalon); a `feedback.sh show <id>` adja a bemásolandó naplóbejegyzést.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::SkillHome;
use super::clock::{new_id, now_iso};
use crate::error::Result;

/// Egy naplózott hiba (a mezősorrend a régi `jq -nc` kimenetéé).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorRow {
    pub id: String,
    pub ts: String,
    pub source: String,
    pub context: String,
    pub message: String,
}

/// Hozzáfűzős hibanapló; olvasáskor a hibás sorokat kihagyja.
pub struct ErrorLog {
    file: PathBuf,
}

impl ErrorLog {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// Naplózza a hibát, és visszaadja az azonosítóját (sikertelen írásnál is — az üzenet a fontosabb).
    pub fn log(&self, source: &str, context: &str, message: &str) -> String {
        let row = ErrorRow {
            id: new_id(),
            ts: now_iso(),
            source: source.to_owned(),
            context: context.to_owned(),
            message: message.to_owned(),
        };
        if let Ok(mut line) = serde_json::to_string(&row) {
            line.push('\n');
            let _ = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.file)
                .and_then(|mut f| f.write_all(line.as_bytes()));
        }
        row.id
    }

    pub fn exists(&self) -> bool {
        self.file.is_file()
    }

    /// Az azonosítóhoz tartozó sorok, nyers (tömörített) JSON-ként.
    pub fn raw_matching(&self, id: &str) -> Vec<String> {
        super::feedback::raw_matching(&self.file, "id", id)
    }

    /// Az összes értelmezhető bejegyzés.
    pub fn all(&self) -> Vec<ErrorRow> {
        std::fs::read_to_string(&self.file)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
}

/// A bash `fail` mása: naplóz, kiírja az üzenetet és a visszajelzés-sort, majd 1-es kilépési kódot ad.
pub fn fail(
    home: &SkillHome,
    source: &str,
    context: &str,
    message: &str,
    err: &mut dyn Write,
) -> Result<i32> {
    let message = home.localize(message);
    let id = ErrorLog::new(home.errors_file()).log(source, context, &message);
    writeln!(err, "{message}")?;
    let feedback = home.script("feedback.sh");
    writeln!(
        err,
        "{}",
        crate::tf!(
            "[error-id: {id}]  report: {ISSUE_FORM}  (details to paste: {feedback} show {id})",
            "[error-id: {id}]  hibajelentés: {ISSUE_FORM}  (bemásolandó részletek: {feedback} show {id})"
        )
    )?;
    Ok(1)
}

/// A GitHub hibajelentő űrlap (a projektoldal „Bugs and feedback” részével azonos).
pub(crate) const ISSUE_FORM: &str =
    "https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml";

/// Egy skill-parancs hibája: a `fail` kontextusa (`usage` / `not_found`) és az üzenet.
#[derive(Debug)]
pub struct SkillFailure {
    pub context: &'static str,
    pub message: String,
}

impl SkillFailure {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            context: "usage",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            context: "not_found",
            message: message.into(),
        }
    }
}

impl From<crate::error::CallgraphError> for SkillFailure {
    fn from(e: crate::error::CallgraphError) -> Self {
        Self::usage(e.to_string())
    }
}

impl From<std::io::Error> for SkillFailure {
    fn from(e: std::io::Error) -> Self {
        Self::usage(crate::error::CallgraphError::Write(e).to_string())
    }
}

/// Egy skill-parancs eredménye kilépési kóddá: siker → 0, hiba → `fail` (napló + error-id sor) → 1.
pub fn finish(
    home: &SkillHome,
    source: &str,
    result: std::result::Result<(), SkillFailure>,
    err: &mut dyn Write,
) -> Result<i32> {
    match result {
        Ok(()) => Ok(0),
        Err(f) => fail(home, source, f.context, &f.message, err),
    }
}
