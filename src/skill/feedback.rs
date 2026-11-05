//! Visszajelzések (`feedback.jsonl`): rögzítés, megoldottnak jelölés, lista a kapcsolódó hibával.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::ErrorLog;
use super::clock::{new_id, now_iso};
use crate::error::Result;

pub struct FeedbackStore {
    file: PathBuf,
}

impl FeedbackStore {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    pub fn exists(&self) -> bool {
        self.file.is_file()
    }

    /// Új bejegyzés (`error_id` nélkül: általános észrevétel); a visszaadott érték a feedback-azonosító.
    pub fn add(&self, error_id: Option<&str>, description: &str) -> Result<String> {
        let id = new_id();
        let row = json!({
            "id": id, "ts": now_iso(), "error_id": error_id, "description": description,
            "resolved": false, "resolved_at": null,
        });
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)?;
        writeln!(f, "{row}")?;
        Ok(id)
    }

    /// Megoldottnak jelöli; `false`, ha nincs ilyen azonosító. A többi sor bájtra változatlan marad.
    pub fn resolve(&self, id: &str) -> Result<bool> {
        let text = std::fs::read_to_string(&self.file).unwrap_or_default();
        let (ts, mut found) = (now_iso(), false);
        let mut out = String::with_capacity(text.len());
        for line in text.lines() {
            match serde_json::from_str::<Value>(line) {
                Ok(mut v) if v["id"] == id => {
                    v["resolved"] = Value::Bool(true);
                    v["resolved_at"] = Value::String(ts.clone());
                    out.push_str(&v.to_string());
                    found = true;
                }
                _ => out.push_str(line),
            }
            out.push('\n');
        }
        if found {
            let tmp = self.file.with_extension("jsonl.tmp");
            std::fs::write(&tmp, out)?;
            std::fs::rename(&tmp, &self.file)?;
        }
        Ok(found)
    }

    /// A `feedback.sh list [--open]` sorai: fejléc, leírás, a kapcsolódó hiba (ha ismert), üres sor.
    pub fn list(&self, open_only: bool, errors: &ErrorLog) -> Vec<String> {
        let index: HashMap<String, _> = errors
            .all()
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect();
        let text = std::fs::read_to_string(&self.file).unwrap_or_default();
        let mut lines = Vec::new();
        for f in text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        {
            let resolved = f["resolved"] == true;
            if open_only && resolved {
                continue;
            }
            let state = if resolved {
                crate::tf!(
                    "  [RESOLVED {}]",
                    "  [MEGOLDVA {}]",
                    f["resolved_at"].as_str().unwrap_or("")
                )
            } else {
                crate::t!("  [OPEN]", "  [NYITOTT]").to_owned()
            };
            let error_id = f["error_id"].as_str().filter(|e| !e.is_empty());
            let link = error_id.map_or(
                crate::t!("  (no error)", "  (nincs error)").to_owned(),
                |e| format!("  error:{e}"),
            );
            lines.push(format!(
                "[{}]  id:{}{state}{link}",
                text_of(&f["ts"]),
                text_of(&f["id"])
            ));
            lines.push(crate::tf!(
                "  description: {}",
                "  leírás: {}",
                text_of(&f["description"])
            ));
            if let Some(e) = error_id.and_then(|e| index.get(e)) {
                lines.push(crate::tf!(
                    "  error: ({}/{}) {}",
                    "  hiba: ({}/{}) {}",
                    e.source,
                    e.context,
                    e.message
                ));
            }
            lines.push(String::new());
        }
        lines
    }

    /// Egy hibához érkezett bejegyzések, nyers (tömörített) JSON-ként.
    pub fn raw_for_error(&self, error_id: &str) -> Vec<String> {
        raw_matching(&self.file, "error_id", error_id)
    }
}

fn text_of(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
}

/// A `key == value` sorok tömörített JSON-ként (a `jq -c 'select(…)'` párja).
pub(super) fn raw_matching(file: &Path, key: &str, value: &str) -> Vec<String> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v[key] == value)
        .map(|v| v.to_string())
        .collect()
}
