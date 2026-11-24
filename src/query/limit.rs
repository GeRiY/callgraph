//! A callgraph kompatibilis, alapértelmezett kimeneti korlát.

use crate::i18n::Lang;
use std::env;
pub struct OutputLimit {
    pub limit: usize,
    pub full: bool,
    /// A figyelmeztető sorok nyelve (alapból a futásé).
    pub lang: Lang,
}
impl OutputLimit {
    pub fn from_env(full: bool) -> Self {
        let value = env::var("CALLGRAPH_OUT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(400);
        Self {
            limit: value,
            full,
            lang: crate::i18n::lang(),
        }
    }
    pub fn method_index(&self, text: String, index: String) -> String {
        self.method_index_for("outline", text, index)
    }
    /// Egyfájlos tartalék: 400 sor fölött a metódus-index (a figyelmeztetés a tényleges formátumot nevezi meg).
    pub fn method_index_for(&self, format: &str, text: String, index: String) -> String {
        if self.full {
            return text;
        }
        let n = text.lines().count();
        if n <= self.limit {
            return text;
        }
        ltf!(
            self.lang,
            "⚠ --format {format} output would be {n} lines — showing the method index instead. One method: <file>::<method> [--format outline|flow] or --body; full output: --full.\n\n{index}",
            "⚠ A --format {format} kimenete {n} sor lenne — helyette metódus-index. Egy metódus: <fájl>::<metódus> [--format outline|flow] vagy --body; a teljes kimenet: --full.\n\n{index}"
        )
    }
    /// Egyfájlos kimenet: a korlát fölött először a kompakt alak (ha az már belefér), csak utána a metódus-index.
    pub fn compact_or_index(
        &self,
        format: &str,
        text: String,
        compact: impl FnOnce() -> String,
        index: String,
    ) -> String {
        if self.full {
            return text;
        }
        let n = text.lines().count();
        if n <= self.limit {
            return text;
        }
        let compact = compact();
        let m = compact.lines().count();
        if m > self.limit {
            return self.method_index_for(format, text, index);
        }
        ltf!(
            self.lang,
            "⚠ --format {format} output would be {n} lines — compact form ({m} lines: comments dropped, long lists wrapped or capped with '… +N'; every member kept). One method: <file>::<method> [--format outline|flow] or --body; full output: --full.\n\n{compact}",
            "⚠ A --format {format} kimenete {n} sor lenne — kompakt alak ({m} sor: megjegyzések nélkül, a hosszú listák tördelve vagy „… +N”-nel rövidítve; minden tag megvan). Egy metódus: <fájl>::<metódus> [--format outline|flow] vagy --body; a teljes kimenet: --full.\n\n{compact}"
        )
    }
    /// Mappa-szintű `summary`: a korlát fölött a mappánkénti összesítés.
    pub fn summary_or_folders(
        &self,
        text: String,
        folders: impl FnOnce() -> String,
        file_count: usize,
    ) -> String {
        if self.full || text.lines().count() <= self.limit {
            return text;
        }
        let n = text.lines().count();
        let folders = folders();
        ltf!(
            self.lang,
            "⚠ --format summary output would be {n} lines ({file_count} files) — folder totals instead (files, containers, methods per folder; small folders list their files). Narrow it down: a subfolder or a single file; full output: --full.\n\n{folders}",
            "⚠ A --format summary kimenete {n} sor lenne ({file_count} fájl) — helyette mappánkénti összesítés (fájl, konténer, metódus mappánként; a kis mappák felsorolják a fájljaikat). Szűkíts: almappa vagy egy fájl; a teljes kimenet: --full.\n\n{folders}"
        )
    }
    pub fn directory_summary(
        &self,
        text: String,
        summary: String,
        format: &str,
        file_count: usize,
    ) -> String {
        if self.full || text.lines().count() <= self.limit || file_count <= 1 {
            return text;
        }
        let n = text.lines().count();
        ltf!(
            self.lang,
            "⚠ --format {format} output would be {n} lines ({file_count} files) — showing the summary instead. Narrow it down: a subfolder, --grep <pattern>, or a single file; full output: --full.\n\n{summary}",
            "⚠ A --format {format} kimenete {n} sor lenne ({file_count} fájl) — helyette summary. Szűkíts: almappa, --grep <minta>, vagy egy fájl; a teljes kimenet: --full.\n\n{summary}"
        )
    }
    /// `--body`: a korlát fölött metódusonként az első sorok maradnak (a `== …` fejlécek mind), a végén egy figyelmeztetés.
    /// A modul-szintű pszeudo-metódus (`__module__`) törzse külön tippet kap.
    pub fn body(&self, text: String) -> String {
        if self.full {
            return text;
        }
        let total = text.lines().count();
        if total <= self.limit {
            return text;
        }
        // szakaszok: egy `== ` fejléctől a következőig
        let mut sections: Vec<Vec<&str>> = Vec::new();
        for line in text.lines() {
            if line.starts_with("== ") || sections.is_empty() {
                sections.push(Vec::new());
            }
            if let Some(last) = sections.last_mut() {
                last.push(line);
            }
        }
        let mut left = self.limit.max(sections.len());
        let mut out = String::new();
        let mut shown = 0usize;
        let mut module_level = false;
        let count = sections.len();
        for (i, section) in sections.iter().enumerate() {
            // a maradék keret egyenlően a hátralévő szakaszok közt (a rövidek maradéka a hosszabbaké)
            let share = (left / (count - i)).max(1);
            let take = section.len().min(share);
            for line in &section[..take] {
                out.push_str(line);
                out.push('\n');
            }
            if take < section.len() && section[0].contains("::__module__") {
                module_level = true;
            }
            shown += take;
            left = left.saturating_sub(take);
        }
        let hint = if module_level {
            ltf!(
                self.lang,
                " __module__ is module-level code, not one method: pick a range with --lines A-B.",
                " A __module__ modul-szintű kód, nem egy metódus: válassz sávot a --lines A-B kapcsolóval."
            )
        } else {
            String::new()
        };
        let warning = ltf!(
            self.lang,
            "⚠ --body output would be {total} lines — showing the first {shown}. A line range: --lines A-B; full output: --full.{hint}",
            "⚠ A --body kimenete {total} sor lenne — az első {shown} sor látszik. Sáv: --lines A-B; a teljes kimenet: --full.{hint}"
        );
        out.push_str(&warning);
        out.push('\n');
        out
    }
    pub fn with_value(mut self, value: Option<usize>) -> Self {
        if let Some(value) = value {
            self.limit = value;
        }
        self
    }
    /// A figyelmeztetések nyelve.
    pub fn with_lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }
}
