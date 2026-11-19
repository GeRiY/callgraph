//! The lines of the `props` view.

use super::{Finding, Kind, VueContract};
use crate::i18n::Lang;

/// Findings listed; the rest is a `+N` line.
const MAX_FINDINGS: usize = 40;

impl VueContract {
    /// The header with the counts and one line per finding; empty when no component use was checked.
    pub(crate) fn lines(&self, lang: Lang) -> Vec<String> {
        if self.checked == 0 {
            return Vec::new();
        }
        let count = |kind: Kind| self.findings.iter().filter(|f| f.kind == kind).count();
        let mut parts = Vec::new();
        for (kind, one, many) in [
            (
                Kind::MissingProp,
                lang.pick("missing required", "hiányzó kötelező"),
                lang.pick("missing required", "hiányzó kötelező"),
            ),
            (
                Kind::UnknownProp,
                lang.pick("unknown prop", "ismeretlen prop"),
                lang.pick("unknown props", "ismeretlen prop"),
            ),
            (
                Kind::UnknownSlot,
                lang.pick("unknown slot", "ismeretlen slot"),
                lang.pick("unknown slots", "ismeretlen slot"),
            ),
        ] {
            match count(kind) {
                0 => {}
                1 => parts.push(format!("1 {one}")),
                n => parts.push(format!("{n} {many}")),
            }
        }
        let tail = if parts.is_empty() {
            lang.pick("no findings", "nincs találat").to_string()
        } else {
            parts.join(", ")
        };
        let mut lines = vec![format!(
            "props: {} {}, {tail}",
            self.checked,
            lang.pick("component uses checked", "komponens-használat ellenőrizve")
        )];
        let mut found: Vec<&Finding> = self.findings.iter().collect();
        found.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        found.dedup();
        lines.extend(
            found
                .iter()
                .take(MAX_FINDINGS)
                .map(|f| finding_line(f, lang)),
        );
        if found.len() > MAX_FINDINGS {
            lines.push(format!("+{} more", found.len() - MAX_FINDINGS));
        }
        lines
    }
}

fn finding_line(f: &Finding, lang: Lang) -> String {
    let what = match f.kind {
        Kind::MissingProp => lang.pick("missing required prop", "hiányzó kötelező prop"),
        Kind::UnknownProp => lang.pick("unknown prop", "ismeretlen prop"),
        Kind::UnknownSlot => lang.pick("unknown slot", "ismeretlen slot"),
    };
    let auto = if f.auto { " (auto)" } else { "" };
    format!(
        "⚠ {what} '{}'  {} L{} <{}>{auto}",
        f.name, f.file, f.line, f.tag
    )
}
